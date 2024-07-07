use std::collections::{HashMap, VecDeque};
use std::io::{Error as IOError, ErrorKind, Seek, SeekFrom};
use std::net::Ipv4Addr;
use std::path::Path;
use std::str::FromStr;

use binrw::{BinRead, BinResult};
use indexmap::IndexMap;
use rtp::{codec_to_codec_type, parse_rtp_payload};
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

mod codec_detector;
mod demuxer;
mod rtp;
mod utils;
use codec_detector::{Codec, CodecDetector};
use demuxer::{Channel, RtpDemuxer, SimpleRtpPacket};
use rtp::{parse_rtp, PayloadType, RawRtpPacket, RtpPacket};

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
        params.with_max_frames_per_packet(frames);
    }

    match codec_to_codec_type(codec) {
        Some(ct) => params.codec = ct,
        None => return None,
    };

    if codec.name.as_str() == "amr" || codec.name.as_str() == "amrwb" {
        use symphonia_bundle_amr::DecoderParams;
        let mut dp = DecoderParams::default();
        if let Some(p) = codec.params.as_ref() {
            if p.contains("octet-align=1") {
                dp.octet_align = true;
            } else if p.contains("octet-align=0") {
                dp.octet_align = false;
            }
        }
        params.extra_data = Some(utils::struct_to_boxed_bytes(dp));
    }

    Some(params)
}

pub struct RtpdumpReader {
    demuxer: RtpDemuxer<SimpleRtpPacket>,
    reader: MediaSourceStream,
    tracks: Vec<Track>,
    track_ts: Vec<(u32, u64)>,
    cues: Vec<Cue>,
    metadata: MetadataLog,
    codecs: HashMap<PayloadType, Codec>,
    cache: VecDeque<SimpleRtpPacket>,
    pkt_cnt: usize,
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
    fn try_new(mut source: MediaSourceStream, _options: &FormatOptions) -> Result<Self>
    where
        Self: Sized,
    {
        let _hdr = match FileHeader::read(&mut source) {
            Ok(hdr) => hdr,
            Err(binrw::Error::Io(e)) => return Err(Error::IoError(e)),
            Err(_) => return Err(Error::DecodeError("Failed to decode rtpdump header")),
        };
        let hdr_len = source.pos();

        let mut chls: IndexMap<u32, Channel<SimpleRtpPacket>> = Default::default();
        let mut detector = CodecDetector::new();
        detector
            .get_features_from_yaml(Path::new("codec.yaml"))
            .unwrap();
        loop {
            let pkt = match read_rd_pkt(&mut source) {
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

            match chls.get_mut(&pkt.ssrc()) {
                None => {
                    let chl = Channel {
                        ssrc: pkt.ssrc(),
                        start: pkt.ts(),
                        ..Default::default()
                    };
                    chls.insert(pkt.ssrc(), chl);
                }
                Some(chl) => {
                    chl.start = (chl.start).min(pkt.ts());
                    if chl.end == 0 {
                        chl.end = pkt.ts();
                    } else {
                        chl.end = chl.end.max(pkt.ts());
                    }
                }
            }
            detector.on_pkt(&pkt);
        }

        let result = detector.get_result();
        println!("codec detect result: {:?}", result);
        if result.is_empty() {
            return Err(Error::Unsupported("Failed to detect codec"));
        }
        if result.len() != 1 {
            todo!("Support multi codec/change codec")
        }

        let (ssrcs, chls): (Vec<u32>, Vec<Channel<SimpleRtpPacket>>) = chls.into_iter().unzip();

        let mut r = Self {
            demuxer: RtpDemuxer::new(chls, 250),
            reader: source,
            tracks: vec![],
            track_ts: vec![],
            cues: vec![],
            metadata: Default::default(),
            codecs: result.clone(),
            cache: vec![].into(),
            pkt_cnt: 0,
        };

        r.reader.seek(SeekFrom::Start(hdr_len))?;
        let codec = result.values().collect::<Vec<_>>()[0];
        for ssrc in ssrcs {
            let param =
                codec_to_param(&codec).ok_or_else(|| Error::Unsupported("Unsupported codec"))?;
            r.tracks.push(Track::new(ssrc, param));
            r.track_ts.push((ssrc, 0));
        }
        Ok(r)
    }

    fn next_packet(&mut self) -> Result<Packet> {
        if !self.cache.is_empty() {
            let pkt = self.get_pkt_from_cache()?;
            return self.rtp_pkt_to_symphonia_pkt(pkt);
        }

        loop {
            if self.demuxer.all_chl_finished() {
                self.demuxer.get_all_pkts(&mut self.cache);
                if !self.cache.is_empty() {
                    let pkt = self.get_pkt_from_cache()?;
                    return self.rtp_pkt_to_symphonia_pkt(pkt);
                } else {
                    return Err(Error::IoError(IOError::new(
                        ErrorKind::UnexpectedEof,
                        "end of streamm",
                    )));
                }
            }

            let data = match read_rd_pkt(&mut self.reader) {
                Ok(data) => data,
                Err(e) => {
                    self.demuxer.get_all_pkts(&mut self.cache);
                    if !self.cache.is_empty() {
                        let pkt = self.get_pkt_from_cache()?;
                        return self.rtp_pkt_to_symphonia_pkt(pkt);
                    } else {
                        // println!("total pkt cnt: {}", self.pkt_cnt);
                        return Err(e);
                    }
                }
            };

            let pkt = parse_rtp(data.as_ref()).unwrap();
            let need_align = self.demuxer.add_pkt(SimpleRtpPacket::from(&pkt));

            let codec = self.codecs.get(&pkt.payload_type());
            let chl = self.demuxer.chls.iter_mut().find(|c| c.ssrc == pkt.ssrc());
            match (codec, chl) {
                (Some(codec), Some(chl)) => {
                    chl.delta_time = codec.sample_rate / 50;
                }
                _ => unreachable!("this should never happens"),
            };

            if let Some(pkts) = self.demuxer.get_pkts(need_align) {
                for (_ssrc, pkts) in pkts {
                    self.cache.extend(pkts);
                }
                break;
            }
        }

        let pkt = self.get_pkt_from_cache()?;
        return self.rtp_pkt_to_symphonia_pkt(pkt);
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

    fn seek(&mut self, _mode: SeekMode, _to: SeekTo) -> Result<SeekedTo> {
        if self.tracks.is_empty() {
            return seek_error(SeekErrorKind::Unseekable);
        }

        unimplemented!()
    }

    fn into_inner(self: Box<Self>) -> MediaSourceStream {
        self.reader
    }
}

impl RtpdumpReader {
    fn get_pkt_from_cache(&mut self) -> Result<SimpleRtpPacket> {
        match self.cache.pop_front() {
            None => Err(Error::IoError(IOError::new(ErrorKind::UnexpectedEof, ""))),
            Some(pkt) => {
                self.pkt_cnt += 1;
                Ok(pkt)
            }
        }
    }

    fn rtp_pkt_to_symphonia_pkt(&mut self, pkt: SimpleRtpPacket) -> Result<Packet> {
        // println!("pkt ts: {}", pkt.ts());
        let track = self.tracks.iter().find(|t| t.id == pkt.ssrc()).unwrap();
        let ts = self
            .track_ts
            .iter_mut()
            .find(|(ssrc, _)| *ssrc == pkt.ssrc())
            .map(|(_, ts)| ts)
            .unwrap();

        let data = if pkt.payload().is_empty() {
            vec![]
        } else {
            parse_rtp_payload(&track.codec_params, &pkt).unwrap_or_default()
        };

        let pkt = Packet::new_from_slice(
            pkt.ssrc(),
            *ts * (track.codec_params.sample_rate.unwrap() as u64) / 50,
            ((track.codec_params.sample_rate.unwrap() / 50) as u64).min(data.len() as u64),
            &data,
        );
        *ts += 1;
        return Ok(pkt);
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
