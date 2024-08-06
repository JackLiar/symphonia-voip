use std::collections::HashMap;
use std::io::{Error as IOError, ErrorKind, Seek, SeekFrom};
use std::ops::Add;
use std::path::Path;
use std::time::Duration;

use binrw::BinRead;
use indexmap::IndexMap;
use log::{debug, info, warn};
use rtp::{codec_to_codec_type, parse_rtp_payload, SeqNum};
use symphonia_core::audio::Channels;
use symphonia_core::codecs::CodecParameters;
use symphonia_core::errors::{seek_error, Error, Result, SeekErrorKind};
use symphonia_core::formats::{Cue, FormatOptions, FormatReader, Packet, SeekMode, SeekTo, SeekedTo, Track};
use symphonia_core::io::{MediaSourceStream, ReadBytes};
use symphonia_core::meta::{Metadata, MetadataLog};
use symphonia_core::probe::{Descriptor, Instantiate, QueryDescriptor};
use symphonia_core::support_format;
use symphonia_core::units::TimeBase;

mod codec_detector;
mod demuxer;
mod demuxer_new;
mod format;
mod rtp;
mod utils;

use codec_detector::{Codec, CodecDetector};
// use demuxer::{Channel, RtpDemuxer, SimpleRtpPacket};
use demuxer_new::{Channel, RtpDemuxer, SimpleRtpPacket};
use format::{read_rd_pkt, FileHeader, MAGIC};
use rtp::{parse_rtp, parse_rtp_event, PayloadType, RawRtpPacket, RtpPacket};

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
    pkt_cnt: usize,
    start_ts: Duration,
    rd_pkt_cnt: u64,
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

struct LastPacket {
    seq: SeqNum,
    ts: u32,
}

impl FormatReader for RtpdumpReader {
    fn try_new(mut source: MediaSourceStream, _options: &FormatOptions) -> Result<Self>
    where
        Self: Sized,
    {
        let hdr = match FileHeader::read(&mut source) {
            Ok(hdr) => hdr,
            Err(binrw::Error::Io(e)) => return Err(Error::IoError(e)),
            Err(_) => return Err(Error::DecodeError("Failed to decode rtpdump header")),
        };
        info!("rtpdump hdr: {:?}", hdr);
        let hdr_len = source.pos();

        let mut chls: IndexMap<u32, (Channel<SimpleRtpPacket>, LastPacket)> = Default::default();
        let mut detector = CodecDetector::new();
        detector.get_features_from_yaml(Path::new("codec.yaml")).unwrap();
        loop {
            let (offset, pkt) = match read_rd_pkt(&mut source) {
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
                        first_packet: hdr.start_ts().add(offset),
                        ingress_sort_uniq_len: 250,
                        frame_dur: 20,
                        ..Default::default()
                    };
                    let last = LastPacket {
                        seq: SeqNum(pkt.seq()),
                        ts: pkt.ts(),
                    };
                    chls.insert(pkt.ssrc(), (chl, last));
                }
                Some((chl, last)) => {
                    let seq = SeqNum(pkt.seq());
                    chl.start = (chl.start).min(pkt.ts());
                    chl.last_packet = hdr.start_ts().add(offset);

                    if chl.end == 0 || seq > last.seq {
                        chl.end = pkt.ts();
                    }
                    last.seq = seq;
                    last.ts = pkt.ts();
                }
            };

            detector.on_pkt(&pkt);
        }

        let result = detector.get_result();
        info!("codecs:");
        for (pt, codec) in &result {
            let pt: u8 = (*pt).into();
            info!("{}: {:?}", pt, codec);
        }
        if result.is_empty() {
            return Err(Error::Unsupported("Failed to detect codec"));
        }

        let chls: Vec<Channel<SimpleRtpPacket>> = chls.into_values().map(|(chl, _)| chl).collect();
        let codec = result.values().collect::<Vec<_>>()[0];
        let mut tracks = vec![];
        let mut track_ts = vec![];
        for chl in &chls {
            let mut param = match codec_to_param(codec) {
                Some(p) => p,
                None => {
                    warn!("Unsupported codec: {:?}", codec);
                    return Err(Error::Unsupported("Unsupported codec"));
                }
            };
            param.with_n_frames((chl.end.saturating_sub(chl.start)) as u64);
            let track = Track::new(chl.ssrc, param);
            tracks.push(track);
            track_ts.push((chl.ssrc, 0));
        }

        let mut r = Self {
            demuxer: RtpDemuxer::new(chls),
            reader: source,
            tracks,
            track_ts,
            cues: vec![],
            metadata: Default::default(),
            codecs: result.clone(),
            pkt_cnt: 0,
            start_ts: hdr.start_ts(),
            rd_pkt_cnt: 0,
        };

        r.reader.seek(SeekFrom::Start(hdr_len))?;

        Ok(r)
    }

    fn next_packet(&mut self) -> Result<Packet> {
        loop {
            let (offset, data) = match read_rd_pkt(&mut self.reader) {
                Ok(data) => data,
                Err(e) => {
                    // self.demuxer.get_all_pkts(&mut self.cache);
                    debug!("total pkt cnt: {}", self.pkt_cnt);
                    return Err(Error::IoError(IOError::new(ErrorKind::UnexpectedEof, "end of stream")));
                }
            };
            self.rd_pkt_cnt += 1;
            println!("rd pkt cnt: {}", self.rd_pkt_cnt);

            let pkt = parse_rtp(data.as_ref()).unwrap();

            if parse_rtp_event(pkt.payload()).is_ok() {
                continue;
            }
            let codec = self.codecs.get(&pkt.payload_type());
            let chl = self.demuxer.chls.iter_mut().find(|c| c.ssrc == pkt.ssrc());
            let track = self.tracks.iter_mut().find(|t| t.id == pkt.ssrc());
            match (codec, chl, track) {
                (Some(codec), Some(chl), Some(track)) => {
                    match codec.delta_time {
                        Some(dt) => chl.delta_time = dt,
                        None => chl.delta_time = codec.sample_rate / 50,
                    };
                    let param = match codec_to_param(codec) {
                        Some(p) => p,
                        None => {
                            warn!("Unsupported codec: {:?}", codec);
                            return Err(Error::Unsupported("Unsupported codec"));
                        }
                    };
                    track.codec_params = param;
                }
                _ => {
                    unreachable!(
                        "this should never happens: {:?} {:?} {:#010x}",
                        pkt.payload_type(),
                        codec,
                        pkt.ssrc()
                    )
                }
            };

            if let Some(pkt) = self
                .demuxer
                .add_pkt(SimpleRtpPacket::from(&pkt), self.start_ts.add(offset))
            {
                return self.rtp_pkt_to_symphonia_pkt(pkt);
            }
        }
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
    fn rtp_pkt_to_symphonia_pkt(&mut self, pkt: SimpleRtpPacket) -> Result<Packet> {
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
            match parse_rtp_payload(&track.codec_params, &pkt) {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("Failed to decode rtp papyload, {}", e);
                    vec![]
                }
            }
        };

        let pkt = Packet::new_from_slice(
            pkt.ssrc(),
            *ts * (track.codec_params.sample_rate.unwrap() as u64) / 50,
            ((track.codec_params.sample_rate.unwrap() / 50) as u64).min(data.len() as u64),
            &data,
        );
        *ts += 1;
        Ok(pkt)
    }
}
