#[cfg(feature = "alloc")]
mod bt4;
#[cfg(feature = "alloc")]
mod hash234;
#[cfg(feature = "alloc")]
mod hc4;
#[cfg_attr(feature = "alloc", path = "lz_decoder_alloc.rs")]
#[cfg_attr(not(feature = "alloc"), path = "lz_decoder_no_alloc.rs")]
mod lz_decoder;
#[cfg(feature = "alloc")]
mod lz_encoder;
pub use lz_decoder::*;
#[cfg(feature = "alloc")]
pub use lz_encoder::*;
