//! Fuzzes package decoding past the signature check: the harness re-signs whatever manifest the input holds.
#![no_main]

use civbb_format::package::SIGNATURE_DOMAIN;
use civbb_format::{Package, ReadLimits, SigningKey};
use ed25519_dalek::Signer;
use libfuzzer_sys::fuzz_target;

/// The key `seeds` signs with.
const KEY: [u8; 32] = [7; 32];

fuzz_target!(|data: &[u8]| {
    let key = SigningKey::from_bytes(&KEY);
    let mut data = data.to_vec();
    // Layout: magic (8), container version (4), manifest length (4), manifest, signature (64), blobs.
    if let Some(len) = data
        .get(12..16)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
    {
        let end = 16usize.saturating_add(len);
        if data.len() >= end.saturating_add(64) {
            let signature = key.sign(&[SIGNATURE_DOMAIN, &data[16..end]].concat());
            data[end..end + 64].copy_from_slice(&signature.to_bytes());
        }
    }
    let limits = ReadLimits::default();
    if let Ok(p) = Package::decode_verified(&data, &key.verifying_key(), &limits) {
        let _ = p.avatar(&limits);
    }
});
