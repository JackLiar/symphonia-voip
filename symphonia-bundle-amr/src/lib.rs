pub mod dec;
pub mod format;

pub use dec::{AmrDecoder, AmrwbDecoder, CODEC_TYPE_AMR, CODEC_TYPE_AMRWB};
pub use format::{AmrReader, AmrwbReader};
