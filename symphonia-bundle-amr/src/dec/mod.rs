mod amrnb;
mod amrwb;

pub use amrnb::{Decoder as AmrDecoder, CODEC_TYPE_AMR};
pub use amrwb::{Decoder as AmrwbDecoder, CODEC_TYPE_AMRWB};

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct DecoderParams {
    pub octet_align: bool,
}
