use std::collections::VecDeque;
use std::time::Duration;

use itertools::Itertools;

use crate::rtp::{RawRtpPacket, RtpPacket};

pub trait DummyRtpPacket: RtpPacket {
    fn dummy(ssrc: u32) -> Self;
    fn dummy_ts(ssrc: u32, ts: u32) -> Self;
}

#[derive(Clone, Debug, Default)]
pub struct SimpleRtpPacket {
    pub raw: Vec<u8>,
}

impl RtpPacket for SimpleRtpPacket {
    fn raw(&self) -> &[u8] {
        &self.raw
    }
}

impl DummyRtpPacket for SimpleRtpPacket {
    fn dummy(ssrc: u32) -> Self {
        let ssrc = ssrc.to_be_bytes();
        let mut raw = vec![0; 12];
        raw[8] = ssrc[0];
        raw[9] = ssrc[1];
        raw[10] = ssrc[2];
        raw[11] = ssrc[3];
        Self { raw }
    }

    fn dummy_ts(ssrc: u32, ts: u32) -> Self {
        let ssrc = ssrc.to_be_bytes();
        let ts = ts.to_be_bytes();
        let mut raw = vec![0; 12];
        raw[4] = ts[0];
        raw[5] = ts[1];
        raw[6] = ts[2];
        raw[7] = ts[3];
        raw[8] = ssrc[0];
        raw[9] = ssrc[1];
        raw[10] = ssrc[2];
        raw[11] = ssrc[3];
        Self { raw }
    }
}

impl From<&RawRtpPacket<'_>> for SimpleRtpPacket {
    fn from(value: &RawRtpPacket) -> Self {
        Self {
            raw: value.raw().to_vec(),
        }
    }
}

#[derive(Default)]
pub struct Channel<R> {
    /// RTP SSRC
    pub ssrc: u32,
    /// Codec specific delta time(aka frame duration), generally (sample rate)/50
    pub delta_time: u32,
    /// Frame duration, generally 20ms
    pub frame_dur: u16,
    /// RTP start timestamp
    pub start: u32,
    /// RTP end timestamp
    pub end: u32,
    /// Channel first packet timestamp
    pub first_packet: Duration,
    /// Chanel last packet timestamp
    pub last_packet: Duration,
    /// Real timestamp since UNIX EPOCH
    pub timestamp: Duration,
    // pub pkt_cnt: u64,
    pub delivered: Option<u32>,
    /// Last delivered ts
    pub last_dummy_ts: Duration,
    /// Receive packets from format reader
    pub ingress: VecDeque<R>,
    /// Num of packets to be cached to sort and uniq RTP packet
    pub ingress_sort_uniq_len: usize,
    /// Send packets to codec decoder
    pub egress: VecDeque<R>,
}

impl<R: RtpPacket> Channel<R> {
    /// Ingress queue length
    fn ingress_len(&self) -> usize {
        match (self.ingress.front(), self.ingress.back()) {
            (Some(first), Some(last)) => (last.ts().wrapping_sub(first.ts()) / self.delta_time) as usize + 1,
            _ => 0,
        }
    }

    /// Egress queue length
    fn egress_len(&self) -> usize {
        match (self.egress.front(), self.egress.back()) {
            (Some(first), Some(last)) => (last.ts().wrapping_sub(first.ts()) / self.delta_time) as usize + 1,
            _ => 0,
        }
    }

    /// Ingress queue is full or not
    pub fn ingress_full(&self, max: usize) -> bool {
        self.ingress_len() > max
    }

    fn find_first_greater_seq_pkt(&self, pkt: &R) -> Option<usize> {
        self.ingress
            .iter()
            .enumerate()
            .find(|(_, p)| p.seq() > pkt.seq())
            .map(|(idx, _)| idx)
    }

    fn active(&self) -> bool {
        let started = self.timestamp >= self.first_packet;
        let ended = self.timestamp >= self.last_packet;
        started && !ended
    }

    fn finished(&self) -> bool {
        self.timestamp >= self.last_packet
    }
}

impl<R: RtpPacket + DummyRtpPacket> Channel<R> {
    /// Add RTP pkt into ingress queue
    pub fn add_pkt(&mut self, pkt: R, ts: Duration) -> Option<R> {
        self.timestamp = ts;
        if self.start < self.end {
            // no timestamp wrapping
            if pkt.ts() < self.start || pkt.ts() > self.end {
                return None;
            }
        } else {
            // timestamp wrapping, very likely
            if pkt.ts() < self.end && pkt.ts() > self.start {
                return None;
            }
        }

        if let Some(last_seq) = self.ingress.back().map(|p| p.seq()) {
            if last_seq.wrapping_add(1) == pkt.seq() {
                self.ingress.push_back(pkt);
            } else {
                match self.find_first_greater_seq_pkt(&pkt) {
                    Some(gre) => {
                        self.ingress.insert(gre, pkt);
                    }
                    None => {
                        self.ingress.push_back(pkt);
                    }
                };
            }
        } else {
            self.ingress.push_back(pkt);
        }

        self.get_pkt()
    }

    // Only effects when channel is not active
    pub fn sync(&mut self, ts: Duration) {
        self.timestamp = ts;
        if self.active() {
            return;
        }

        let dur = self.timestamp.saturating_sub(self.last_dummy_ts).as_millis();
        if (dur / self.frame_dur as u128) != 0 {
            self.egress.push_back(R::dummy(self.ssrc));
            self.last_dummy_ts = ts;
        }
    }

    pub fn get_pkt(&mut self) -> Option<R> {
        if !self.finished() && !self.ingress_full(self.ingress_sort_uniq_len) {
            return None;
        }

        let pkt = match self.ingress.pop_front() {
            None => return None,
            Some(pkt) => pkt,
        };

        match self.delivered {
            None => {
                self.delivered = Some(pkt.ts());
                Some(pkt)
            }
            Some(ts) => {
                let sid_cnt = (pkt.ts().saturating_sub(ts) / self.delta_time).saturating_sub(1);
                for i in 0..sid_cnt {
                    self.egress
                        .push_back(R::dummy_ts(self.ssrc, pkt.ts().wrapping_add((i + 1) * self.delta_time)))
                }
                self.egress.push_back(pkt);
                let pkt = self.egress.pop_front();
                if let Some(pkt) = &pkt {
                    self.delivered = Some(pkt.ts());
                }
                pkt
            }
        }
    }
}

#[derive(Default)]
pub struct RtpDemuxer<R: RtpPacket> {
    pub chls: Vec<Channel<R>>,
}

impl<R: RtpPacket + std::default::Default> RtpDemuxer<R> {
    /// 100 rtp pkts is about 2 seconds
    pub fn new(chls: Vec<Channel<R>>) -> Self {
        Self { chls }
    }

    pub fn all_chl_finished(&self) -> bool {
        self.chls.iter().all(|c| c.finished())
    }
}

impl<R: RtpPacket + DummyRtpPacket + std::default::Default> RtpDemuxer<R> {
    pub fn add_pkt(&mut self, pkt: R, ts: Duration) -> Option<R> {
        let ssrc = pkt.ssrc();
        let pkt = match self.chls.iter_mut().find(|chl| chl.ssrc == ssrc) {
            None => unreachable!("no channel {:#010x} found", ssrc),
            Some(chl) => chl.add_pkt(pkt, ts),
        };

        for chl in &mut self.chls.iter_mut().filter(|c| c.ssrc != ssrc) {
            chl.sync(ts);
        }
        pkt
    }

    pub fn get_pkt(&mut self) -> Option<R> {
        for chl in &mut self.chls.iter_mut().sorted_by_key(|c| c.ingress_len()) {
            if let Some(pkt) = chl.get_pkt() {
                return Some(pkt);
            }
        }

        None
    }

    pub fn get_all_pkts(&mut self, queue: &mut VecDeque<R>) {
        for chl in &mut self.chls {
            while let Some(pkt) = chl.egress.pop_front() {
                queue.push_back(pkt);
            }
        }
    }
}
