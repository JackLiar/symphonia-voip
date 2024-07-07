use std::cmp::Ordering;
use std::io::{Error, ErrorKind, Result, Write};

use bitvec::prelude::*;
use byteorder::ReadBytesExt;
use symphonia_core::codecs::CodecType;

use crate::{CODEC_TYPE_AMR, CODEC_TYPE_AMRWB};

const AMR_PAYLOAD_SIZES: &[usize] = &[13, 14, 16, 18, 20, 21, 27, 32, 6];
const AMR_PAYLOAD_BE_BIT_SIZES: &[usize] = &[95, 103, 118, 134, 148, 159, 204, 244, 39];
const AMRWB_PAYLOAD_SIZES: &[usize] = &[18, 24, 33, 37, 41, 47, 51, 59, 61, 7];
const AMRWB_PAYLOAD_BE_BIT_SIZES: &[usize] = &[132, 177, 253, 285, 317, 365, 397, 461, 477, 40];

#[derive(Clone, Debug)]
pub enum Frame<'a> {
    Octect(&'a [u8]),
    Bits(&'a BitSlice<u8, Msb0>),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Toc(u8);

impl Toc {
    pub fn followed_by_another_speech_frame(&self) -> bool {
        (self.0 >> 7) == 1
    }

    pub fn bit_rate_idx(&self) -> usize {
        ((self.0 >> 3) & 0x0f) as usize
    }

    pub fn bit_rate(&self, sizes: &[usize]) -> Option<usize> {
        sizes.get(self.bit_rate_idx()).copied()
    }

    pub fn ok(&self) -> bool {
        ((self.0 >> 2) & 1) == 1
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameHeader(pub u8);

impl FrameHeader {
    pub fn frame_type(&self) -> u8 {
        (self.0 >> 3) & 0b01111
    }

    pub fn quality(&self) -> bool {
        ((self.0 >> 2) & 0b1) == 1
    }
}

impl From<Toc> for FrameHeader {
    fn from(toc: Toc) -> Self {
        Self(toc.0 & 0b01111100)
    }
}

fn parse_bandwidth_efficient<'a>(
    data: &'a [u8],
    bit_size: &'static [usize],
) -> Result<(Vec<(Toc, &'a BitSlice<u8, Msb0>)>, &'a BitSlice<u8, Msb0>)> {
    let data: &BitSlice<u8, Msb0> = unsafe { std::mem::transmute(data.as_bits::<Msb0>()) };
    let mut data = &data[4..];
    let mut tocs = vec![];
    let mut frm_sizes = vec![];
    loop {
        if data.len() < 6 {
            return Err(Error::new(ErrorKind::UnexpectedEof, "Expecting 6 bytes"));
        }

        let (toc, rem) = data.split_at(6);
        let toc = Toc(toc.load_be::<u8>() << 2);
        data = rem;
        match toc.bit_rate(bit_size) {
            None => return Err(Error::new(ErrorKind::InvalidData, "Invalid bit rate")),
            Some(br) => {
                tocs.push(toc);
                frm_sizes.push(br);
            }
        };
        if !toc.followed_by_another_speech_frame() {
            break;
        }
    }

    let mut frames = vec![];

    let expected_size = frm_sizes.iter().sum::<usize>();
    match data.len().cmp(&expected_size) {
        Ordering::Equal | Ordering::Greater => {}
        Ordering::Less => return Err(Error::new(ErrorKind::UnexpectedEof, "Expecting 6 bytes")),
    };

    for (toc, size) in tocs.into_iter().zip(frm_sizes) {
        let (frame, rem) = data.split_at(size);
        frames.push((toc, frame));
        data = rem;
    }

    if !data.not_any() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Invalid padding bits, not all zero",
        ));
    }

    Ok((frames, data))
}

fn parse_octect_aligned<'a>(
    mut data: &'a [u8],
    payload_size: &'static [usize],
) -> Result<(Vec<(Toc, &'a [u8])>, &'a [u8])> {
    let org_data = data;
    let first_byte = data.read_u8()?;
    // let (first_byte, mut data) = take(1).map(|b: &[u8]| b[0]).parse(data)?;
    if first_byte != 0xf0 {
        return Err(Error::new(ErrorKind::InvalidData, "Not octet aligned mode"));
    }

    // Parse payload table of content
    let mut tocs = vec![];
    let mut frm_sizes = vec![];
    loop {
        let toc = Toc(data.read_u8()?);
        // let (toc, rem) = take(1).map(|b: &[u8]| Toc(b[0])).parse(data)?;
        // data = rem;
        match toc.bit_rate(payload_size) {
            None => return Err(Error::new(ErrorKind::InvalidData, "Invalid bit rate")),
            Some(br) => {
                tocs.push(toc);
                frm_sizes.push(br)
            }
        };
        if !toc.followed_by_another_speech_frame() {
            break;
        }
    }

    let expected_size = frm_sizes.iter().sum::<usize>() + 1;
    match org_data.len().cmp(&expected_size) {
        Ordering::Equal => {}
        Ordering::Greater => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Have extra tailing data",
            ))
        }
        Ordering::Less => return Err(Error::new(ErrorKind::UnexpectedEof, "Data not enough")),
    };

    let mut frames = vec![];
    for (toc, size) in tocs.into_iter().zip(frm_sizes) {
        if data.len() >= size {
            frames.push((toc, &data[..size]));
            data = &data[size..];
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                format!("Expecting {} bytes, {} remain", size, data.len()),
            ));
        }
        // let (frame, rem) = take(size - 1).parse(data)?;
        // frames.push((toc, frame));
        // data = rem;
    }

    Ok((frames, data))
}

pub fn parse_amr_oa(buf: &[u8]) -> Result<Vec<(Toc, Frame)>> {
    parse_octect_aligned(buf, AMR_PAYLOAD_SIZES).map(|(v, _)| {
        v.into_iter()
            .map(|(toc, f)| (toc, Frame::Octect(f)))
            .collect()
    })
}

pub fn parse_amr_be(buf: &[u8]) -> Result<Vec<(Toc, Frame)>> {
    parse_bandwidth_efficient(buf, AMR_PAYLOAD_BE_BIT_SIZES).map(|(v, _)| {
        v.into_iter()
            .map(|(toc, f)| (toc, Frame::Bits(f)))
            .collect()
    })
}

pub fn parse_amrwb_oa(buf: &[u8]) -> Result<Vec<(Toc, Frame)>> {
    parse_octect_aligned(buf, AMRWB_PAYLOAD_SIZES).map(|(v, _)| {
        v.into_iter()
            .map(|(toc, f)| (toc, Frame::Octect(f)))
            .collect()
    })
}

pub fn parse_amrwb_be(buf: &[u8]) -> Result<Vec<(Toc, Frame)>> {
    parse_bandwidth_efficient(buf, AMRWB_PAYLOAD_BE_BIT_SIZES).map(|(v, _)| {
        v.into_iter()
            .map(|(toc, f)| (toc, Frame::Bits(f)))
            .collect()
    })
}

pub fn on_amr_amrwb_oa(r: &mut dyn Write, rtp: &[u8], codec: CodecType) -> Result<()> {
    let toc_frames = match codec {
        CODEC_TYPE_AMR => parse_amr_oa(rtp)?,
        CODEC_TYPE_AMRWB => parse_amrwb_oa(rtp)?,
        _ => unreachable!(),
    };

    for (toc, frame) in toc_frames {
        let fhdr = FrameHeader::from(toc);
        r.write_all(&[fhdr.0])?;
        match frame {
            Frame::Octect(octect) => {
                r.write_all(octect)?;
            }
            Frame::Bits(_) => unreachable!(),
        }
    }
    Ok(())
}

pub fn on_amr_amrwb_be(r: &mut dyn Write, rtp: &[u8], codec: CodecType) -> Result<()> {
    let toc_frames = match codec {
        CODEC_TYPE_AMR => parse_amr_be(rtp)?,
        CODEC_TYPE_AMRWB => parse_amrwb_be(rtp)?,
        _ => unreachable!(),
    };

    for (toc, frame) in toc_frames {
        let fhdr = FrameHeader::from(toc);
        r.write_all(&[fhdr.0])?;
        match frame {
            Frame::Bits(bits) => {
                let mut data = bits.to_owned();
                data.force_align();
                let data = data.into_vec();
                r.write_all(&data)?;
            }
            Frame::Octect(_) => unreachable!(),
        }
    }

    Ok(())
}

pub fn is_amr(data: &[u8]) -> bool {
    if let Ok(toc_frames) = parse_amr_oa(data) {
        if !toc_frames.is_empty() {
            return true;
        }
    } else if let Ok(toc_frames) = parse_amr_be(data) {
        if !toc_frames.is_empty() {
            return true;
        }
    }
    false
}

pub fn is_amrwb(data: &[u8]) -> bool {
    if let Ok(toc_frames) = parse_amrwb_oa(data) {
        if !toc_frames.is_empty() {
            return true;
        }
    } else if let Ok(toc_frames) = parse_amrwb_be(data) {
        if !toc_frames.is_empty() {
            return true;
        }
    }
    false
}
