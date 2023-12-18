use num_derive::FromPrimitive;

use evs_codec_sys::{
    frameMode_FRAMEMODE_FUTURE, frameMode_FRAMEMODE_MISSING, frameMode_FRAMEMODE_NORMAL, G192,
    MIME, VOIP_G192_RTP, VOIP_RTPDUMP,
};
use evs_codec_sys::{MODE1, MODE2};

const fn bitrate_to_payload_len(br: u32) -> usize {
    ((br as usize / 50) + 7) / 8
}

#[derive(Clone, Copy, Debug, Default, FromPrimitive)]
#[repr(C)]
pub enum CodecFormat {
    G192 = G192 as _,
    #[default]
    Mime = MIME as _,
    VoipG192Rtp = VOIP_G192_RTP as _,
    VoipRtpdump = VOIP_RTPDUMP as _,
}

#[derive(Clone, Copy, Debug, Default, FromPrimitive)]
pub enum CodecMode {
    #[default]
    Mode1 = MODE1 as _,
    Mode2 = MODE2 as _,
}

#[derive(Clone, Copy, Debug, Default, FromPrimitive)]
pub enum FrameMode {
    Future = frameMode_FRAMEMODE_FUTURE as _,
    Missing = frameMode_FRAMEMODE_MISSING as _,
    #[default]
    Normal = frameMode_FRAMEMODE_NORMAL as _,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, FromPrimitive, PartialEq)]
pub enum PrimaryFrameTypeIndex {
    Primary2800 = 0,
    Primary7200 = 1,
    Primary8000 = 2,
    Primary9600 = 3,
    Primary13200 = 4,
    Primary16400 = 5,
    Primary24400 = 6,
    Primary32000 = 7,
    Primary48000 = 8,
    Primary64000 = 9,
    Primary96000 = 10,
    Primary128000 = 11,
    #[default]
    SID = 12,
    Future = 13,
    SpeechLost = 14,
    NoData = 15,
}

impl PrimaryFrameTypeIndex {
    pub fn bit_rate(self) -> Option<u32> {
        let br: Option<PrimaryBitRate> = self.into();
        br.map(|br| br as u32)
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, FromPrimitive)]
pub enum PrimaryBitRate {
    Primary2800 = 2800,
    Primary7200 = 7200,
    Primary8000 = 8000,
    Primary9600 = 9600,
    Primary13200 = 13200,
    Primary16400 = 16400,
    Primary24400 = 24400,
    Primary32000 = 32000,
    Primary48000 = 48000,
    Primary64000 = 64000,
    Primary96000 = 96000,
    Primary128000 = 128000,
    #[default]
    SID = 2400,
    NoData = 0,
}

impl PrimaryBitRate {
    pub const fn to_payload_size(self) -> usize {
        bitrate_to_payload_len(self as u32)
    }
}

impl From<PrimaryFrameTypeIndex> for Option<PrimaryBitRate> {
    fn from(value: PrimaryFrameTypeIndex) -> Self {
        match value {
            PrimaryFrameTypeIndex::Future | PrimaryFrameTypeIndex::SpeechLost => None,
            PrimaryFrameTypeIndex::NoData => Some(PrimaryBitRate::NoData),
            PrimaryFrameTypeIndex::Primary2800 => Some(PrimaryBitRate::Primary2800),
            PrimaryFrameTypeIndex::Primary7200 => Some(PrimaryBitRate::Primary7200),
            PrimaryFrameTypeIndex::Primary8000 => Some(PrimaryBitRate::Primary8000),
            PrimaryFrameTypeIndex::Primary9600 => Some(PrimaryBitRate::Primary9600),
            PrimaryFrameTypeIndex::Primary13200 => Some(PrimaryBitRate::Primary13200),
            PrimaryFrameTypeIndex::Primary16400 => Some(PrimaryBitRate::Primary16400),
            PrimaryFrameTypeIndex::Primary24400 => Some(PrimaryBitRate::Primary24400),
            PrimaryFrameTypeIndex::Primary32000 => Some(PrimaryBitRate::Primary32000),
            PrimaryFrameTypeIndex::Primary48000 => Some(PrimaryBitRate::Primary48000),
            PrimaryFrameTypeIndex::Primary64000 => Some(PrimaryBitRate::Primary64000),
            PrimaryFrameTypeIndex::Primary96000 => Some(PrimaryBitRate::Primary96000),
            PrimaryFrameTypeIndex::Primary128000 => Some(PrimaryBitRate::Primary128000),
            PrimaryFrameTypeIndex::SID => Some(PrimaryBitRate::SID),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, FromPrimitive, PartialEq)]
pub enum AMRWBIOFrameTypeIndex {
    AMRWBIO6600 = 0,
    AMRWBIO8850 = 1,
    AMRWBIO12650 = 2,
    AMRWBIO14250 = 3,
    AMRWBIO15850 = 4,
    AMRWBIO18250 = 5,
    AMRWBIO19850 = 6,
    AMRWBIO23050 = 7,
    AMRWBIO23850 = 8,
    #[default]
    SID = 9,
    Future10 = 10,
    Future11 = 11,
    Future12 = 12,
    Future13 = 13,
    SpeechLost = 14,
    NoData = 15,
}

impl AMRWBIOFrameTypeIndex {
    pub fn bit_rate(self) -> Option<u32> {
        let br: Option<AMRWBIOBitRate> = self.into();
        br.map(|br| br as u32)
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, FromPrimitive)]
pub enum AMRWBIOBitRate {
    AMRWBIO6600 = 6600,
    AMRWBIO8850 = 8850,
    AMRWBIO12650 = 12650,
    AMRWBIO14250 = 14250,
    AMRWBIO15850 = 15850,
    AMRWBIO18250 = 18250,
    AMRWBIO19850 = 19850,
    AMRWBIO23050 = 23050,
    AMRWBIO23850 = 23850,
    #[default]
    SID = 1750,
    NoData = 0,
}

impl AMRWBIOBitRate {
    pub const fn to_payload_size(self) -> usize {
        bitrate_to_payload_len(self as u32)
    }
}

impl From<AMRWBIOFrameTypeIndex> for Option<AMRWBIOBitRate> {
    fn from(value: AMRWBIOFrameTypeIndex) -> Self {
        match value {
            AMRWBIOFrameTypeIndex::Future10
            | AMRWBIOFrameTypeIndex::Future11
            | AMRWBIOFrameTypeIndex::Future12
            | AMRWBIOFrameTypeIndex::Future13
            | AMRWBIOFrameTypeIndex::SpeechLost => None,
            AMRWBIOFrameTypeIndex::NoData => Some(AMRWBIOBitRate::NoData),
            AMRWBIOFrameTypeIndex::AMRWBIO6600 => Some(AMRWBIOBitRate::AMRWBIO6600),
            AMRWBIOFrameTypeIndex::AMRWBIO8850 => Some(AMRWBIOBitRate::AMRWBIO8850),
            AMRWBIOFrameTypeIndex::AMRWBIO12650 => Some(AMRWBIOBitRate::AMRWBIO12650),
            AMRWBIOFrameTypeIndex::AMRWBIO14250 => Some(AMRWBIOBitRate::AMRWBIO14250),
            AMRWBIOFrameTypeIndex::AMRWBIO15850 => Some(AMRWBIOBitRate::AMRWBIO15850),
            AMRWBIOFrameTypeIndex::AMRWBIO18250 => Some(AMRWBIOBitRate::AMRWBIO18250),
            AMRWBIOFrameTypeIndex::AMRWBIO19850 => Some(AMRWBIOBitRate::AMRWBIO19850),
            AMRWBIOFrameTypeIndex::AMRWBIO23050 => Some(AMRWBIOBitRate::AMRWBIO23050),
            AMRWBIOFrameTypeIndex::AMRWBIO23850 => Some(AMRWBIOBitRate::AMRWBIO23850),
            AMRWBIOFrameTypeIndex::SID => Some(AMRWBIOBitRate::SID),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameTypeIndex {
    Primary(PrimaryFrameTypeIndex),
    AMRWBIO(AMRWBIOFrameTypeIndex),
}

impl FrameTypeIndex {
    pub fn bit_rate(self) -> Option<u32> {
        match self {
            Self::AMRWBIO(ft) => ft.bit_rate(),
            Self::Primary(ft) => ft.bit_rate(),
        }
    }

    pub fn missing(self) -> bool {
        match self {
            Self::AMRWBIO(ft) => ft == AMRWBIOFrameTypeIndex::SpeechLost,
            Self::Primary(ft) => ft == PrimaryFrameTypeIndex::SpeechLost,
        }
    }

    pub fn sid(self) -> bool {
        match self {
            Self::AMRWBIO(ft) => ft == AMRWBIOFrameTypeIndex::SpeechLost,
            Self::Primary(ft) => ft == PrimaryFrameTypeIndex::SpeechLost,
        }
    }
}

impl From<FrameTypeIndex> for u8 {
    fn from(value: FrameTypeIndex) -> Self {
        match value {
            FrameTypeIndex::AMRWBIO(ft) => ft as u8,
            FrameTypeIndex::Primary(ft) => ft as u8,
        }
    }
}
