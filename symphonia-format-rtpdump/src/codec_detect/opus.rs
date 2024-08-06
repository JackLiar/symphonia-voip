use std::collections::HashSet;

use super::CodecDetectorTrait;

struct OpusDetector {
    modes: HashSet<u8>,
    bandwidths: HashSet<u8>,
    frame_sizes: HashSet<u8>,
    channels: HashSet<u8>,
    num_of_frames: HashSet<u8>,
}

impl CodecDetectorTrait for OpusDetector {
    fn on_pkt(&mut self, pkt: &dyn crate::rtp::RtpPacket) {}

    fn detect(&self) -> bool {
        if self.modes.len() > 1 {
            return false;
        }
        if self.bandwidths.len() > 1 {
            return false;
        }
        if self.frame_sizes.len() > 1 {
            return false;
        }
        if self.channels.len() > 1 {
            return false;
        }
        self.num_of_frames.len() <= 1
    }
}
