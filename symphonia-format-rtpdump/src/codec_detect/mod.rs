use crate::rtp::RtpPacket;

mod opus;

trait CodecDetectorTrait {
    fn on_pkt(&mut self, pkt: &dyn RtpPacket);
    fn detect(&self) -> bool;
}
