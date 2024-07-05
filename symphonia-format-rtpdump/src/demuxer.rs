use std::collections::VecDeque;

use codec_detector::rtp::{PayloadType, RawRtpPacket, RtpPacket};

#[derive(Clone, Debug, Default)]
pub struct SimpleRtpPacket {
    pub raw: Vec<u8>,
}

impl RtpPacket for SimpleRtpPacket {
    fn raw(&self) -> &[u8] {
        &self.raw
    }
}

impl SimpleRtpPacket {
    pub fn new_dummy(ssrc: u32, pt: PayloadType) -> Self {
        let ssrc = ssrc.to_be_bytes();
        let mut raw = vec![0; 12];
        raw[1] = pt.into();
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
    pub pkts: VecDeque<R>,
}

impl<R: RtpPacket> Channel<R> {
    pub fn pkt_queue_len(&self) -> usize {
        match (self.pkts.front(), self.pkts.back()) {
            (Some(first), Some(last)) => {
                // println!("first: {}, lst: {}", first.ts(), last.ts());
                (last.ts().wrapping_sub(first.ts()) / self.delta_time) as usize + 1
            }
            _ => 0,
        }
    }

    pub fn is_queue_full(&self, max: usize) -> bool {
        // println!("ssrc: {:X}, queue len: {}", self.ssrc, self.pkt_queue_len());
        self.pkt_queue_len() > max
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
            if last_seq + 1 == pkt.seq() {
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
    pub fn new(chls: Vec<Channel<R>>) -> Self {
        Self {
            chls,
            sort_uniq_queue_size: 100,
            ..Default::default()
        }
    }

    fn need_align(&self) -> bool {
        if self.chls.len() == 1 {
            // if there is only one channel, no needs to align
            return false;
        }

        // some channel just receives its first packet
        let cond1 = self.chls.iter().any(|c| c.pkts.len() == 1);
        // more than one channels have recieve packets already
        let cond2 = self.chls.iter().filter(|c| c.pkts.is_empty()).count() > 1;
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

    pub fn get_pkts(&mut self, need_align: bool) -> Option<Vec<(u32, VecDeque<R>)>> {
        if need_align && !self.aligned {
            let mut result = vec![];
            for chl in &mut self.chls {
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
            if let Some(pkts) = chl.get_pkts(50) {
                result.push((chl.ssrc, pkts));
            }
        }

        Some(result)
    }

    pub fn get_all_pkts(&mut self, queue: &mut VecDeque<R>) {
        for chl in self.chls.iter_mut() {
            for pkt in chl.pkts.drain(..) {
                queue.push_back(pkt);
            }
        }
    }
}

pub fn insert_silence_dummy_pkt<I: Iterator<Item = SimpleRtpPacket>>(
    pkts: I,
    cache: &mut VecDeque<SimpleRtpPacket>,
    pt: PayloadType,
    delta_time: u32,
) {
    let mut last_ts = None;
    for pkt in pkts {
        if let Some(ts) = last_ts {
            let loss = (pkt.ts().wrapping_sub(ts) / delta_time).wrapping_sub(1);
            if loss != 0 {
                for _ in 0..loss + 1 {
                    let dummy = SimpleRtpPacket::new_dummy(pkt.ssrc(), pt);
                    cache.push_back(dummy);
                }
            }
        }
        last_ts = Some(pkt.ts());
        cache.push_back(pkt);
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
        RtpDemuxer::<SimpleRtpPacket>::new(chls)
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
