pub mod dec;
pub mod format;
pub mod rtp;

pub use dec::{AmrDecoder, AmrwbDecoder, DecoderParams, CODEC_TYPE_AMR, CODEC_TYPE_AMRWB};
pub use format::{AmrReader, AmrwbReader};

const AMR_SAMPLE_RATE: u32 = 8000;
const AMR_BUFFER_SIZE: u64 = AMR_SAMPLE_RATE as u64 / 50;
const AMRWB_SAMPLE_RATE: u32 = 16000;
const AMRWB_BUFFER_SIZE: u64 = AMRWB_SAMPLE_RATE as u64 / 50;
