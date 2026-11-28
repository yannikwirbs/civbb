//! On a real OpenXR runtime (ADR 0122): an instance with XR_KHR_vulkan_enable2, the head-mounted system, its two
//! views and the Vulkan versions it accepts. Skips without a loader (the gate's case unless `CIVBB_OPENXR_LOADER`
//! is set), without a runtime and without a headset.

use civbb_xr::{Error, Xr};

#[test]
fn reads_the_system_and_its_views() {
    let xr = match Xr::new() {
        Ok(xr) => xr,
        Err(e @ (Error::NoLoader(_) | Error::NoRuntime(_) | Error::NoHeadset)) => {
            eprintln!("skipped: {e}");
            return;
        }
        Err(e) => panic!("{e}"),
    };
    eprintln!("runtime: {}", xr.runtime().unwrap());
    assert_eq!(xr.views.len(), 2);
    for v in &xr.views {
        eprintln!(
            "view: {} x {}",
            v.recommended_image_rect_width, v.recommended_image_rect_height
        );
        assert!(v.recommended_image_rect_width > 0 && v.recommended_image_rect_height > 0);
        assert!(v.recommended_image_rect_width <= v.max_image_rect_width);
    }
    let (min, max) = xr.vulkan;
    assert!(min <= max, "{min:#x} > {max:#x}");
}
