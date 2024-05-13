use std::os::raw::c_int;

use symphonia_core::audio::{
    AsAudioBufferRef, AudioBuffer, AudioBufferRef, Channels, Signal, SignalSpec,
};
use symphonia_core::codecs::{
    decl_codec_type, CodecDescriptor, CodecParameters, CodecType, Decoder as D, DecoderOptions,
    FinalizeResult,
};
use symphonia_core::errors::{Error, Result};
use symphonia_core::formats::Packet;
use symphonia_core::support_codec;

use libg7221_sys::*;

const G722_1_SAMPLE_RATE_16000: u32 = g722_1_sample_rates_t_G722_1_SAMPLE_RATE_16000;
const G722_1_SAMPLE_RATE_32000: u32 = g722_1_sample_rates_t_G722_1_SAMPLE_RATE_32000;
const G722_1_BIT_RATE_24000: u32 = g722_1_bit_rates_t_G722_1_BIT_RATE_24000;
const G722_1_BIT_RATE_32000: u32 = g722_1_bit_rates_t_G722_1_BIT_RATE_32000;
const G722_1_BIT_RATE_48000: u32 = g722_1_bit_rates_t_G722_1_BIT_RATE_48000;

pub const CODEC_TYPE_G722_1: CodecType = decl_codec_type(b"g7221");

pub struct Decoder {
    decoded_data: AudioBuffer<i16>,
    params: CodecParameters,
    st: g722_1_decode_state_t,
    sample_rate: u32,
    bit_per_sample: u32,
}

unsafe impl Send for Decoder {}
unsafe impl Sync for Decoder {}

impl Decoder {
    pub fn new() -> Self {
        Self {
            decoded_data: AudioBuffer::new(
                G722_1_SAMPLE_RATE_16000 as u64 / 50,
                SignalSpec::new(G722_1_SAMPLE_RATE_16000, Channels::FRONT_CENTRE),
            ),
            params: CodecParameters::default(),
            st: g722_1_decode_state_t::default(),
            sample_rate: G722_1_SAMPLE_RATE_16000,
            bit_per_sample: G722_1_BIT_RATE_24000,
        }
    }

    pub fn decode(&mut self, data: &[u8]) {
        unsafe {
            let sample_cnt = g722_1_decode(
                &mut self.st,
                self.decoded_data.chan_mut(0).as_mut_ptr(),
                data.as_ptr().cast_mut(),
                data.len() as _,
            );
        }
    }
}

impl D for Decoder {
    fn try_new(params: &CodecParameters, _options: &DecoderOptions) -> Result<Self>
    where
        Self: Sized,
    {
        let mut decoder = Self::new();
        decoder.sample_rate = match params.sample_rate {
            Some(sr) if sr == G722_1_SAMPLE_RATE_16000 || sr == G722_1_SAMPLE_RATE_32000 => sr,
            _ => {
                return Err(Error::Unsupported(
                    "Unsupported sample rate or no sample rate is provided",
                ))
            }
        };
        decoder.bit_per_sample = match params.bits_per_sample {
            Some(br)
                if br == g722_1_bit_rates_t_G722_1_BIT_RATE_24000
                    || br == g722_1_bit_rates_t_G722_1_BIT_RATE_32000
                    || br == g722_1_bit_rates_t_G722_1_BIT_RATE_48000 =>
            {
                br
            }
            _ => {
                return Err(Error::Unsupported(
                    "Unsupported bit rate or no bit rate is provided",
                ))
            }
        };
        decoder.params = params.clone();
        unsafe {
            let r = g722_1_decode_init(
                &mut decoder.st,
                decoder.bit_per_sample as _,
                decoder.sample_rate as _,
            );
            if r.is_null() {
                return Err(Error::DecodeError("Failed to initialize G.722.1 Decoder"));
            }
        }
        Ok(decoder)
    }

    fn reset(&mut self) {
        std::mem::take(&mut self.st);
        unsafe {
            let r = g722_1_decode_init(
                &mut self.st,
                self.bit_per_sample as _,
                self.sample_rate as _,
            );
            if r.is_null() {
                panic!("Failed to initialize G.722.1 Decoder");
            }
        }
    }

    fn supported_codecs() -> &'static [CodecDescriptor] {
        &[support_codec!(CODEC_TYPE_G722_1, "g722.1", "G.722.1")]
    }

    fn codec_params(&self) -> &CodecParameters {
        &self.params
    }

    fn decode(&mut self, packet: &Packet) -> Result<AudioBufferRef> {
        self.decoded_data.clear();
        self.decoded_data
            .render_reserved(Some(self.sample_rate as usize / 50));

        self.decode(&packet.data);

        Ok(self.decoded_data.as_audio_buffer_ref())
    }

    fn finalize(&mut self) -> FinalizeResult {
        Default::default()
    }

    fn last_decoded(&self) -> AudioBufferRef {
        self.decoded_data.as_audio_buffer_ref()
    }
}
