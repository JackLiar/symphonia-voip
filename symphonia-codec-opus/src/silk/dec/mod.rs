use std::io::Write;

use super::{Channels, ControlParameter, SampleRate};

#[derive(Clone, Copy, Debug, Default)]
pub struct Channel {
    pub prev_gain_q16: i32,
    // pub exc_q145: [i32; 320],
    pub s_lpc_q14_buf: [i32; 16],
    // pub out_buf: [i16; 480],
    pub lag_prev: i32,
    pub last_gain_idx: i8,
    pub sample_rate: SampleRate,
    pub fs_api_hz: i32,
    pub nb_subfr: i32,
    pub frame_len: i32,
    pub subfr_len: i32,
    pub ltp_mem_length: i32,
    pub lpc_order: i32,
    pub prev_nlsf_q15: [i16; 16],
    pub first_frame_after_reset: bool,
    // pub pitch_lag_low_bits_icdf:
    // pub pitch_contour_icdf:
    pub decoded_frames_num: i32,
    pub frames_per_pkt: i32,
    pub ec_prev_signal_type: i32,
    pub ec_prev_lag_index: i32,
    pub vad_flags: [i32; 3],
    pub lbrr_flag: i32,
    pub lbrr_flags: [i32; 3],

    pub loss_cnt: u32,
    pub prev_signal_type: i32,
    pub arch: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Decoder {
    pub channels: [Channel; 2],
    // stereo fields
    pub pred_prev_q13: [i16; 2],
    pub smid: [i16; 2],
    pub sside: [i16; 2],

    pub nchannels: Channels,
    pub channels_internal: Channels,
    pub prev_decode_only_middle: bool,
}

impl Decoder {
    pub fn reset(&mut self) {
        self.pred_prev_q13 = [0; 2];
        self.smid = [0; 2];
        self.sside = [0; 2];
        self.prev_decode_only_middle = false;
    }

    pub fn decode<W: Write>(&mut self, ctl: &ControlParameter, lost: bool, first_pkt: bool, w: &mut W) {
        if first_pkt {
            for chl in self.channels.iter_mut().take(self.nchannels as usize) {
                chl.decoded_frames_num = 0;
            }
        }

        if ctl.channels_internal > self.channels_internal {
            todo!("init self.channels[1]");
        }

        let stereo2mono = ctl.channels_internal == Channels::Mono
            && self.channels_internal == Channels::Stereo
            && (ctl.sample_rate_internal as u32 == self.channels[0].sample_rate as u32);
    }
}
