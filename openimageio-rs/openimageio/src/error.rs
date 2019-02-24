use crate::cstring_to_owned;
use std::error;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Error {
    OpenError(String),
    WriteError(String),
    ReadError(String),
    SubimageNotFound,
    ChannelNotFound,
    InvalidChannelIndex,
    InvalidAttributeNameOrType,
    BufferTooSmall,
}

impl error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match *self {
            Error::OpenError(ref msg) => write!(f, "error opening image: {}", msg),
            Error::WriteError(ref msg) => write!(f, "error writing image data: {}", msg),
            Error::ReadError(ref msg) => write!(f, "error reading image data: {}", msg),
            Error::SubimageNotFound => write!(f, "non-existent subimage"),
            Error::ChannelNotFound => write!(f, "non-existent channel"),
            Error::InvalidAttributeNameOrType => {
                write!(f, "non-existent attribute or incorrect type for attribute")
            }
            Error::InvalidChannelIndex => write!(f, "non-existent channel index"),
            Error::BufferTooSmall => write!(f, "buffer was too small"),
            //_ => write!(f, "Unknown error."),
        }
    }
}

pub fn get_last_error() -> String {
    unsafe { cstring_to_owned(openimageio_sys::OIIO_geterror()) }
}
