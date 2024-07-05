#[macro_use]
extern crate num_derive;

use num_traits::FromPrimitive;

mod consts;
pub mod dec;
pub mod format;
pub mod rtp;
mod utils;

use consts::{
    AMRWBIOBitRate, AMRWBIOFrameTypeIndex, FrameTypeIndex, PrimaryBitRate, PrimaryFrameTypeIndex,
};

/* See RFC 4867 section 5.3 */
#[derive(Clone, Copy, Debug)]
struct AmrToc(pub u8);

impl AmrToc {
    /// padding
    pub fn padding(&self) -> u8 {
        self.0 & 0b00000011
    }

    /// Frame quality indicator
    pub fn q(&self) -> bool {
        ((self.0 >> 2) & 0x01) == 1
    }

    /// Frame type index
    pub fn ft(&self) -> usize {
        ((self.0 >> 3) & 0x0f) as usize
    }

    /// Frame followed by another speech frame
    pub fn followed(&self) -> bool {
        (self.0 >> 7) == 1
    }
}

/* 3GPP TS 26.445 A.2.2.1.1 */
#[derive(Clone, Copy, Debug)]
struct EvsCmr(pub u8);

impl EvsCmr {
    /// D the requested frametype
    pub fn frame_type(&self) -> usize {
        (self.0 & 0x0f) as usize
    }

    /// T type of request
    pub fn r#type(&self) -> u8 {
        0
    }

    // H
    pub fn header_type(&self) -> u8 {
        0
    }
}

/* 3GPP TS 26.445 A.2.2.1.2 */
struct EvsToc(pub u8);

impl EvsToc {
    /// Header type, always 0
    pub fn header_type(&self) -> bool {
        (self.0 & 0x80) == 0x80
    }

    /// Followed by another speech data
    pub fn followed(&self) -> bool {
        (self.0 & 0x40) == 0x40
    }

    /// EVS mode bit
    pub fn is_amrwb(&self) -> bool {
        (self.0 & 0x20) == 0x20
    }

    /// AMRWB Q bit
    pub fn quality(&self) -> bool {
        if self.is_amrwb() {
            ((self.0 & 0x10) >> 4) == 1
        } else {
            true
        }
    }

    /// EVS frametype (~FT in amr toc)
    pub fn frame_type(&self) -> FrameTypeIndex {
        if self.is_amrwb() {
            match AMRWBIOFrameTypeIndex::from_u8(self.0 & 0x0f) {
                None => unreachable!("Frame type index always valid"),
                Some(fti) => FrameTypeIndex::AMRWBIO(fti),
            }
        } else {
            match PrimaryFrameTypeIndex::from_u8(self.0 & 0x0f) {
                None => unreachable!("Frame type index always valid"),
                Some(fti) => FrameTypeIndex::Primary(fti),
            }
        }
    }

    /// Get payload size of current speech data
    pub fn payload_size(&self) -> Option<usize> {
        match self.frame_type() {
            FrameTypeIndex::AMRWBIO(ft) => {
                let br: Option<AMRWBIOBitRate> = ft.into();
                br.map(|br| br.to_payload_size())
            }
            FrameTypeIndex::Primary(ft) => {
                let br: Option<PrimaryBitRate> = ft.into();
                br.map(|br| br.to_payload_size())
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_amr_toc() {}

    #[test]
    fn test_evs_toc() {}
}
