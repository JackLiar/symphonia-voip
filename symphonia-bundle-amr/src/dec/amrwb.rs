use std::ffi::{c_int, c_short};

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

use opencore_amr_sys::{D_IF_decode, D_IF_exit, D_IF_init};

use crate::format::AmrwbToc;
use crate::{AMRWB_BUFFER_SIZE, AMRWB_SAMPLE_RATE};

pub const CODEC_TYPE_AMRWB: CodecType = decl_codec_type(b"amrwb");

/// A dummy Decoder struct to handle c_void casting
#[derive(Default)]
struct AmrwbDecoder;

pub struct Decoder {
    decoded_data: AudioBuffer<c_short>,
    params: CodecParameters,
    st: Box<AmrwbDecoder>,
}

impl Default for Decoder {
    fn default() -> Self {
        unsafe {
            Self {
                decoded_data: AudioBuffer::new(
                    AMRWB_BUFFER_SIZE,
                    SignalSpec::new(AMRWB_SAMPLE_RATE, Channels::FRONT_CENTRE),
                ),
                params: CodecParameters::default(),
                st: Box::from_raw(D_IF_init().cast()),
            }
        }
    }
}

impl Decoder {
    pub fn decode(&mut self, data: &[u8]) {
        let toc = AmrwbToc(data[0]);
        unsafe {
            D_IF_decode(
                (self.st.as_mut() as *mut AmrwbDecoder).cast(),
                data.as_ptr(),
                self.decoded_data.chan_mut(0).as_mut_ptr(),
                !toc.q() as c_int,
            )
        }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe {
            D_IF_exit((self.st.as_mut() as *mut AmrwbDecoder).cast());
            self.st = Box::<AmrwbDecoder>::default();
        }
    }
}

impl D for Decoder {
    fn try_new(params: &CodecParameters, _options: &DecoderOptions) -> Result<Self>
    where
        Self: Sized,
    {
        let mut decoder = Self::default();
        decoder.params = params.clone();
        Ok(decoder)
    }

    fn reset(&mut self) {
        unsafe {
            D_IF_exit((self.st.as_mut() as *mut AmrwbDecoder).cast());
            self.st = Box::from_raw(D_IF_init().cast());
        }
        self.decoded_data.clear();
    }

    fn supported_codecs() -> &'static [CodecDescriptor] {
        &[support_codec!(CODEC_TYPE_AMRWB, "amrwb", "AMRWB")]
    }

    fn codec_params(&self) -> &CodecParameters {
        &self.params
    }

    fn decode(&mut self, packet: &Packet) -> Result<AudioBufferRef> {
        self.decoded_data.clear();
        self.decoded_data
            .render_reserved(Some(AMRWB_BUFFER_SIZE as usize));

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
