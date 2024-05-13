use std::io::{Error as IOError, Read};
use std::net::Ipv4Addr;
use std::str::FromStr;

use binrw::{BinRead, BinResult};
use symphonia_core::audio::Channels;
use symphonia_core::codecs::CodecParameters;
use symphonia_core::errors::{seek_error, Error, Result, SeekErrorKind};
use symphonia_core::formats::{
    Cue, FormatOptions, FormatReader, Packet, SeekMode, SeekTo, SeekedTo, Track,
};
use symphonia_core::io::{MediaSourceStream, ReadBytes};
use symphonia_core::meta::{Metadata, MetadataLog};
use symphonia_core::probe::{Descriptor, Instantiate, QueryDescriptor};
use symphonia_core::support_format;
use symphonia_core::units::TimeBase;

const MAGIC: &[u8] = b"#!rtpplay1.0 ";

#[binrw::parser(reader, endian)]
fn parse_src_ip() -> BinResult<Ipv4Addr> {
    let pos = reader.stream_position()?;
    let ip: &mut [u8] = &mut [0; 15];
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
    channels: usize,
    chl_idx: usize,
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

        let mut r = Self {
            reader: source,
            tracks: vec![],
            track_ts: vec![],
            cues: vec![],
            metadata: Default::default(),
            channels: 0,
            chl_idx: 0,
            pkt_cnt: 0,
            sample_rate: None,
            timestamp_interval: 320,
        };

        let mut codec_params = CodecParameters::new();
        let sr = 16000;
        codec_params.codec = symphonia_codec_g7221::CODEC_TYPE_G722_1;
        codec_params.channels = Some(Channels::FRONT_CENTRE);
        codec_params
            .with_bits_per_sample(24000)
            .with_sample_rate(sr)
            .with_time_base(TimeBase::new(1, sr));

        r.channels = 1;
        r.tracks.push(Track::new(0, codec_params));
        r.track_ts.push(0);
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
        let pkt = Packet::new_from_slice(
            self.chl_idx as u32,
            self.track_ts[self.chl_idx] * self.timestamp_interval,
            self.timestamp_interval,
            &data[12..],
        );
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
