mod celt;
pub mod dec;
pub mod errors;
mod silk;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub enum Channels {
    #[default]
    /// One channel.
    Mono = 1,
    /// Two channels, left and right.
    Stereo = 2,
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub enum SampleRate {
    #[default]
    Fs8000 = 8000,
    Fs12000 = 12000,
    Fs16000 = 16000,
    Fs24000 = 24000,
    Fs48000 = 48000,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Mode {
    #[default]
    Celt,
    Hybrid,
    Silk,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default)]
pub enum BandWidth {
    #[default]
    /// narrow band
    NB,
    /// medium band
    MB,
    /// wide band
    WB,
    /// super wide band
    SWB,
    /// full band
    FB,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum FrameSize {
    /// 2.5 ms
    FS2_5,
    /// 5 ms
    FS5,
    /// 10 ms
    FS10,
    /// 20 ms
    FS20,
    /// 40 ms
    FS40,
    /// 60 ms
    FS60,
    /// 80 ms
    FS80,
    /// 100 ms
    FS100,
    /// 120 ms
    FS120,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, PartialOrd)]
pub enum Complexity {
    C0,
    C1,
    C2,
    C3,
    C4,
    C5,
    C6,
    C7,
    C8,
    C9,
    C10,
}

/// The available channel setings.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum FrameNum {
    One = 0,
    Two = 1,
    TwoDiff = 2,
    Arbitrary = 3,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Toc(pub u8);

impl Toc {
    fn config(&self) -> u8 {
        self.0 >> 3
    }

    pub fn mode(&self) -> Mode {
        match self.config() {
            c if (0..=11).contains(&c) => Mode::Silk,
            c if (12..=15).contains(&c) => Mode::Hybrid,
            c if (16..=31).contains(&c) => Mode::Celt,
            _ => unreachable!("OPUS config is always less than 32"),
        }
    }

    pub fn bandwidth(&self) -> BandWidth {
        match self.config() {
            c if (0..=3).contains(&c) || (16..=19).contains(&c) => BandWidth::NB,
            c if (4..=7).contains(&c) => BandWidth::MB,
            c if (8..=11).contains(&c) || (20..=23).contains(&c) => BandWidth::WB,
            c if (12..=13).contains(&c) || (24..=27).contains(&c) => BandWidth::SWB,
            c if (14..=15).contains(&c) || (28..=31).contains(&c) => BandWidth::FB,
            _ => unreachable!("OPUS config is always less than 32"),
        }
    }

    pub fn frame_size(&self) -> FrameSize {
        match self.config() {
            c if [16, 20, 24, 28].contains(&c) => FrameSize::FS2_5,
            c if [17, 21, 25, 29].contains(&c) => FrameSize::FS5,
            c if [0, 4, 8, 12, 14, 18, 22, 26, 30].contains(&c) => FrameSize::FS10,
            c if [1, 5, 9, 13, 15, 19, 23, 27, 31].contains(&c) => FrameSize::FS20,
            c if [2, 6, 10].contains(&c) => FrameSize::FS40,
            c if [3, 7, 11].contains(&c) => FrameSize::FS60,
            _ => unreachable!("OPUS config is always less than 32"),
        }
    }

    pub fn channels(&self) -> Channels {
        if ((self.0 >> 2) & 1) == 0 {
            Channels::Mono
        } else {
            Channels::Stereo
        }
    }

    pub fn num_of_frame(&self) -> FrameNum {
        match self.0 & 0b11 {
            0b00 => FrameNum::One,
            0b01 => FrameNum::Two,
            0b10 => FrameNum::TwoDiff,
            0b11 => FrameNum::Arbitrary,
            _ => unreachable!("OPUS num of frame is always less than 4"),
        }
    }

    /// Get samples per frame of specific sample rate
    pub fn samples_per_frame(&self, fs: SampleRate) -> usize {
        let fs = fs as usize;
        match (self.mode(), self.frame_size()) {
            (Mode::Celt, _) => (fs << (self.0 >> 3 & 0x3)) / 400,
            (Mode::Hybrid, FrameSize::FS20) => fs / 50,
            (Mode::Hybrid, FrameSize::FS10) => fs / 100,
            (Mode::Silk, FrameSize::FS60) => fs * 60 / 1000,
            (Mode::Silk, FrameSize::FS10) | (Mode::Silk, FrameSize::FS20) | (Mode::Silk, FrameSize::FS40) => {
                (fs << (self.0 >> 3 & 0x3)) / 100
            }
            _ => unreachable!("no such combinations"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_toc_samples_per_frame() {
        let spfs = (0..32)
            .map(|i| Toc(i << 3).samples_per_frame(SampleRate::Fs8000))
            .collect::<Vec<_>>();

        assert_eq!(
            spfs.as_slice(),
            &[
                80, 160, 320, 480, 80, 160, 320, 480, 80, 160, 320, 480, 80, 160, 80, 160, 20, 40, 80, 160, 20, 40, 80,
                160, 20, 40, 80, 160, 20, 40, 80, 160
            ]
        );
    }
}
