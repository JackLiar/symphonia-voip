//! Original algorithm: Fast RTP Detection and Codecs Classification in Internet Traffic(2014)

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, Seek};
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use fraction::Fraction;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

pub mod rtp;
use rtp::{parse_rtp_event, PayloadType, RtpPacket};

#[derive(Clone, Debug, Deserialize, Serialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Codec {
    pub name: Arc<String>,
    pub sample_rate: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<u8>,
    pub bit_rate: Option<u32>,
    pub params: Option<String>,
    pub payload_type: Option<u8>,
}

impl Codec {
    pub fn new(name: String, sample_rate: u32, channels: Option<u8>) -> Self {
        Self {
            name: Arc::new(name),
            sample_rate,
            channels,
            bit_rate: None,
            params: None,
            payload_type: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodecFeature {
    payload_size: Option<u16>,
    delta_time: u32,
    #[serde(skip_deserializing)]
    ratio: Option<Fraction>,
}

impl CodecFeature {
    pub fn new(payload_size: Option<u16>, delta_time: u32) -> Self {
        Self {
            payload_size,
            delta_time,
            ratio: payload_size.map(|ps| Fraction::new(delta_time, ps)),
        }
    }

    fn set_radio(&mut self) {
        self.ratio = self
            .payload_size
            .map(|ps| Fraction::new(self.delta_time, ps));
    }
}

#[derive(Clone, Debug, Default)]
pub struct CodecDetector {
    pt_pkt_stat: HashMap<PayloadType, u64>,
    codec_stat: HashMap<PayloadType, HashMap<Codec, u64>>,
    features: IndexMap<Codec, Vec<CodecFeature>>,
    last_seq: HashMap<u32, u16>,
    last_ts: HashMap<u32, u32>,
    pub max_uniq_payload_size_num: usize,
    payload_size_stat: HashMap<PayloadType, HashSet<usize>>,
}

impl CodecDetector {
    pub fn new() -> Self {
        CodecDetector {
            max_uniq_payload_size_num: 3,
            ..Default::default()
        }
    }
}

impl CodecDetector {
    pub fn add_feature(&mut self, codec: Codec, ft: CodecFeature) {
        match self.features.get_mut(&codec) {
            None => {
                self.features.insert(codec, vec![ft]);
            }
            Some(fts) => fts.push(ft),
        }
    }

    fn add_payload_len<P: RtpPacket>(&mut self, pkt: &P) {
        let payload_len = pkt.payload().len();
        match self.payload_size_stat.get_mut(&pkt.payload_type()) {
            None => {
                let mut lens = HashSet::new();
                lens.insert(payload_len);
                self.payload_size_stat.insert(pkt.payload_type(), lens);
            }
            Some(lens) => {
                if !lens.contains(&payload_len) {
                    lens.insert(payload_len);
                }
            }
        };
    }

    fn update_codec_stat(&mut self, pt: PayloadType, codec: &Codec) {
        match self.codec_stat.get_mut(&pt) {
            None => {
                let mut stat = HashMap::new();
                stat.insert(codec.clone(), 1);
                self.codec_stat.insert(pt, stat);
            }
            Some(stat) => {
                if let Some(stat) = stat.get_mut(codec) {
                    *stat += 1;
                }
            }
        }
    }

    fn is_dynamic_len<P: RtpPacket>(&mut self, pkt: &P) -> bool {
        match self.payload_size_stat.get(&pkt.payload_type()) {
            None => unreachable!("payload_size_stat always have incoming RTP payload type"),
            Some(lens) => lens.len() > self.max_uniq_payload_size_num,
        }
    }

    fn last_seq<P: RtpPacket>(&self, pkt: &P) -> u16 {
        match self.last_seq.get(&pkt.ssrc()) {
            Some(s) => *s,
            None => 0,
        }
    }

    fn last_ts<P: RtpPacket>(&self, pkt: &P) -> u32 {
        match self.last_ts.get(&pkt.ssrc()) {
            Some(ts) => *ts,
            None => 0,
        }
    }

    pub fn on_pkt<P: RtpPacket>(&mut self, pkt: &P) {
        // Filter out all RTP event pkts and non dynamic codec pkts
        if parse_rtp_event(pkt.payload()).is_ok() {
            return;
        }

        if !pkt.payload_type().is_dynamic() {
            let codec = self
                .features
                .iter()
                .find(|(codec, _)| codec.payload_type == Some(pkt.payload_type().to_u8()))
                .map(|(codec, _)| codec.clone());
            if let Some(codec) = codec {
                self.update_codec_stat(pkt.payload_type(), &codec);
            }
            return;
        }

        self.add_payload_len(pkt);
        match self.pt_pkt_stat.get_mut(&pkt.payload_type()) {
            None => {
                self.pt_pkt_stat.insert(pkt.payload_type(), 1);
            }
            Some(cnt) => *cnt += 1,
        };

        if (pkt.seq() - self.last_seq(pkt)) != 1 {
            self.last_seq.insert(pkt.ssrc(), pkt.seq());
            self.last_ts.insert(pkt.ssrc(), pkt.ts());
            return;
        }

        let delta_time = pkt.ts().wrapping_sub(self.last_ts(pkt));
        self.last_seq.insert(pkt.ssrc(), pkt.seq());
        self.last_ts.insert(pkt.ssrc(), pkt.ts());

        let payload_len = if self.is_dynamic_len(pkt) {
            None
        } else {
            Some(pkt.payload().len() as u16)
        };
        let ft = CodecFeature::new(payload_len, delta_time);

        for (codec, fts) in &self.features {
            for f in fts {
                let ft_match = match ft.payload_size {
                    Some(_) => f.ratio == ft.ratio,
                    None => f.delta_time == ft.delta_time,
                };
                if ft_match {
                    match self.codec_stat.get_mut(&pkt.payload_type()) {
                        None => {
                            let mut stat = HashMap::new();
                            stat.insert(codec.clone(), 1);
                            self.codec_stat.insert(pkt.payload_type(), stat);
                        }
                        Some(stat) => {
                            if let Some(stat) = stat.get_mut(codec) {
                                *stat += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn on_pkts<'a, I, P: RtpPacket + 'a>(&mut self, pkts: I)
    where
        I: IntoIterator<Item = &'a P>,
    {
        for pkt in pkts {
            self.on_pkt(pkt)
        }
    }

    pub fn get_result(&self) -> HashMap<PayloadType, Codec> {
        let mut result = HashMap::new();
        for (pt, stat) in &self.codec_stat {
            let tot_cnt = self.pt_pkt_stat.get(pt).unwrap_or(&0);
            for (codec, cnt) in stat {
                if *cnt > (tot_cnt * 618 / 1000) {
                    result.insert(*pt, codec.clone());
                    break;
                }
            }
        }
        result
    }

    pub fn pts(&self) -> Vec<PayloadType> {
        self.pt_pkt_stat.keys().cloned().collect()
    }

    pub fn get_features_from_yaml(&mut self, fpath: &Path) -> Result<()> {
        let mut file = BufReader::new(File::open(fpath)?);
        let codecs: Vec<Codec> = serde_yaml::from_reader(&mut file)?;
        file.rewind()?;
        let features: Vec<CodecFeature> = serde_yaml::from_reader(&mut file)?;
        for (codec, mut ft) in codecs.iter().zip(features) {
            ft.set_radio();
            self.add_feature(codec.clone(), ft);
        }
        Ok(())
    }
}
