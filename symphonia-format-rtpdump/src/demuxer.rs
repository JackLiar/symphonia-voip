use std::collections::VecDeque;

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
    pub ssrc: u32,
    /// Codec specific delta time, generally (sample rate)/50
    pub delta_time: u32,
    pub start: u32,
    pub end: u32,
    pub missed: usize,
    pub pkts: VecDeque<R>,
    pub pkt_cnt: u64,
    /// Last delivered ts
    pub last_ts: Option<u32>,
}

fn pkt_queue_len<R: RtpPacket>(queue: &VecDeque<R>, delta_time: u32) -> usize {
    match (queue.front(), queue.back()) {
        (Some(first), Some(last)) => (last.ts().wrapping_sub(first.ts()) / delta_time) as usize + 1,
        _ => 0,
    }
}

impl<R: RtpPacket> Channel<R> {
    fn queue_len(&self) -> usize {
        match (self.pkts.front(), self.pkts.back()) {
            (Some(first), Some(last)) => (last.ts().wrapping_sub(first.ts()) / self.delta_time) as usize + 1,
            _ => 0,
        }
    }

    pub fn is_queue_full(&self, max: usize) -> bool {
        pkt_queue_len(&self.pkts, self.delta_time) > max
    }

    fn find_first_greater_seq_pkt(&self, pkt: &R) -> Option<usize> {
        self.pkts
            .iter()
            .enumerate()
            .find(|(_, p)| p.seq() > pkt.seq())
            .map(|(idx, _)| idx)
    }

    pub fn add_pkt(&mut self, pkt: R) {
        if self.start < self.end {
            // no timestamp wrapping
            if pkt.ts() < self.start || pkt.ts() > self.end {
                return;
            }
        } else {
            // timestamp wrapping, very likely
            if pkt.ts() < self.end && pkt.ts() > self.start {
                return;
            }
        }

        if let Some(last_seq) = self.pkts.back().map(|p| p.seq()) {
            if last_seq.wrapping_add(1) == pkt.seq() {
                self.pkts.push_back(pkt);
            } else {
                match self.find_first_greater_seq_pkt(&pkt) {
                    Some(gre) => {
                        self.pkts.insert(gre, pkt);
                    }
                    None => {
                        self.pkts.push_back(pkt);
                    }
                };
            }
        } else {
            self.pkts.push_back(pkt);
        }
        self.pkt_cnt += 1;
    }
}

impl<R: RtpPacket + DummyRtpPacket> Channel<R> {
    pub fn get_pkts(&mut self, cnt: usize) -> VecDeque<R> {
        let mut pkts = VecDeque::with_capacity(cnt);
        loop {
            if pkts.len() >= cnt + self.missed {
                self.missed = 0;
                break;
            }

            match (self.pkts.pop_front(), self.last_ts) {
                (None, None) => {
                    // not enough pkts, insert 50 dummy pkts
                    // should only happends on the first iteration
                    for _ in 0..cnt {
                        pkts.push_back(R::dummy(self.ssrc));
                    }
                    break;
                }
                (Some(pkt), None) => {
                    self.last_ts = Some(pkt.ts());
                    pkts.push_back(pkt);
                }
                (Some(pkt), Some(ts)) => {
                    let gap = pkt.ts().saturating_sub(ts) / self.delta_time;
                    let overflow_cnt = (pkts.len() as u32 + gap).saturating_sub(cnt as u32);
                    if gap == 1 && overflow_cnt == 0 {
                        // [1st, 50th] pkt, and no packet is missed since last pkt
                        self.last_ts = Some(pkt.ts());
                        pkts.push_back(pkt);
                    } else if gap > 1 && overflow_cnt == 0 {
                        // [1st, 50th] pkt, and some packets are missed since last pkt
                        // insert dummy pkts before current pkt
                        for i in 1..gap {
                            pkts.push_back(R::dummy_ts(self.ssrc, ts.wrapping_add(i * self.delta_time)));
                        }
                        self.last_ts = Some(pkt.ts());
                        pkts.push_back(pkt);
                    } else if gap == 1 && overflow_cnt > 0 {
                        // 51th pkt, and no packet is missed since last pkt
                        if self.missed == 0 {
                            // if no pkt is missed in the past, put pkt back to cache
                            self.pkts.push_front(pkt);
                            break;
                        } else {
                            // if there are pkts missed in the past, dequeue one more pkt
                            self.missed -= 1;
                            self.last_ts = Some(pkt.ts());
                            pkts.push_back(pkt);
                        }
                    } else if gap > 1 && overflow_cnt > 0 {
                        // [52th, ) pkt, and some packets are missed since last pkt
                        let cnt = (cnt.saturating_sub(pkts.len())) as u32;
                        for i in 0..cnt {
                            pkts.push_back(R::dummy_ts(self.ssrc, ts.wrapping_add((i + 1) * self.delta_time)));
                        }
                        self.last_ts = Some(ts.wrapping_add(cnt * self.delta_time));
                        self.pkts.push_front(pkt);
                        break;
                    }
                }
                (None, Some(ts)) => {
                    if self.end <= ts {
                        // no more pkts to dequeue, channel is out, fill dummy to 50
                        let cnt = cnt.saturating_sub(pkts.len()) as u32;
                        for i in 0..cnt {
                            pkts.push_back(R::dummy_ts(self.ssrc, ts.wrapping_add((i + 1) * self.delta_time)));
                        }
                        self.last_ts = Some(ts.wrapping_add((cnt) * self.delta_time));
                        break;
                    } else {
                        // no more pkts to dequeue, but still in progress
                        // record how many packets are missed in this epoch, wait for future packets
                        self.missed += cnt.saturating_sub(pkts.len());
                        break;
                    }
                }
            }
        }
        pkts
    }
}

#[derive(Default)]
pub struct RtpDemuxer<R: RtpPacket> {
    pub chls: Vec<Channel<R>>,
    sort_uniq_queue_size: usize,
    aligned: bool,
}

impl<R: RtpPacket + std::default::Default> RtpDemuxer<R> {
    /// 100 rtp pkts is about 2 seconds
    pub fn new(chls: Vec<Channel<R>>) -> Self {
        Self {
            chls,
            sort_uniq_queue_size: 250,
            ..Default::default()
        }
    }

    fn need_align(&self) -> bool {
        if self.chls.len() == 1 {
            // if there is only one channel, no needs to align
            return false;
        }

        // some channel just receives its first packet
        let cond1 = self.chls.iter().any(|c| c.pkt_cnt == 1);
        // more than one channels have recieve packets already
        let cond2 = self.chls.iter().filter(|c| c.pkt_cnt > 0).count() > 1;
        cond1 && cond2
    }

    /// Add new rtp pkt into interval buffer, return whether found a new ssrc channel
    /// If found a new channel, all existing pkts needs to be processed so channels could be aligned
    pub fn add_pkt(&mut self, pkt: R) -> bool {
        let ssrc = pkt.ssrc();
        match self.chls.iter_mut().find(|chl| chl.ssrc == ssrc) {
            None => {
                eprintln!("no channel {:x} found", ssrc);
            }
            Some(chl) => {
                chl.add_pkt(pkt);
            }
        };

        self.need_align()
    }

    fn any_queue_full(&self) -> bool {
        self.chls.iter().any(|chl| chl.is_queue_full(self.sort_uniq_queue_size))
    }

    pub fn all_chl_finished(&self) -> bool {
        self.chls.iter().all(|c| match c.last_ts {
            None => false,
            Some(ts) => ts >= c.end,
        })
    }
}

impl<R: RtpPacket + DummyRtpPacket + std::default::Default> RtpDemuxer<R> {
    pub fn get_all_pkts(&mut self, queue: &mut VecDeque<R>) {
        let cnt = self
            .chls
            .iter()
            .map(|c| pkt_queue_len(&c.pkts, c.delta_time))
            .max()
            .unwrap();

        for chl in &mut self.chls {
            for pkt in chl.get_pkts(cnt) {
                queue.push_back(pkt);
            }
        }
    }

    pub fn get_pkts(&mut self, need_align: bool) -> Option<Vec<(u32, VecDeque<R>)>> {
        if need_align && !self.aligned {
            let mut result = vec![];
            let cnt = self
                .chls
                .iter()
                .map(|c| pkt_queue_len(&c.pkts, c.delta_time))
                .max()
                .unwrap();

            for chl in &mut self.chls {
                if chl.pkt_cnt == 1 {
                    // don't align channel that has just received its first packet
                    let mut pkts: VecDeque<_> = vec![].into();
                    for _ in 0..cnt {
                        pkts.push_back(R::dummy(chl.ssrc));
                    }
                    result.push((chl.ssrc, pkts));
                    continue;
                }

                let pkts = chl.get_pkts(cnt);
                result.push((chl.ssrc, pkts));
            }

            return Some(result);
        }

        if !self.any_queue_full() {
            return None;
        }

        let mut result = vec![];

        for chl in &mut self.chls {
            let pkts = chl.get_pkts(50);
            result.push((chl.ssrc, pkts));
        }

        Some(result)
    }
}

#[cfg(test)]
mod test {
    use rand::seq::SliceRandom;
    use rand::thread_rng;

    use super::*;

    impl SimpleRtpPacket {
        pub fn new_seq(seq: u16) -> Self {
            let seq = seq.to_be_bytes();
            let mut raw = [0; 12];
            raw[2] = seq[0];
            raw[3] = seq[1];
            Self { raw: raw.to_vec() }
        }

        pub fn new_seq_ts(seq: u16, ts: u32) -> Self {
            let seq = seq.to_be_bytes();
            let ts = ts.to_be_bytes();
            let mut raw = [0; 12];
            raw[2] = seq[0];
            raw[3] = seq[1];
            raw[4] = ts[0];
            raw[5] = ts[1];
            raw[6] = ts[2];
            raw[7] = ts[3];
            Self { raw: raw.to_vec() }
        }

        pub fn new_seq_ts_ssrc(seq: u16, ts: u32, ssrc: u32) -> Self {
            let seq = seq.to_be_bytes();
            let ts = ts.to_be_bytes();
            let ssrc = ssrc.to_be_bytes();
            let mut raw = [0; 12];
            raw[2] = seq[0];
            raw[3] = seq[1];
            raw[4] = ts[0];
            raw[5] = ts[1];
            raw[6] = ts[2];
            raw[7] = ts[3];
            raw[8] = ssrc[0];
            raw[9] = ssrc[1];
            raw[10] = ssrc[2];
            raw[11] = ssrc[3];
            Self { raw: raw.to_vec() }
        }
    }

    fn default_single_channel_demuxer() -> RtpDemuxer<SimpleRtpPacket> {
        let chl = Channel {
            delta_time: 1,
            ..Default::default()
        };
        let chls = vec![chl].into_iter().collect();
        RtpDemuxer::<SimpleRtpPacket>::new(chls)
    }

    #[test]
    fn test_add_non_consecutive_pkt() {
        let mut chl = Channel::default();

        // incoming  [0,2,1]
        // expecting [0,1,2]
        for seq in [0, 2, 1] {
            let pkt = SimpleRtpPacket::new_seq(seq);
            chl.add_pkt(pkt);
        }
        assert_eq!(chl.pkts.len(), 3);
        assert_eq!(chl.pkts[0].seq(), 0);
        assert_eq!(chl.pkts[1].seq(), 1);
        assert_eq!(chl.pkts[2].seq(), 2);

        // incoming  [0,1,4,2]
        // expecting [0,1,2,4]
        let mut chl = Channel::default();
        for seq in [0, 1, 4, 2] {
            let pkt = SimpleRtpPacket::new_seq(seq);
            chl.add_pkt(pkt);
        }
        assert_eq!(chl.pkts.len(), 4);
        assert_eq!(chl.pkts[0].seq(), 0);
        assert_eq!(chl.pkts[1].seq(), 1);
        assert_eq!(chl.pkts[2].seq(), 2);
        assert_eq!(chl.pkts[3].seq(), 4);
    }

    #[test]
    fn test_single_ssrc() {
        let mut demuxer = default_single_channel_demuxer();

        let pkt = SimpleRtpPacket::new_seq_ts(0, 1);
        assert!(!demuxer.add_pkt(pkt));

        let mut data = (1..251u32).collect::<Vec<_>>();
        data.shuffle(&mut thread_rng());
        for i in data {
            let pkt = SimpleRtpPacket::new_seq_ts(i as u16, i + 1);
            assert!(!demuxer.add_pkt(pkt));
        }

        assert_eq!(demuxer.chls[0].pkts.len(), 251);
        assert!(demuxer.any_queue_full());

        let pkts = demuxer.get_pkts(false);
        assert!(pkts.is_some());
        let pkts = pkts.unwrap();
        assert_eq!(pkts.len(), 1);
        assert_eq!(pkts[0].1.len(), 50);
        for (_ssrc, pkts) in pkts {
            for (idx, pkt) in pkts.into_iter().enumerate() {
                assert_eq!(pkt.ts(), idx as u32 + 1);
            }
        }
    }

    #[test]
    fn test_double_ssrc() {
        let chls = [0, 1]
            .into_iter()
            .map(|i| Channel {
                ssrc: i,
                delta_time: 1,
                ..Default::default()
            })
            .collect::<Vec<_>>();
        let mut demuxer = RtpDemuxer::<SimpleRtpPacket>::new(chls);

        let mut pkts = vec![];
        let mut data = (0..251u32).collect::<Vec<_>>();
        // channel 0 pkts
        data.shuffle(&mut thread_rng());
        for i in data.iter().copied() {
            let pkt = SimpleRtpPacket::new_seq_ts_ssrc(i as u16, i + 1, 0);
            pkts.push(pkt);
        }

        // channel 1 pkts
        data.shuffle(&mut thread_rng());
        for i in data.iter().copied() {
            let pkt = SimpleRtpPacket::new_seq_ts_ssrc(i as u16, i + 1, 1);
            pkts.push(pkt);
        }

        pkts.shuffle(&mut thread_rng());

        for pkt in pkts {
            demuxer.add_pkt(pkt);
        }

        assert_eq!(demuxer.chls[0].pkts.len(), 251);
        assert!(demuxer.any_queue_full());
        assert_eq!(demuxer.chls[1].pkts.len(), 251);
        assert!(demuxer.any_queue_full());

        let pkts = demuxer.get_pkts(false);
        assert!(pkts.is_some());
        let pkts = pkts.unwrap();
        assert_eq!(pkts.len(), 2);

        assert_eq!(pkts[0].1.len(), 50);
        assert_eq!(pkts[1].1.len(), 50);
        for (ssrc, pkts) in pkts {
            for (idx, pkt) in pkts.into_iter().enumerate() {
                println!("[{:#010x}] pkt ts: {}, idx: {}", ssrc, pkt.ts(), idx + 1);
                assert_eq!(pkt.ts(), idx as u32 + 1);
            }
        }
    }
}
