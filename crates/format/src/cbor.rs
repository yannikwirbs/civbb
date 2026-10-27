//! CBOR encoding helpers with bounded decoding for untrusted input.

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Maximum nesting depth accepted when decoding; deep nesting is a stack-exhaustion vector.
pub const MAX_DEPTH: usize = 64;

/// CBOR encode or decode failure.
#[derive(Debug, thiserror::Error)]
#[error("cbor: {0}")]
pub struct CborError(String);

/// Encodes `value` as CBOR.
pub fn to_vec<T: Serialize>(value: &T) -> Vec<u8> {
    let mut out = Vec::new();
    ciborium::into_writer(value, &mut out).expect("writing CBOR to a Vec cannot fail");
    out
}

/// Decodes CBOR from `bytes`, rejecting input longer than `max_len` or nested deeper than [`MAX_DEPTH`].
pub fn from_slice<T: DeserializeOwned>(bytes: &[u8], max_len: usize) -> Result<T, CborError> {
    if bytes.len() > max_len {
        return Err(CborError(format!("{} bytes exceeds limit {max_len}", bytes.len())));
    }
    ciborium::de::from_reader_with_recursion_limit(bytes, MAX_DEPTH).map_err(|e| CborError(e.to_string()))
}
