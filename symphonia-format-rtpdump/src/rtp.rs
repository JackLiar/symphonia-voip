use std::fmt::Display;
use std::ops::{Add, Sub};

use anyhow::{anyhow, bail, Result};
use combine::error::UnexpectedParse;
use combine::parser::byte::num::be_u16;
use combine::parser::byte::{byte, bytes};
use combine::parser::range::take;
use combine::parser::repeat::skip_many;
use combine::{look_ahead, many1, Parser};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::Serialize;
use symphonia_core::codecs::{
    CodecParameters, CodecType, CODEC_TYPE_PCM_ALAW, CODEC_TYPE_PCM_MULAW,
};
use symphonia_core::errors::unsupported_error;

use symphonia_bundle_amr::rtp::{on_amr_amrwb_be, on_amr_amrwb_oa};
use symphonia_bundle_amr::{DecoderParams as AMRDecodeParams, CODEC_TYPE_AMR, CODEC_TYPE_AMRWB};
use symphonia_bundle_evs::dec::CODEC_TYPE_EVS;
use symphonia_codec_g722::CODEC_TYPE_G722;
use symphonia_codec_g7221::CODEC_TYPE_G722_1;

use crate::codec_detector::Codec;
use crate::utils::bytes_to_struct;

pub fn codec_to_codec_type(codec: &Codec) -> Option<CodecType> {
    let ct = match codec.name.to_lowercase().as_str() {
        "amr" => CODEC_TYPE_AMR,
        "amrwb" => CODEC_TYPE_AMRWB,
        "evs" => CODEC_TYPE_EVS,
        "g.722" => CODEC_TYPE_G722,
        "g.722.1" => CODEC_TYPE_G722_1,
        "pcma" => CODEC_TYPE_PCM_ALAW,
        "pcmu" => CODEC_TYPE_PCM_MULAW,
        _ => return None,
    };
    Some(ct)
}

pub fn parse_rtp_payload<R: RtpPacket>(
    params: &CodecParameters,
    rtp: &R,
) -> symphonia_core::errors::Result<Vec<u8>> {
    match params.codec {
        CODEC_TYPE_G722_1 | CODEC_TYPE_G722 | CODEC_TYPE_PCM_ALAW | CODEC_TYPE_PCM_MULAW => {
            return Ok(rtp.payload().to_vec())
        }
        CODEC_TYPE_AMR | CODEC_TYPE_AMRWB => {
            let param: AMRDecodeParams = params
                .extra_data
                .as_ref()
                .map(|d| bytes_to_struct(d))
                .unwrap_or_default();
            let mut pkt = vec![];
            if param.octet_align {
                on_amr_amrwb_oa(&mut pkt, rtp.payload(), params.codec)?;
                Ok(pkt)
            } else {
                on_amr_amrwb_be(&mut pkt, rtp.payload(), params.codec)?;
                Ok(pkt)
            }
        }
        CODEC_TYPE_EVS => {
            let mut pkt = vec![];
            symphonia_bundle_evs::rtp::on_evs(&mut pkt, rtp.payload())?;
            Ok(pkt)
        }
        _ => return unsupported_error("Unsupport codec"),
    }
}

#[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
#[repr(transparent)]
pub struct SeqNum(pub u16);

impl Add for SeqNum {
    type Output = u16;

    fn add(self, rhs: Self) -> Self::Output {
        let (seq, _) = self.0.overflowing_add(rhs.0);
        seq
    }
}

impl Sub for SeqNum {
    type Output = u16;

    fn sub(self, rhs: Self) -> Self::Output {
        let (seq, _) = self.0.overflowing_sub(rhs.0);
        seq
    }
}

impl From<u16> for SeqNum {
    fn from(x: u16) -> Self {
        Self(x)
    }
}

impl From<SeqNum> for u16 {
    fn from(x: SeqNum) -> Self {
        x.0
    }
}

/// RTP payload type, range from 0~127
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum PayloadType {
    #[default]
    PCMU = 0,
    CELP = 1,
    G721 = 2,
    GSM = 3,
    G723 = 4,
    DVI4_8000 = 5,
    DVI4_16000 = 6,
    LPC = 7,
    PCMA = 8,
    G722 = 9,
    L16_44100_2 = 10,
    L16_44100_1 = 11,
    QCELP = 12,
    CN = 13,
    MPA = 14,
    G728 = 15,
    DVI4_11025 = 16,
    DVI4_22050 = 17,
    G729 = 18,
    CELB = 25,
    JPEG = 26,
    NV = 28,
    H261 = 31,
    MPV = 32,
    MP2T = 33,
    H263 = 34,
    Reserved(u8),
    Dynamic(u8),
    Unassigned(u8),
}

impl PayloadType {
    pub fn is_dynamic(self) -> bool {
        matches!(self, Self::Dynamic(_))
    }
}

impl From<PayloadType> for u8 {
    fn from(val: PayloadType) -> Self {
        match val {
            PayloadType::PCMU => 0,
            PayloadType::CELP => 1,
            PayloadType::G721 => 2,
            PayloadType::GSM => 3,
            PayloadType::G723 => 4,
            PayloadType::DVI4_8000 => 5,
            PayloadType::DVI4_16000 => 6,
            PayloadType::LPC => 7,
            PayloadType::PCMA => 8,
            PayloadType::G722 => 9,
            PayloadType::L16_44100_2 => 10,
            PayloadType::L16_44100_1 => 11,
            PayloadType::QCELP => 12,
            PayloadType::CN => 13,
            PayloadType::MPA => 14,
            PayloadType::G728 => 15,
            PayloadType::DVI4_11025 => 16,
            PayloadType::DVI4_22050 => 17,
            PayloadType::G729 => 18,
            PayloadType::CELB => 25,
            PayloadType::JPEG => 26,
            PayloadType::NV => 28,
            PayloadType::H261 => 31,
            PayloadType::MPV => 32,
            PayloadType::MP2T => 33,
            PayloadType::H263 => 34,
            PayloadType::Reserved(t) | PayloadType::Dynamic(t) | PayloadType::Unassigned(t) => t,
        }
    }
}

impl From<u8> for PayloadType {
    fn from(value: u8) -> Self {
        match value & 0x7f {
            0 => Self::PCMU,
            3 => Self::GSM,
            4 => Self::G723,
            5 => Self::DVI4_8000,
            6 => Self::DVI4_16000,
            7 => Self::LPC,
            8 => Self::PCMA,
            9 => Self::G722,
            10 => Self::L16_44100_2,
            11 => Self::L16_44100_1,
            12 => Self::QCELP,
            13 => Self::CN,
            14 => Self::MPA,
            15 => Self::G728,
            16 => Self::DVI4_11025,
            17 => Self::DVI4_22050,
            18 => Self::G729,
            25 => Self::CELB,
            26 => Self::JPEG,
            28 => Self::NV,
            31 => Self::H261,
            32 => Self::MPV,
            33 => Self::MP2T,
            34 => Self::H263,
            t if t == 1 || t == 2 || t == 19 => Self::Reserved(t),
            t if (72..=76).contains(&t) => Self::Reserved(t),
            t if (96..=127).contains(&t) => Self::Dynamic(t),
            t => Self::Unassigned(t),
        }
    }
}

impl Display for PayloadType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::PCMU => "PCMU",
            Self::CELP => "CELP",
            Self::G721 => "G721",
            Self::GSM => "GSM",
            Self::G723 => "G723",
            Self::DVI4_8000 => "DVI4",
            Self::DVI4_16000 => "DVI4",
            Self::LPC => "LPC",
            Self::PCMA => "PCMA",
            Self::G722 => "G722",
            Self::L16_44100_2 => "L16",
            Self::L16_44100_1 => "L16",
            Self::QCELP => "QCELP",
            Self::CN => "CN",
            Self::MPA => "MPA",
            Self::G728 => "G728",
            Self::DVI4_11025 => "DVI4",
            Self::DVI4_22050 => "DVI4",
            Self::G729 => "G729",
            Self::CELB => "CelB",
            Self::JPEG => "JPEG",
            Self::NV => "NV",
            Self::H261 => "H261",
            Self::MPV => "MPV",
            Self::MP2T => "MP2T",
            Self::H263 => "H263",
            Self::Dynamic(t) => return format!("DYNAMIC-{}", t).fmt(f),
            Self::Reserved(t) => return format!("RESERVED-{}", t).fmt(f),
            Self::Unassigned(t) => return format!("UNASSIGNED-{}", t).fmt(f),
        }
        .fmt(f)
    }
}

impl Serialize for PayloadType {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Extension<'a> {
    pub id: u8,
    pub value: &'a [u8],
}

pub trait RtpPacket {
    fn raw(&self) -> &[u8];
    fn version(&self) -> u8 {
        (self.raw()[0] & 0b1100_0000) >> 6
    }

    fn padding(&self) -> bool {
        (self.raw()[0] & 0b0010_0000) == 0b0010_0000
    }

    fn extension(&self) -> bool {
        (self.raw()[0] & 0b0001_0000) == 0b0001_0000
    }

    fn csi_cnt(&self) -> usize {
        (self.raw()[0] & 0x0f) as usize
    }

    fn marked(&self) -> bool {
        (self.raw()[1] & 0x80) == 0x80
    }

    fn payload_type(&self) -> PayloadType {
        PayloadType::from(self.raw()[1])
    }

    fn seq(&self) -> u16 {
        match <&[u8; 2]>::try_from(&self.raw()[2..4]) {
            Ok(seq) => u16::from_be_bytes(*seq),
            Err(_) => unreachable!(),
        }
    }

    fn ts(&self) -> u32 {
        match <&[u8; 4]>::try_from(&self.raw()[4..8]) {
            Ok(seq) => u32::from_be_bytes(*seq),
            Err(_) => unreachable!(),
        }
    }

    fn ssrc(&self) -> u32 {
        match <&[u8; 4]>::try_from(&self.raw()[8..12]) {
            Ok(seq) => u32::from_be_bytes(*seq),
            Err(_) => unreachable!(),
        }
    }

    fn payload(&self) -> &[u8] {
        let mut buf = if !self.extension() {
            &self.raw()[12..]
        } else {
            let mut offset = 12 + 2;
            let ext_len = match <&[u8; 2]>::try_from(&self.raw()[offset..offset + 2]) {
                Ok(seq) => u16::from_be_bytes(*seq) as usize,
                Err(_) => unreachable!(),
            } * 4;
            offset += ext_len;
            &self.raw()[offset..]
        };

        if self.padding() {
            if let Some(padding_len) = buf.last() {
                buf = &buf[0..(buf.len() - (*padding_len as usize))];
            }
        }

        buf
    }

    fn get_extensions(&self) -> Result<Option<Vec<()>>> {
        if !self.extension() {
            return Ok(None);
        }

        match look_ahead(bytes(b"\xbe\xde")).parse(&self.raw()[12..]) {
            Ok((_, rem)) => {
                // One byte header extensions
                let (exts, _) = take(2)
                    .and(be_u16())
                    .then(|(_magic, len)| {
                        take(len as usize * 4).and_then(|a: &[u8]| {
                            if !a.is_empty() {
                                let ext_parser = take(1)
                                    .map(|b: &[u8]| (b[0] & 0xf0, (b[0] & 0x0f) + 1))
                                    .then(|(id, len)| take(len as usize + 1).map(move |r| (id, r)))
                                    .skip(skip_many(byte(0x00)))
                                    .map(|(id, value)| Extension { id, value });
                                many1::<Vec<_>, _, _>(ext_parser)
                                    .parse(a)
                                    .map(|(exts, _)| exts)
                            } else {
                                Ok(vec![])
                            }
                        })
                    })
                    .parse(rem)?;
                exts
            }
            Err(UnexpectedParse::Unexpected) => {
                // Two byte header extensions
                let (exts, _) = take(2)
                    .and(be_u16())
                    .then(|(_magic, len)| {
                        take(len as usize * 4).and_then(|a: &[u8]| {
                            if !a.is_empty() {
                                let ext_parser = take(1)
                                    .and(take(1))
                                    .map(|(id, len): (&[u8], &[u8])| (id[0], len[0] as usize))
                                    .then(|(id, len)| take(len).map(move |r| (id, r)))
                                    .skip(skip_many(byte(0x00)))
                                    .map(|(id, value)| Extension { id, value });
                                many1::<Vec<_>, _, _>(ext_parser)
                                    .parse(a)
                                    .map(|(exts, _)| exts)
                            } else {
                                Ok(vec![])
                            }
                        })
                    })
                    .parse(&self.raw()[12..])?;
                exts
            }
            Err(UnexpectedParse::Eoi) => unreachable!(),
        };
        todo!()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RawRtpPacket<'a> {
    raw: &'a [u8],
}

impl<'a> RtpPacket for RawRtpPacket<'a> {
    fn raw(&self) -> &[u8] {
        self.raw
    }
}

impl<'a> RawRtpPacket<'a> {
    pub fn new(raw: &'a [u8]) -> Self {
        Self { raw }
    }
}

pub fn parse_rtp(data: &[u8]) -> Result<RawRtpPacket> {
    let (_hdr, mut rem) = take(12).parse(data)?;

    let pkt = RawRtpPacket { raw: data };
    if pkt.extension() {
        let (_exts, r) = take(2)
            .and(be_u16())
            .then(|(_magic, len)| take(len as usize * 4))
            .parse(rem)?;
        rem = r;
    }

    if pkt.padding() {
        let len = match rem.last() {
            None => bail!("Invalid RTP Packet: no payload avaliable"),
            Some(l) => *l as usize,
        };
        if len >= rem.len() {
            bail!("Invalid RTP Packet: padding is longer than payload len");
        } else {
            rem = &rem[0..rem.len() - 1 - len];
        }
    }

    if rem.is_empty() {
        bail!("Invalid RTP Packet: no payload avaliable");
    }

    Ok(pkt)
}

/// Detect whether a packet is not a RTP packet
pub fn detect_not_rtp(data: &[u8], ssrcs: &[u32]) -> bool {
    if data.is_empty() {
        return true;
    }

    if data[0] < 0x80 || data[0] > 0xbf {
        return true;
    }

    if data.len() >= 8 && data[4..8] == [0x21, 0x12, 0xa4, 0x42] {
        // skip STUN packets
        return true;
    }

    if data.len() >= 8 {
        // skip RTCP packets
        let ssrc = ((data[4] as u32) << 24)
            | ((data[5] as u32) << 16)
            | ((data[6] as u32) << 8)
            | (data[7] as u32);
        if ssrcs.contains(&ssrc) {
            return true;
        }
    }

    false
}

#[derive(Clone, Copy, Debug, Default, Eq, FromPrimitive, Hash, PartialEq)]
#[repr(u8)]
pub enum EventCode {
    #[default]
    DTMF0 = 0,
    DTMF1 = 1,
    DTMF2 = 2,
    DTMF3 = 3,
    DTMF4 = 4,
    DTMF5 = 5,
    DTMF6 = 6,
    DTMF7 = 7,
    DTMF8 = 8,
    DTMF9 = 9,
    Star = 10,
    Pound = 11,
    A = 12,
    B = 13,
    C = 14,
    D = 15,
    Flash = 16,
}

impl Display for EventCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DTMF0 => "DTMF 0",
            Self::DTMF1 => "DTMF 1",
            Self::DTMF2 => "DTMF 2",
            Self::DTMF3 => "DTMF 3",
            Self::DTMF4 => "DTMF 4",
            Self::DTMF5 => "DTMF 5",
            Self::DTMF6 => "DTMF 6",
            Self::DTMF7 => "DTMF 7",
            Self::DTMF8 => "DTMF 8",
            Self::DTMF9 => "DTMF 9",
            Self::Star => "DTMF *",
            Self::Pound => "DTMF #",
            Self::A => "DTMF A",
            Self::B => "DTMF B",
            Self::C => "DTMF C",
            Self::D => "DTMF D",
            Self::Flash => "Flash",
        }
        .fmt(f)
    }
}

impl Serialize for EventCode {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RtpEvent {
    pub event_id: EventCode,
    pub flags: u8,
    pub duration: u16,
}

impl RtpEvent {
    pub fn is_end_of_event(&self) -> bool {
        self.flags & 0b10000000 == 0b10000000
    }
}

/// Parse RTP event ID heuristically
pub fn parse_rtp_event(data: &[u8]) -> Result<RtpEvent> {
    let (((event_id, flags), duration), rem) = take(1).and(take(1)).and(be_u16()).parse(data)?;
    let event_id = EventCode::from_u8(event_id[0]).ok_or_else(|| anyhow!("Invalid RTP EventID"))?;
    if !rem.is_empty() {
        bail!("Payload type is not RTP Event");
    }
    Ok(RtpEvent {
        event_id,
        flags: flags[0],
        duration,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rtp() -> Result<()> {
        let data: &[u8] = &[
            0x80, 0x7f, 0x00, 0x02, 0x08, 0x37, 0x76, 0x60, 0x00, 0x84, 0x1a, 0xa8, 0x8b, 0x73,
            0x6f, 0xf5, 0x58, 0x4a, 0xc0, 0x90, 0x44, 0xc4, 0x50, 0x16, 0x03, 0xd8, 0x07, 0xfe,
            0x19, 0x2b, 0x80, 0x28, 0x02, 0x00, 0x80, 0x00, 0x16, 0x70, 0x90, 0x5c, 0x69, 0xdc,
            0xf0, 0xa9, 0x5c,
        ];
        let rtp = parse_rtp(data)?;
        assert!(!rtp.marked());
        assert_eq!(rtp.payload_type(), PayloadType::Dynamic(127));
        assert_eq!(rtp.seq(), 0x0002);
        assert_eq!(rtp.ts(), 0x08377660);
        assert_eq!(rtp.ssrc(), 0x00841aa8);
        assert_eq!(rtp.payload().len(), 33);
        Ok(())
    }

    #[test]
    fn test_seq_num() -> Result<()> {
        let seq1 = SeqNum(1);
        let seq2 = SeqNum(2);
        assert_eq!(seq2 - seq1, 1);

        let seq1 = SeqNum(2);
        let seq2 = SeqNum(2);
        assert_eq!(seq2 - seq1, 0);

        let seq1 = SeqNum(3);
        let seq2 = SeqNum(2);
        assert_eq!(seq2 - seq1, 65535);

        let seq1 = SeqNum(65535);
        let seq2 = SeqNum(0);
        assert_eq!(seq2 - seq1, 1);

        let seq1 = SeqNum(0);
        let seq2 = SeqNum(65535);
        assert_eq!(seq2 - seq1, 65535);
        Ok(())
    }
}
