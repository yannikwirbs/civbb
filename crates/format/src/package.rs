//! Runtime packages: a signed CBOR manifest followed by the BLAKE3-addressed blobs it lists.
//!
//! Wire layout (all integers little-endian):
//!
//! ```text
//! "CIVBBPKG" | container version u32 | manifest length u32 | manifest (CBOR) | Ed25519 signature (64)
//! | blob bytes, concatenated in manifest order
//! ```
//!
//! The signature covers [`SIGNATURE_DOMAIN`] followed by the manifest bytes. A reader checks the signature
//! before decoding the manifest, bounds every size before allocating, and hashes every blob.

use std::collections::BTreeMap;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::avatar::Avatar;
use crate::blob::BlobId;
use crate::cbor::{self, CborError};

const MAGIC: &[u8; 8] = b"CIVBBPKG";
const CONTAINER_VERSION: u32 = 1;
/// Prefix of the signed message, so a manifest signature can't be replayed as any other signature.
pub const SIGNATURE_DOMAIN: &[u8] = b"civbb-manifest-v1\0";

/// What a package contains.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentKind {
    Avatar,
    World,
}

/// Target platform of a compiled variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Platform {
    /// PC VR and flat desktop.
    Desktop,
    /// Standalone Android headsets.
    Quest,
}

/// Everything in a manifest except the blob list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Header {
    /// [`crate::FORMAT_VERSION`] the content was compiled for.
    pub format_version: u32,
    /// Name and version of the compiler that built it, so old content can be recompiled (design doc §9).
    pub compiler: String,
    /// Content kind.
    pub kind: ContentKind,
    /// Platform variant.
    pub platform: Platform,
    /// The blob holding the CBOR-encoded root object (an [`Avatar`] for avatars).
    pub root: BlobId,
}

/// One listed blob.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobEntry {
    /// Content address.
    pub id: BlobId,
    /// Exact length in bytes.
    pub size: u64,
}

/// The signed part of a package, as it appears on the wire.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Package metadata.
    pub header: Header,
    /// Blobs in ascending id order, without duplicates.
    pub blobs: Vec<BlobEntry>,
}

/// Size limits applied while reading an untrusted package.
#[derive(Clone, Copy, Debug)]
pub struct ReadLimits {
    /// Largest accepted manifest.
    pub max_manifest_bytes: usize,
    /// Most blobs in one package.
    pub max_blobs: usize,
    /// Largest single blob.
    pub max_blob_bytes: u64,
    /// Largest total of all blobs.
    pub max_total_bytes: u64,
    /// Largest accepted root object (decoded with these bounds by [`Package::avatar`]).
    pub max_root_bytes: usize,
}

impl Default for ReadLimits {
    fn default() -> Self {
        Self {
            max_manifest_bytes: 1 << 20,
            max_blobs: 4096,
            max_blob_bytes: 256 << 20,
            max_total_bytes: 1 << 30,
            max_root_bytes: 256 << 20,
        }
    }
}

/// Why a package was rejected.
#[allow(missing_docs)]
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("not a CIVBB package")]
    BadMagic,
    #[error("unsupported container version {0}")]
    UnsupportedVersion(u32),
    #[error("truncated package")]
    Truncated,
    #[error("trailing bytes after the last blob")]
    TrailingBytes,
    #[error("limit exceeded: {0}")]
    TooLarge(&'static str),
    #[error("manifest signature invalid")]
    BadSignature,
    #[error("manifest: {0}")]
    Manifest(#[from] CborError),
    #[error("blob list not sorted and unique")]
    UnsortedBlobs,
    #[error("blob {0} does not match its hash")]
    BlobMismatch(BlobId),
    #[error("root blob {0} not listed")]
    MissingRoot(BlobId),
}

/// A package in memory: header plus blobs by id.
#[derive(Clone, Debug, PartialEq)]
pub struct Package {
    /// Metadata.
    pub header: Header,
    /// All blobs, keyed by their hash.
    pub blobs: BTreeMap<BlobId, Vec<u8>>,
}

impl Package {
    /// Adds a blob and returns its id. Adding identical bytes twice stores them once.
    pub fn add_blob(&mut self, bytes: Vec<u8>) -> BlobId {
        let id = BlobId::of(&bytes);
        self.blobs.entry(id).or_insert(bytes);
        id
    }

    /// The bytes of a blob, if present.
    pub fn blob(&self, id: BlobId) -> Option<&[u8]> {
        self.blobs.get(&id).map(Vec::as_slice)
    }

    /// Builds an avatar package: stores `avatar` as the root blob. Texture blobs must already be added.
    pub fn with_avatar(mut self, avatar: &Avatar) -> Self {
        self.header.kind = ContentKind::Avatar;
        self.header.root = self.add_blob(cbor::to_vec(avatar));
        self
    }

    /// Decodes the root blob as an [`Avatar`].
    pub fn avatar(&self, limits: &ReadLimits) -> Result<Avatar, PackageError> {
        let root = self
            .blob(self.header.root)
            .ok_or(PackageError::MissingRoot(self.header.root))?;
        Ok(cbor::from_slice(root, limits.max_root_bytes)?)
    }

    /// The manifest this package would be written with.
    pub fn manifest(&self) -> Manifest {
        Manifest {
            header: self.header.clone(),
            blobs: self
                .blobs
                .iter()
                .map(|(id, b)| BlobEntry {
                    id: *id,
                    size: b.len() as u64,
                })
                .collect(),
        }
    }

    /// Serialises and signs the package.
    pub fn encode_signed(&self, key: &SigningKey) -> Vec<u8> {
        let manifest = cbor::to_vec(&self.manifest());
        let signature = key.sign(&signed_message(&manifest));
        let mut out = Vec::with_capacity(80 + manifest.len() + self.blobs.values().map(Vec::len).sum::<usize>());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&CONTAINER_VERSION.to_le_bytes());
        out.extend_from_slice(
            &u32::try_from(manifest.len())
                .expect("manifest under 4 GiB")
                .to_le_bytes(),
        );
        out.extend_from_slice(&manifest);
        out.extend_from_slice(&signature.to_bytes());
        for blob in self.blobs.values() {
            out.extend_from_slice(blob);
        }
        out
    }

    /// Parses and verifies an untrusted package against `key`. Returns only fully verified content.
    pub fn decode_verified(bytes: &[u8], key: &VerifyingKey, limits: &ReadLimits) -> Result<Self, PackageError> {
        let mut r = Reader(bytes);
        if r.take(8)? != MAGIC {
            return Err(PackageError::BadMagic);
        }
        let version = r.u32()?;
        if version != CONTAINER_VERSION {
            return Err(PackageError::UnsupportedVersion(version));
        }
        let manifest_len = r.u32()? as usize;
        if manifest_len > limits.max_manifest_bytes {
            return Err(PackageError::TooLarge("manifest"));
        }
        let manifest_bytes = r.take(manifest_len)?;
        let signature = Signature::from_bytes(r.take(64)?.try_into().expect("took 64 bytes"));
        key.verify(&signed_message(manifest_bytes), &signature)
            .map_err(|_| PackageError::BadSignature)?;

        let manifest: Manifest = cbor::from_slice(manifest_bytes, limits.max_manifest_bytes)?;
        if manifest.blobs.len() > limits.max_blobs {
            return Err(PackageError::TooLarge("blob count"));
        }
        if !manifest.blobs.windows(2).all(|w| w[0].id < w[1].id) {
            return Err(PackageError::UnsortedBlobs);
        }
        let mut total = 0u64;
        for entry in &manifest.blobs {
            if entry.size > limits.max_blob_bytes {
                return Err(PackageError::TooLarge("blob"));
            }
            total += entry.size;
        }
        if total > limits.max_total_bytes {
            return Err(PackageError::TooLarge("total blob size"));
        }
        if !manifest.blobs.iter().any(|e| e.id == manifest.header.root) {
            return Err(PackageError::MissingRoot(manifest.header.root));
        }

        let mut blobs = BTreeMap::new();
        for entry in &manifest.blobs {
            let data = r.take(usize::try_from(entry.size).map_err(|_| PackageError::TooLarge("blob"))?)?;
            if BlobId::of(data) != entry.id {
                return Err(PackageError::BlobMismatch(entry.id));
            }
            blobs.insert(entry.id, data.to_vec());
        }
        if !r.0.is_empty() {
            return Err(PackageError::TrailingBytes);
        }
        Ok(Self {
            header: manifest.header,
            blobs,
        })
    }
}

fn signed_message(manifest: &[u8]) -> Vec<u8> {
    [SIGNATURE_DOMAIN, manifest].concat()
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], PackageError> {
        if n > self.0.len() {
            return Err(PackageError::Truncated);
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Ok(head)
    }

    fn u32(&mut self) -> Result<u32, PackageError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("took 4 bytes")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::sample_avatar;

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7; 32])
    }

    fn sample() -> (Package, Vec<u8>) {
        let mut pkg = Package {
            header: Header {
                format_version: crate::FORMAT_VERSION,
                compiler: "test".into(),
                kind: ContentKind::Avatar,
                platform: Platform::Desktop,
                root: BlobId([0; 32]),
            },
            blobs: BTreeMap::new(),
        };
        let mut avatar = sample_avatar();
        avatar.textures[0].blob = pkg.add_blob(b"not really a png".to_vec());
        let pkg = pkg.with_avatar(&avatar);
        let bytes = pkg.encode_signed(&key());
        (pkg, bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Package, PackageError> {
        Package::decode_verified(bytes, &key().verifying_key(), &ReadLimits::default())
    }

    #[test]
    fn round_trip() {
        let (pkg, bytes) = sample();
        let back = decode(&bytes).unwrap();
        assert_eq!(back, pkg);
        assert_eq!(
            back.avatar(&ReadLimits::default()).unwrap().textures[0].blob,
            pkg.avatar(&ReadLimits::default()).unwrap().textures[0].blob
        );
    }

    #[test]
    fn identical_blobs_are_stored_once() {
        let (mut pkg, _) = sample();
        let n = pkg.blobs.len();
        let a = pkg.add_blob(b"x".to_vec());
        let b = pkg.add_blob(b"x".to_vec());
        assert_eq!(a, b);
        assert_eq!(pkg.blobs.len(), n + 1);
    }

    #[test]
    fn any_flipped_byte_is_rejected() {
        let (_, bytes) = sample();
        // Every region: magic, version, length, manifest, signature, blobs.
        for i in 0..bytes.len() {
            let mut t = bytes.clone();
            t[i] ^= 0x01;
            assert!(decode(&t).is_err(), "flip at byte {i} of {} accepted", bytes.len());
        }
    }

    #[test]
    fn wrong_key_is_rejected() {
        let (_, bytes) = sample();
        let other = SigningKey::from_bytes(&[8; 32]).verifying_key();
        let err = Package::decode_verified(&bytes, &other, &ReadLimits::default()).unwrap_err();
        assert!(matches!(err, PackageError::BadSignature));
    }

    #[test]
    fn truncation_and_trailing_bytes_are_rejected() {
        let (_, bytes) = sample();
        for len in 0..bytes.len() {
            assert!(decode(&bytes[..len]).is_err(), "truncated to {len} accepted");
        }
        let mut long = bytes.clone();
        long.push(0);
        assert!(matches!(decode(&long), Err(PackageError::TrailingBytes)));
    }

    #[test]
    fn limits_are_enforced() {
        let (_, bytes) = sample();
        let tight = ReadLimits {
            max_blob_bytes: 4,
            ..ReadLimits::default()
        };
        let err = Package::decode_verified(&bytes, &key().verifying_key(), &tight).unwrap_err();
        assert!(matches!(err, PackageError::TooLarge("blob")));
    }
}
