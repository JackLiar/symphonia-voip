use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum Error {
    #[error("Invalid Opus decoder param: {0}")]
    InvalidDecoderParam(String),
    #[error("Invalid Opus packet: {0}")]
    InvalidPacket(String),
}

pub type Result<T> = std::result::Result<T, Error>;
