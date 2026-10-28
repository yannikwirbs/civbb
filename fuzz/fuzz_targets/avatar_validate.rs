//! Fuzzes bounded CBOR decoding of an avatar followed by validation.
#![no_main]

use civbb_format::{Avatar, Caps, cbor, validate};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(a) = cbor::from_slice::<Avatar>(data, 16 << 20) {
        let _ = validate(&a, &Caps::default(), |_| true);
    }
});
