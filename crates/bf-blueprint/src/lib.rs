//! PT: Codec da blueprint string do Factorio com limites de segurança. Toda entrada é tratada
//!     como hostil: tamanho, descompressão, contagens, profundidade, posições e fios são
//!     conferidos antes de qualquer uso.
//! EN: Factorio blueprint string codec with safety limits. Every input is treated as hostile:
//!     size, decompression, counts, depth, positions and wires are checked before any use.

mod bounded;
pub mod codec;
pub mod error;
pub mod legacy;
pub mod limits;
pub mod model;
mod validate;

pub use codec::{Decoded, Warning, decode, encode, encode_raw_json, precheck};
pub use error::{DecodeError, EncodeError};
pub use limits::Limits;
pub use model::*;
