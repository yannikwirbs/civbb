//! Fuzzes the compiler stages on whatever the glTF importer accepts: prune, atlas, merge, lod and collapse must not
//! panic, and must leave a valid avatar (texture compression is Basis Universal on decoded images; not here).
#![no_main]

use civbb_format::{Caps, validate};
use civbb_pipeline::{Limits, atlas, collapse, import, lod, merge, prune};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(mut im) = import(data, &Limits::default(), |_| None) else {
        return;
    };
    prune(&mut im.avatar);
    if atlas(&mut im, 4096).is_err() {
        return;
    }
    merge(&mut im.avatar);
    lod(&mut im.avatar);
    collapse(&mut im.avatar);
    let blobs = &im.blobs;
    if let Err(errors) = validate(&im.avatar, &Caps::default(), |id| blobs.contains_key(&id)) {
        panic!("the stages made an invalid avatar: {errors:?}");
    }
});
