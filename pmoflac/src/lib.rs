pub mod decoder;
pub mod encoder;
pub mod error;
mod pcm;
mod stream;
mod util;

pub use decoder::{decode_flac_stream, FlacDecodedStream};
pub use encoder::{encode_flac_stream, EncoderOptions, FlacEncodedStream};
pub use error::FlacError;
pub use pcm::{PcmFormat, StreamInfo};
