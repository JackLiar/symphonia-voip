mod consts;
mod dec;
mod enc;

#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub enum Channels {
    #[default]
    Mono = 1,
    Stereo = 2,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum SampleRate {
    Fs8000 = 8000,
    Fs12000 = 12000,
    Fs16000 = 16000,
    Fs24000 = 24000,
    Fs32000 = 32000,
    Fs44100 = 44100,
    #[default]
    Fs48000 = 48000,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum InternalSampleRate {
    Fs8000 = 8000,
    Fs12000 = 12000,
    #[default]
    Fs16000 = 16000,
}

/// Aka Packet Rate
#[derive(Clone, Copy, Debug, Default)]
pub enum FrameDuration {
    Ms10 = 10,
    #[default]
    Ms20 = 20,
    Ms40 = 40,
    Ms60 = 60,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ControlParameter {
    pub channels: Channels,
    pub channels_internal: Channels,
    pub sample_rate: SampleRate,
    pub sample_rate_internal: InternalSampleRate,
    pub frame_duration: FrameDuration,
    pub prev_pitch_lag: i32,
    pub enable_deep_plc: bool,
}

/// Packet Loss Concealment
pub struct PLC {
    pub pitch_lag_q8: i32,
    pub ltp_coef_q14: [i16; 5],
    pub prev_lpc_q12: [i16; 16],
    pub last_frame_lost: bool,
    pub rand_seed: i32,
    pub rand_scale_q14: i16,
    pub conc_energy: i32,
    pub conc_energy_shift: i32,
    pub prev_ltp_scale_q14: i16,
    pub prev_gain_q16: [i32; 2],
    pub sample_rate: SampleRate,
    pub nb_subfr: u32,
    pub subfr_len: u32,
    pub enable_deep_plc: bool,
}
