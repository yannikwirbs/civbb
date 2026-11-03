//! Fuzzes the client's KTX2 to BC7 transcoding of texture blobs (ADR 0029).
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = civbb_client::view::transcode_ktx2(data);
});
