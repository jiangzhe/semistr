use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Error)]
pub enum Error {
    #[error("string too long, length is {0}")]
    StringTooLong(usize),
    #[error("invalid utf-8 string")]
    InvalidUtf8String,
}