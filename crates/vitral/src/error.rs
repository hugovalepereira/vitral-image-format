use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// The main PNG datastream could not be decoded or encoded.
    Png(String),
    /// Arguments are inconsistent (e.g. buffer sizes do not match dimensions).
    InvalidInput(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Png(msg) => write!(f, "PNG error: {msg}"),
            Error::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<png::DecodingError> for Error {
    fn from(e: png::DecodingError) -> Self {
        Error::Png(e.to_string())
    }
}

impl From<png::EncodingError> for Error {
    fn from(e: png::EncodingError) -> Self {
        Error::Png(e.to_string())
    }
}
