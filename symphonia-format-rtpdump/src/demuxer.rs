use std::collections::VecDeque;

use crate::rtp::{PayloadType, RawRtpPacket, RtpPacket};

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
        (Some(first), Some(last)) => {
            // println!("first: {}, lst: {}", first.ts(), last.ts());
            (last.ts().wrapping_sub(first.ts()) / delta_time) as usize + 1
        }
        _ => 0,
    }
}

impl<R: RtpPacket> Channel<R> {
    pub fn is_queue_full(&self, max: usize) -> bool {
        pkt_queue_len(&self.pkts, self.delta_time) > max
    }

    fn find_first_greater_seq_pkt(&self, pkt: &R) -> Option<usize> {
        self.pkts
            .iter()
            .enumerate()
            .filter(|(_, p)| p.seq() > pkt.seq())
            .next()
            .map(|(idx, _)| idx)
    }

    pub fn add_pkt(&mut self, pkt: R) {
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

    pub fn get_pkts(&mut self, cnt: usize) -> Option<VecDeque<R>> {
        let first_ts = match self.pkts.front() {
            None => return None,
            Some(p) => p.ts(),
        };

        let idx = match self
            .pkts
            .iter()
            .enumerate()
            .find(|(_, p)| (p.ts().wrapping_sub(first_ts) / self.delta_time) as usize + 1 > cnt)
        {
            None => return Some(std::mem::replace(&mut self.pkts, Default::default())),
            Some((idx, _)) => idx,
        };

        let mut rem = self.pkts.split_off(idx);
        std::mem::swap(&mut self.pkts, &mut rem);
        Some(rem)
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
    pub fn new(chls: Vec<Channel<R>>, qsize: usize) -> Self {
        Self {
            chls,
            sort_uniq_queue_size: qsize,
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
        return cond1 && cond2;
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

        return self.need_align();
    }

    fn any_queue_full(&self) -> bool {
        self.chls
            .iter()
            .any(|chl| chl.is_queue_full(self.sort_uniq_queue_size))
    }

    pub fn all_chl_finished(&self) -> bool {
        self.chls.iter().all(|c| match c.last_ts {
            None => false,
            Some(ts) => ts >= c.end,
        })
    }

    pub fn get_all_pkts(&mut self, queue: &mut VecDeque<R>) {
        for chl in self.chls.iter_mut() {
            for pkt in chl.pkts.drain(..) {
                queue.push_back(pkt);
            }
        }
    }
}

impl<R: RtpPacket + DummyRtpPacket + std::default::Default> RtpDemuxer<R> {
    pub fn get_pkts(&mut self, need_align: bool) -> Option<Vec<(u32, VecDeque<R>)>> {
        if need_align && !self.aligned {
            let mut result = vec![];
            for chl in &mut self.chls {
                if let Some(last) = chl.pkts.back() {
                    chl.last_ts = Some(last.ts());
                }
                let pkts = &mut chl.pkts;
                let mut rem = pkts.split_off(pkts.len());
                std::mem::swap(pkts, &mut rem);
                let out = rem;
                result.push((chl.ssrc, out));
            }

            return Some(result);
        }

        if !self.any_queue_full() {
            return None;
        }

        let mut result = vec![];

        for chl in &mut self.chls {
            let mut pkts = VecDeque::with_capacity(50);
            // let mut last_ts = None;
            loop {
                if pkts.len() >= 50 + chl.missed {
                    chl.missed = 0;
                    break;
                }

                match (chl.pkts.pop_front(), chl.last_ts) {
                    (None, None) => {
                        // not enough pkts, insert 50 dummy pkts
                        // should only happends on the first iteration
                        for _ in 0..50 {
                            pkts.push_back(R::dummy(chl.ssrc));
                        }
                        break;
                    }
                    (Some(pkt), None) => {
                        chl.last_ts = Some(pkt.ts());
                        pkts.push_back(pkt);
                    }
                    (Some(pkt), Some(ts)) => {
                        let gap = pkt.ts().saturating_sub(ts) / chl.delta_time;
                        let overflow_cnt = (pkts.len() as u32 + gap).saturating_sub(50);
                        if gap == 1 && overflow_cnt == 0 {
                            // 50th pkt
                            chl.last_ts = Some(pkt.ts());
                            pkts.push_back(pkt);
                        } else if gap == 1 && overflow_cnt > 0 {
                            // 51th pkt
                            if chl.missed == 0 {
                                chl.pkts.push_front(pkt);
                                break;
                            } else {
                                chl.missed -= 1;
                                chl.last_ts = Some(pkt.ts());
                                pkts.push_back(pkt);
                            }
                        } else if gap > 1 && overflow_cnt == 0 {
                            // [1st, 49th] pkt
                            for i in 1..gap {
                                // println!("in: {}", ts.wrapping_add((i + 1) * chl.delta_time));
                                pkts.push_back(R::dummy_ts(
                                    chl.ssrc,
                                    ts.wrapping_add(i * chl.delta_time),
                                ));
                            }
                            chl.last_ts = Some(pkt.ts());
                            pkts.push_back(pkt);
                        } else if gap > 1 && overflow_cnt > 0 {
                            // [52th, ) pkt
                            let cnt = (50usize.saturating_sub(pkts.len())) as u32;
                            for i in 0..cnt {
                                pkts.push_back(R::dummy_ts(
                                    chl.ssrc,
                                    ts.wrapping_add((i + 1) * chl.delta_time),
                                ));
                            }
                            chl.last_ts = Some(ts.wrapping_add(cnt * chl.delta_time));
                            chl.pkts.push_front(pkt);
                            break;
                        }
                    }
                    (None, Some(ts)) => {
                        if chl.end <= ts {
                            // no more pkts to dequeue, channel is out, fill dummy to 50
                            let cnt = 50usize.wrapping_sub(pkts.len()) as u32;
                            for i in 0..cnt {
                                pkts.push_back(R::dummy_ts(
                                    chl.ssrc,
                                    ts.wrapping_add((i + 1) * chl.delta_time),
                                ));
                            }
                            chl.last_ts = Some(ts.wrapping_add((cnt) * chl.delta_time));
                            break;
                        } else {
                            // no more pkts to dequeue, but still in progress
                            // record how many packets are missed in this epoch, wait for future packets
                            chl.missed += (50usize).saturating_sub(pkts.len());
                            break;
                        }
                    }
                }
            }
            result.push((chl.ssrc, pkts));
        }

        Some(result)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    impl SimpleRtpPacket {
        pub fn new_ts(ts: u32) -> Self {
            let ts = ts.to_be_bytes();
            let mut raw = [0; 12];
            raw[4] = ts[0];
            raw[5] = ts[1];
            raw[6] = ts[2];
            raw[7] = ts[3];
            Self { raw: raw.to_vec() }
        }
    }

    fn default_single_channel_demuxer() -> RtpDemuxer<SimpleRtpPacket> {
        let chl = Channel {
            delta_time: 1,
            ..Default::default()
        };
        let chls = vec![chl].into_iter().collect();
        RtpDemuxer::<SimpleRtpPacket>::new(chls, 250)
    }

    #[test]
    fn test_add_consecutive_pkt() {
        let mut demuxer = default_single_channel_demuxer();
        let pkt = SimpleRtpPacket::new_ts(0);
        assert!(!demuxer.add_pkt(pkt));
        assert_eq!(demuxer.chls.len(), 1);
        assert_eq!(demuxer.chls[0].pkts.len(), 1);
        assert_eq!(demuxer.chls[0].pkts[0].ts(), 0);

        let pkt = SimpleRtpPacket::new_ts(1);
        assert!(!demuxer.add_pkt(pkt));
        assert_eq!(demuxer.chls.len(), 1);
        assert_eq!(demuxer.chls[0].pkts.len(), 2);
        assert_eq!(demuxer.chls[0].pkts[0].ts(), 0);
        assert_eq!(demuxer.chls[0].pkts[1].ts(), 1);
    }

    #[test]
    fn test_add_non_consecutive_pkt() {
        // incoming [0,2,1]
        // incoming [0,1,2]
        let mut demuxer = default_single_channel_demuxer();
        let pkt = SimpleRtpPacket::new_ts(0);
        assert!(!demuxer.add_pkt(pkt));
        let pkt = SimpleRtpPacket::new_ts(2);
        assert!(!demuxer.add_pkt(pkt));

        let pkt = SimpleRtpPacket::new_ts(1);
        assert!(!demuxer.add_pkt(pkt));
        assert_eq!(demuxer.chls[0].pkts.len(), 3);
        assert_eq!(demuxer.chls[0].pkts[0].ts(), 0);
        assert_eq!(demuxer.chls[0].pkts[1].ts(), 1);
        assert_eq!(demuxer.chls[0].pkts[2].ts(), 2);

        // incoming  [0,1,4,2]
        // expecting [0,1,2,4]
        let mut demuxer = default_single_channel_demuxer();
        let pkt = SimpleRtpPacket::new_ts(0);
        assert!(!demuxer.add_pkt(pkt));
        let pkt = SimpleRtpPacket::new_ts(1);
        assert!(!demuxer.add_pkt(pkt));
        let pkt = SimpleRtpPacket::new_ts(4);
        assert!(!demuxer.add_pkt(pkt));
        assert_eq!(demuxer.chls[0].pkts.len(), 3);
        assert_eq!(demuxer.chls[0].pkts[0].ts(), 0);
        assert_eq!(demuxer.chls[0].pkts[1].ts(), 1);
        assert_eq!(demuxer.chls[0].pkts[2].ts(), 4);

        let pkt = SimpleRtpPacket::new_ts(2);
        assert!(!demuxer.add_pkt(pkt));
        assert_eq!(demuxer.chls[0].pkts.len(), 4);
        assert_eq!(demuxer.chls[0].pkts[0].ts(), 0);
        assert_eq!(demuxer.chls[0].pkts[1].ts(), 1);
        assert_eq!(demuxer.chls[0].pkts[2].ts(), 2);
        assert_eq!(demuxer.chls[0].pkts[3].ts(), 4);
    }

    #[test]
    fn test_single_ssrc() {
        let mut demuxer = default_single_channel_demuxer();

        let pkt = SimpleRtpPacket::new_ts(0);
        assert!(!demuxer.add_pkt(pkt));

        for seq in 1..101 {
            let pkt = SimpleRtpPacket::new_ts(seq);
            assert!(!demuxer.add_pkt(pkt));
        }

        assert_eq!(demuxer.chls[0].pkts.len(), 101);
        assert!(demuxer.any_queue_full());

        let pkts = demuxer.get_pkts(false);
        assert!(pkts.is_some());
        let pkts = pkts.unwrap();
        assert_eq!(pkts.len(), 1);
        assert_eq!(pkts[0].1.len(), 50);
    }
}
