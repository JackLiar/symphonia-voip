use std::collections::VecDeque;
use std::time::Duration;

use itertools::Itertools;

use codec_detector::rtp::RtpPacket;

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
    pub fn new(raw: Vec<u8>) -> Self {
        Self { raw }
    }
}

/// Sort RTP packets by seq num and remove RTP packet retransmission
///
/// What this function actually do is inplace sort and filter.
/// We needs to guarantee each ssrc's packet is uniq and sorted by seq num.
/// Meanwhile rtp packet's original relative location in the whole sequence is maintained,
/// so that we could play them at the appoperate time.
/// Tough relative location is unlikely to be 100% correct,
/// but few sample mis ordered is not that serious.
///
/// This function is not memory efficient nor cpu efficient at all,
/// what we trying to guarantee is the process logic is correct.
pub fn sort_and_uniq<R: RtpPacket>(pkts: Vec<(Duration, R)>) -> Vec<(Duration, R)> {
    // vec![None; pkts.len()] require R to implement Clone, this way could bypass the restriction
    let mut result = Vec::with_capacity(pkts.len());
    for _ in 0..pkts.len() {
        result.push(None)
    }

    // split RTP pkts by ssrc
    let mut ssrc_pkts = vec![];
    for (i, (ts, pkt)) in pkts.into_iter().enumerate() {
        match ssrc_pkts.iter_mut().find(|(ssrc, _)| *ssrc == pkt.ssrc()) {
            None => {
                let ssrc = pkt.ssrc();
                let pkts = vec![(i, (ts, pkt))];
                ssrc_pkts.push((ssrc, vec![pkts]));
            }
            Some((_, pkts)) => {
                let seq = pkt.seq();
                pkts.last_mut().unwrap().push((i, (ts, pkt)));
                if seq == 65535 {
                    pkts.push(vec![])
                }
            }
        }
    }

    for (_, bucket) in ssrc_pkts {
        // Remove any single 65535 packet bucket, the reason this kind of bucket exists
        // is 65535 packet get retransmitted.
        // Then remove all retransmitted rtp pkts in this bucket
        // Finally sort all packets in this bucket and put it in results
        bucket
            .into_iter()
            .filter(|b| !(b.len() == 1 && b[0].1 .1.seq() == 65535))
            .map(|b| {
                b.into_iter()
                    .unique_by(|(_, pkt)| pkt.1.seq())
                    .collect::<Vec<_>>()
            })
            .flat_map(|b| {
                let (idxs, mut pkts): (Vec<_>, Vec<_>) = b.into_iter().unzip();
                pkts.sort_by_key(|(_, pkt)| pkt.seq());
                idxs.into_iter().zip(pkts).collect::<Vec<_>>()
            })
            .for_each(|(idx, (ts, pkt))| {
                result[idx] = Some((ts, pkt));
            });
    }
    result
        .into_iter()
        .filter(|x| x.is_some())
        .flatten()
        .collect::<Vec<_>>()
}

#[derive(Default)]
pub struct Channel<R> {
    pub ssrc: u32,
    /// Codec specific delta time, generally (sample rate)/50
    pub delta_time: u32,
    /// starting timestamp
    pub start: u32,
    /// How many ts have arrived
    pub ts: usize,
    pub pkts: VecDeque<R>,
}

impl<R: RtpPacket> Channel<R> {
    pub fn pkt_queue_len(&self) -> usize {
        match (self.pkts.front(), self.pkts.back()) {
            (Some(first), Some(last)) => {
                println!("first: {}, lst: {}", first.ts(), last.ts());
                (last.ts().wrapping_sub(first.ts()) / self.delta_time) as usize + 1
            }
            _ => 0,
        }
    }

    pub fn is_queue_full(&self, max: usize) -> bool {
        // println!("ssrc: {:X}, queue len: {}", self.ssrc, self.pkt_queue_len());
        self.pkt_queue_len() > max
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

    fn find_first_greater_seq_pkt(pkts: &[R], pkt: &R) -> Option<usize> {
        pkts.iter()
            .enumerate()
            .filter(|(_, p)| p.seq() > pkt.seq())
            .next()
            .map(|(idx, _)| idx)
    }

    fn pkt_queue_len(pkts: &VecDeque<R>) -> u16 {
        match (pkts.front(), pkts.back()) {
            (Some(first), Some(last)) => last.seq().wrapping_sub(first.seq()),
            _ => pkts.len() as u16,
        }
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
                let pkts = &mut chl.pkts;
                if let Some(last_seq) = pkts.back().map(|p| p.seq()) {
                    if last_seq + 1 == pkt.seq() {
                        pkts.push_back(pkt);
                    } else {
                        pkts.make_contiguous();
                        let (first, _) = pkts.as_slices();
                        match Self::find_first_greater_seq_pkt(first, &pkt) {
                            Some(gre) => {
                                pkts.insert(gre, pkt);
                            }
                            None => {
                                pkts.push_back(pkt);
                            }
                        };
                    }
                } else {
                    pkts.push_back(pkt);
                }
            }
        };

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

    fn any_queue_full(&self) -> bool {
        self.chls
            .iter()
            .any(|chl| chl.is_queue_full(self.sort_uniq_queue_size))
    }

    fn sort_uniq_add_sildence_frame(pkts: Vec<R>) -> Vec<R> {
        pkts.iter().unique_by(|pkt| pkt.seq());
        vec![]
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
