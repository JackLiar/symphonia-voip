use std::io::{Error as IOError, ErrorKind, Read, Seek, SeekFrom};
use std::net::Ipv4Addr;
use std::path::Path;
use std::str::FromStr;

use binrw::{BinRead, BinResult};
use codec_detector::rtp::RawRtpPacket;
use codec_detector::{Codec, CodecDetector};
use symphonia_core::audio::Channels;
use symphonia_core::codecs::{CodecParameters, CODEC_TYPE_PCM_ALAW, CODEC_TYPE_PCM_MULAW};
use symphonia_core::errors::{seek_error, Error, Result, SeekErrorKind};
use symphonia_core::formats::{
    Cue, FormatOptions, FormatReader, Packet, SeekMode, SeekTo, SeekedTo, Track,
};
use symphonia_core::io::{MediaSourceStream, ReadBytes};
use symphonia_core::meta::{Metadata, MetadataLog};
use symphonia_core::probe::{Descriptor, Instantiate, QueryDescriptor};
use symphonia_core::support_format;
use symphonia_core::units::TimeBase;

use symphonia_bundle_amr::{CODEC_TYPE_AMR, CODEC_TYPE_AMRWB};
use symphonia_bundle_evs::dec::CODEC_TYPE_EVS;
use symphonia_codec_g7221::CODEC_TYPE_G722_1;

const MAGIC: &[u8] = b"#!rtpplay1.0 ";

#[binrw::parser(reader, endian)]
fn parse_src_ip() -> BinResult<Ipv4Addr> {
    let pos = reader.stream_position()?;
    let ip: &mut [u8] = &mut [0; 16];
    let mut len = 0;

    for c in ip.iter_mut() {
        let char = &mut [0];
        reader.read_exact(char)?;
        if char[0] == b'/' {
            break;
        }
        *c = char[0];
        len += 1;
    }

    Ipv4Addr::from_str(&String::from_utf8_lossy(&ip[..len])).map_err(|e| binrw::Error::Custom {
        pos,
        err: Box::new(e),
    })
}
#[binrw::parser(reader, endian)]
fn parse_src_port() -> BinResult<u16> {
    let pos = reader.stream_position()?;
    let port: &mut [u8] = &mut [0; 6];
    let mut len = 0;

    for c in port.iter_mut() {
        let char = &mut [0];
        reader.read_exact(char)?;
        if char[0] == b'\n' {
            break;
        }
        *c = char[0];
        len += 1;
    }

    String::from_utf8_lossy(&port[..len])
        .parse::<u16>()
        .map_err(|e| binrw::Error::Custom {
            pos,
            err: Box::new(e),
        })
}

#[derive(BinRead, Clone, Copy, Debug)]
#[br(big, magic = b"#!rtpplay1.0 ")]
#[repr(C)]
pub struct FileHeader {
    #[br(parse_with = parse_src_ip)]
    pub ip: Ipv4Addr,
    #[br(parse_with = parse_src_port)]
    pub port: u16,
    pub start_sec: u32,
    pub start_usec: u32,
    pub ip2: u32,
    pub port2: u16,
    pub padding: u16,
}

impl Default for FileHeader {
    fn default() -> Self {
        Self {
            ip: Ipv4Addr::new(0, 0, 0, 0),
            port: 0,
            start_sec: 0,
            start_usec: 0,
            ip2: 0,
            port2: 0,
            padding: 0,
        }
    }
}

#[derive(BinRead, Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct RDPacket {
    /// length of packet, including this header (may be smaller than plen if not whole packet recorded)
    pub len: u16,
    /// actual header+payload length for RTP, 0 for RTCP
    pub org_len: u16,
    /// milliseconds since the start of recording
    pub offset: u32,
}

pub struct RtpdumpReader {
    reader: MediaSourceStream,
    tracks: Vec<Track>,
    track_ts: Vec<u64>,
    cues: Vec<Cue>,
    metadata: MetadataLog,
    ssrcs: Vec<u32>,
    track_idx: usize,
    pkt_cnt: u64,
    pub sample_rate: Option<u32>,
    pub timestamp_interval: u64,
}

impl QueryDescriptor for RtpdumpReader {
    fn query() -> &'static [symphonia_core::probe::Descriptor] {
        &[support_format!(
            "rtpdump",
            "rtpdump",
            &["rtpdump"],
            &["audio/rtpdump"],
            &[MAGIC]
        )]
    }

    fn score(_context: &[u8]) -> u8 {
        255
    }
}

fn read_rd_pkt(source: &mut MediaSourceStream) -> Result<Box<[u8]>> {
    let len = source.read_be_u16()?;
    let org_len = source.read_be_u16()?;
    let offset = source.read_be_u32()?;
    let pkt = RDPacket {
        len,
        org_len,
        offset,
    };
    Ok(source.read_boxed_slice_exact(pkt.org_len as usize)?)
}

fn codec_to_param(codec: &Codec) -> Option<CodecParameters> {
    let mut params = CodecParameters::new();
    params
        .with_sample_rate(codec.sample_rate)
        .with_time_base(TimeBase::new(1, codec.sample_rate))
        .with_channels(Channels::FRONT_CENTRE);

    if let Some(br) = codec.bit_rate {
        params.with_bits_per_sample(br);
    }
    if let Some(frames) = codec.max_frames_per_packet {
        params.with_max_frames_per_packet(160);
    }

    params.codec = match codec.name.as_str() {
        "amr" => CODEC_TYPE_AMR,
        "amrwb" => CODEC_TYPE_AMRWB,
        "evs" => CODEC_TYPE_EVS,
        "G.722.1" => CODEC_TYPE_G722_1,
        "pcma" => CODEC_TYPE_PCM_ALAW,
        "pcmu" => CODEC_TYPE_PCM_MULAW,
        _ => return None,
    };
    Some(params)
}

impl FormatReader for RtpdumpReader {
    fn try_new(mut source: MediaSourceStream, options: &FormatOptions) -> Result<Self>
    where
        Self: Sized,
    {
        let _hdr = match FileHeader::read(&mut source) {
            Ok(hdr) => hdr,
            Err(binrw::Error::Io(e)) => return Err(Error::IoError(e)),
            Err(_) => return Err(Error::DecodeError("Failed to decode rtpdump header")),
        };
        let hdr_len = source.pos();

        let mut r = Self {
            reader: source,
            tracks: vec![],
            track_ts: vec![],
            cues: vec![],
            metadata: Default::default(),
            ssrcs: vec![],
            track_idx: 0,
            pkt_cnt: 0,
            sample_rate: None,
            timestamp_interval: 320,
        };

        let mut detector = CodecDetector::new();
        detector.get_features_from_yaml(Path::new("codec.yaml"));
        loop {
            let pkt = match read_rd_pkt(&mut r.reader) {
                Ok(pkt) => pkt,
                Err(Error::IoError(e)) => {
                    if e.kind() == ErrorKind::UnexpectedEof {
                        break;
                    } else {
                        return Err(Error::IoError(e));
                    }
                }
                Err(e) => return Err(e),
            };
            let pkt = RawRtpPacket::new(pkt.as_ref());
            detector.on_pkt(&pkt);
        }

        let result = detector.get_result();

        r.reader.seek(SeekFrom::Start(hdr_len))?;
        for (id, (pt, codec)) in result.iter().enumerate() {
            let param =
                codec_to_param(&codec).ok_or_else(|| Error::Unsupported("Unsupported codec"))?;
            r.tracks.push(Track::new(id as u32, param));
            r.track_ts.push(0);
        }
        Ok(r)
    }

    fn next_packet(&mut self) -> Result<Packet> {
        let len = self.reader.read_be_u16()?;
        let org_len = self.reader.read_be_u16()?;
        let offset = self.reader.read_be_u32()?;
        let pkt = RDPacket {
            len,
            org_len,
            offset,
        };
        let data = self.reader.read_boxed_slice_exact(pkt.org_len as usize)?;
        let track = &self.tracks()[self.track_idx];

        let sr = track
            .codec_params
            .sample_rate
            .ok_or_else(|| Error::Unsupported("Unknown sample rate"))?;

        let pkt = Packet::new_from_slice(
            self.track_idx as u32,
            self.track_ts[self.track_idx] * (sr as u64) / 50,
            (sr / 50) as u64,
            &data[12..],
        );
        self.track_ts[self.track_idx] += 1;
        Ok(pkt)
    }

    fn metadata(&mut self) -> Metadata<'_> {
        self.metadata.metadata()
    }

    fn cues(&self) -> &[Cue] {
        &self.cues
    }

    fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    fn seek(&mut self, mode: SeekMode, to: SeekTo) -> Result<SeekedTo> {
        if self.tracks.is_empty() {
            return seek_error(SeekErrorKind::Unseekable);
        }

        unimplemented!()
    }

    fn into_inner(self: Box<Self>) -> MediaSourceStream {
        self.reader
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn read_rtpdump_header() {
        let header = b"#!rtpplay1.0 192.168.1.1/12345";
    }
}
