use std::mem::size_of;
use std::num::NonZeroUsize;

use symphonia_core::audio::{
    AsAudioBufferRef, AudioBuffer, AudioBufferRef, Channels, Signal, SignalSpec,
};
use symphonia_core::codecs::{
    decl_codec_type, CodecDescriptor, CodecParameters, CodecType, Decoder as D, DecoderOptions,
    FinalizeResult,
};
use symphonia_core::errors::{decode_error, Error, Result};
use symphonia_core::formats::Packet;
use symphonia_core::support_codec;

use evs_codec_sys::{
    amr_wb_dec, evs_dec, init_decoder, read_indices_from_djb, reset_indices_dec, syn_output,
    Decoder_State, Word16, Word32, MIME,
};

use crate::consts::{CodecFormat, FrameMode, FrameTypeIndex};
use crate::utils::u8_slice_to_any;
use crate::{AmrToc, EvsToc};

pub const CODEC_TYPE_EVS: CodecType = decl_codec_type(b"evs");

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct DecoderParams {
    pub format: CodecFormat,
    pub channel: NonZeroUsize,
    pub sample_rate: Option<u32>,
    pub is_dtx_enabled: bool,
}

impl Default for DecoderParams {
    fn default() -> Self {
        Self {
            format: Default::default(),
            channel: unsafe { NonZeroUsize::new_unchecked(1) },
            is_dtx_enabled: false,
            sample_rate: None,
        }
    }
}

#[derive(Clone)]
pub struct Decoder {
    decode_param: DecoderParams,
    params: CodecParameters,
    raw: Decoder_State,
    decoded_len: usize,
    output: [f32; 128000 / 50],
    decoded_data: AudioBuffer<i16>,
}

impl Default for Decoder {
    fn default() -> Self {
        Self {
            decode_param: Default::default(),
            params: CodecParameters::default(),
            raw: Decoder_State::default(),
            decoded_len: 0,
            output: [0.0; 128000 / 50],
            decoded_data: AudioBuffer::new(960, SignalSpec::new(1, Channels::all())),
        }
    }
}

unsafe impl Send for Decoder {}
unsafe impl Sync for Decoder {}

impl Decoder {
    pub fn samples_rate(&self) -> u32 {
        self.raw.output_Fs as u32
    }

    pub fn samples_per_frame(&self) -> u32 {
        self.raw.output_Fs as u32 / 50
    }
}

impl D for Decoder {
    fn try_new(params: &CodecParameters, options: &DecoderOptions) -> Result<Self> {
        let param =
            unsafe { u8_slice_to_any::<DecoderParams>(params.extra_data.as_ref().unwrap()) };
        let mut decoder = Self::default();
        decoder.decode_param = param.clone();
        decoder.decoded_data =
            AudioBuffer::new(960, SignalSpec::new(16000, Channels::FRONT_CENTRE));

        decoder.raw.bitstreamformat = MIME as Word16;
        decoder.raw.output_Fs = 16000;
        unsafe {
            // decoder.raw.cldfbAna = std::ptr::null_mut();
            // decoder.raw.cldfbBPF = std::ptr::null_mut();
            // decoder.raw.cldfbSyn = std::ptr::null_mut();
            // decoder.raw.hFdCngDec = std::ptr::null_mut();
            init_decoder(&mut decoder.raw);
            reset_indices_dec(&mut decoder.raw);
        }
        Ok(decoder)
    }

    fn reset(&mut self) {
        self.decoded_len = self.samples_per_frame() as usize;
        self.output = [0.0; 128000 / 50];
    }

    fn supported_codecs() -> &'static [CodecDescriptor] {
        &[support_codec!(CODEC_TYPE_EVS, "evs", "EVS")]
    }

    fn codec_params(&self) -> &CodecParameters {
        &self.params
    }

    fn decode(&mut self, packet: &Packet) -> Result<AudioBufferRef> {
        match self.decode_param.format {
            CodecFormat::Mime => self.decode_mime(packet),
            _ => unimplemented!(),
        }
    }

    fn finalize(&mut self) -> FinalizeResult {
        Default::default()
    }

    fn last_decoded(&self) -> AudioBufferRef {
        self.decoded_data.as_audio_buffer_ref()
    }
}

impl Decoder {
    fn decode_mime(&mut self, packet: &Packet) -> Result<AudioBufferRef> {
        if !packet.data.is_empty() {
            self.check(packet)?;
        }

        self.reset();

        unsafe {
            evs_dec(
                &mut self.raw,
                self.output.as_mut_ptr(),
                FrameMode::Normal as _,
            );

            self.decoded_data.clear();
            self.decoded_data
                .render_reserved(Some(self.raw.output_Fs as usize / 50));

            syn_output(
                self.output.as_mut_ptr(),
                (self.raw.output_Fs / 50) as Word16,
                self.decoded_data
                    .chan_mut(packet.track_id() as _)
                    .as_mut_ptr()
                    .cast(),
            );
            // println!(
            //     "decoded len: {}, frames: {}, capacity: {}",
            //     self.decoded_data.chan(packet.track_id() as _).len(),
            //     self.decoded_data.frames(),
            //     self.decoded_data.capacity(),
            // );
        }

        Ok(self.decoded_data.as_audio_buffer_ref())
    }

    fn check(&mut self, packet: &Packet) -> Result<()> {
        let mut data = packet.buf();
        let is_amrwb: bool;
        let frame_type: usize;
        let qbit: bool;
        let total_bitrate: i32;

        // if self.raw.amrwb_rfc4867_flag != 0 {
        //     let toc = AmrToc(data[0]);
        //     is_amrwb = true;
        //     frame_type = toc.ft();
        //     qbit = toc.q();
        //     total_bitrate = 0;
        //     data = &data[size_of::<AmrToc>()..];
        // }
        let toc = EvsToc(data[0]);
        is_amrwb = toc.is_amrwb();
        let ft: u8 = toc.frame_type().into();
        frame_type = ft as usize;
        qbit = toc.quality();

        let total_bitrate = toc
            .frame_type()
            .bit_rate()
            .ok_or_else(|| Error::DecodeError("Invalid bitrate"))?;

        data = &data[size_of::<EvsToc>()..];

        let frame_len = match toc.payload_size() {
            None => return Err(Error::DecodeError("Future use or speech lost")),
            Some(size) => size,
        };

        if data.len() < frame_len {
            eprintln!(
                "Invalid packet {} < {} + {}",
                packet.data.len(),
                frame_len,
                packet.data.len() - data.len(),
            );
            return Err(Error::DecodeError("Invalid packet len"));
        }

        self.raw.Opt_AMR_WB = is_amrwb as Word16;

        // println!("data len: {}", data.len());
        // println!("total bitrate: {}", total_bitrate);
        // println!("is amrwb: {}", is_amrwb);
        // println!("frame type: {}", frame_type);
        // println!("qbit: {}", qbit);
        unsafe {
            read_indices_from_djb(
                &mut self.raw,
                data.as_ptr().cast_mut(),
                total_bitrate as Word32 / 50,
                is_amrwb as Word16,
                frame_type as Word16,
                qbit as Word16,
                0,
                0,
            );
        }

        Ok(())
    }
}
