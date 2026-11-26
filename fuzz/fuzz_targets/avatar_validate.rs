//! Fuzzes bounded CBOR decoding of a document followed by validation.
#![no_main]

use civbb_format::{Caps, Document, cbor, validate};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(a) = cbor::from_slice::<Document>(data, 16 << 20) {
        let _ = validate(&a, &Caps::default(), |_| true);
    }
});
