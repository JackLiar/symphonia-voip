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
use symphonia_core::units::TimeBase;

use opencore_amr_sys::{D_IF_decode, D_IF_exit, D_IF_init};

pub const CODEC_TYPE_AMRWB: CodecType = decl_codec_type(b"amrwb");

/// A dummy Decoder struct to handle c_void casting
#[derive(Default)]
struct AmrwbDecoder;

pub struct Decoder {
    decoded_data: AudioBuffer<c_short>,
    params: CodecParameters,
    st: Box<AmrwbDecoder>,
}

impl Decoder {
    pub fn new() -> Self {
        unsafe {
            Self {
                decoded_data: AudioBuffer::new(320, SignalSpec::new(16000, Channels::all())),
                params: CodecParameters::default(),
                st: Box::from_raw(D_IF_init() as *mut _),
            }
        }
    }

    pub fn decode(&mut self, data: &[u8]) {
        unsafe {
            D_IF_decode(
                self.st.as_mut() as *mut AmrwbDecoder as _,
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
            D_IF_exit(self.st.as_mut() as *mut AmrwbDecoder as _);
            self.st = Box::new(AmrwbDecoder::default());
        }
    }
}

impl D for Decoder {
    fn try_new(params: &CodecParameters, options: &DecoderOptions) -> Result<Self>
    where
        Self: Sized,
    {
        let mut decoder = Self::new();
        decoder.params.codec = CODEC_TYPE_AMRWB;
        decoder.params.channels = Some(Channels::FRONT_CENTRE);
        decoder
            .params
            .with_sample_rate(16000)
            .with_time_base(TimeBase::new(1, 16000));
        Ok(decoder)
    }

    fn reset(&mut self) {
        unsafe {
            D_IF_exit(self.st.as_mut() as *mut AmrwbDecoder as _);
            self.st = Box::from_raw(D_IF_init() as *mut _);
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
        self.decoded_data.render_reserved(Some(320));

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
