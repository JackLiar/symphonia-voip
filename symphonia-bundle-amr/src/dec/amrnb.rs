use std::ffi::c_short;

use symphonia_core::audio::{
    AsAudioBufferRef, AudioBuffer, AudioBufferRef, Channels, Signal, SignalSpec,
};
use symphonia_core::codecs::{
    decl_codec_type, CodecDescriptor, CodecParameters, CodecType, Decoder as D, DecoderOptions,
    FinalizeResult,
};
use symphonia_core::errors::Result;
use symphonia_core::formats::Packet;
use symphonia_core::support_codec;

use opencore_amr_sys::{Decoder_Interface_Decode, Decoder_Interface_exit, Decoder_Interface_init};

use crate::{AMR_BUFFER_SIZE, AMR_SAMPLE_RATE};

pub const CODEC_TYPE_AMR: CodecType = decl_codec_type(b"amr");

/// A dummy Decoder struct to handle c_void casting
#[derive(Default)]
struct AmrDecoder;

pub struct Decoder {
    decoded_data: AudioBuffer<c_short>,
    params: CodecParameters,
    st: Box<AmrDecoder>,
}

impl Decoder {
    pub fn new() -> Self {
        unsafe {
            Self {
                decoded_data: AudioBuffer::new(
                    AMR_BUFFER_SIZE,
                    SignalSpec::new(AMR_SAMPLE_RATE, Channels::all()),
                ),
                params: CodecParameters::default(),
                st: Box::from_raw(Decoder_Interface_init() as _),
            }
        }
    }

    pub fn decode(&mut self, data: &[u8]) {
        unsafe {
            Decoder_Interface_Decode(
                self.st.as_mut() as *mut _ as _,
                data.as_ptr(),
                self.decoded_data.chan_mut(0).as_mut_ptr(),
                0,
            )
        }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe {
            Decoder_Interface_exit(self.st.as_mut() as *mut _ as _);
            self.st = Box::new(AmrDecoder::default());
        }
    }
}

impl D for Decoder {
    fn try_new(params: &CodecParameters, _options: &DecoderOptions) -> Result<Self>
    where
        Self: Sized,
    {
        let mut decoder = Self::new();
        decoder.params = params.clone();
        Ok(decoder)
    }

    fn reset(&mut self) {
        unsafe {
            Decoder_Interface_exit(self.st.as_mut() as *mut _ as _);
            self.st = Box::from_raw(Decoder_Interface_init() as _);
        }
        self.decoded_data.clear();
    }

    fn supported_codecs() -> &'static [CodecDescriptor] {
        &[support_codec!(CODEC_TYPE_AMR, "amr", "AMR")]
    }

    fn codec_params(&self) -> &CodecParameters {
        &self.params
    }

    fn decode(&mut self, packet: &Packet) -> Result<AudioBufferRef> {
        self.decoded_data.clear();
        self.decoded_data
            .render_reserved(Some(AMR_BUFFER_SIZE as usize));

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
