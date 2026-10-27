//! Content addresses: BLAKE3 hashes identifying blobs.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// The BLAKE3 hash of a blob's bytes. Equal content always has an equal id, which is what deduplicates storage.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlobId(pub [u8; 32]);

impl BlobId {
    /// Hashes `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }
}

impl fmt::Display for BlobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

impl fmt::Debug for BlobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlobId({self})")
    }
}

impl Serialize for BlobId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for BlobId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl de::Visitor<'_> for V {
            type Value = BlobId;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("32 bytes")
            }
            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<BlobId, E> {
                v.try_into().map(BlobId).map_err(|_| E::invalid_length(v.len(), &self))
            }
        }
        d.deserialize_bytes(V)
    }
}
