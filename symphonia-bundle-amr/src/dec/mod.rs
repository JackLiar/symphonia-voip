mod amrnb;
mod amrwb;

pub use amrnb::{Decoder as AmrDecoder, CODEC_TYPE_AMR};
pub use amrwb::{Decoder as AmrwbDecoder, CODEC_TYPE_AMRWB};
