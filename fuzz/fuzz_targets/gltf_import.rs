//! Fuzzes the glTF/VRM importer on `.gltf` and `.glb` bytes with no external files.
#![no_main]

use civbb_pipeline::{Limits, import};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = import(data, &Limits::default(), |_| None);
});
