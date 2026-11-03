//! Device and surface format choice (ADR 0021), no device needed.

use civbb_render::{Candidate, choose_format, pick_device, vk};

fn gpu(kind: vk::PhysicalDeviceType) -> Candidate {
    Candidate {
        kind,
        api_version: vk::API_VERSION_1_3,
        features: true,
        queue_family: Some(0),
    }
}

#[test]
fn prefers_discrete_then_integrated_then_cpu() {
    use vk::PhysicalDeviceType as T;
    assert_eq!(
        pick_device(&[gpu(T::CPU), gpu(T::INTEGRATED_GPU), gpu(T::DISCRETE_GPU)]),
        Some(2)
    );
    assert_eq!(pick_device(&[gpu(T::CPU), gpu(T::INTEGRATED_GPU)]), Some(1));
    assert_eq!(pick_device(&[gpu(T::CPU)]), Some(0));
    // Ties go to the first.
    assert_eq!(pick_device(&[gpu(T::DISCRETE_GPU), gpu(T::DISCRETE_GPU)]), Some(0));
}

#[test]
fn rejects_devices_that_cannot_run_the_renderer() {
    let d = gpu(vk::PhysicalDeviceType::DISCRETE_GPU);
    let old = Candidate {
        api_version: vk::API_VERSION_1_2,
        ..d
    };
    let no_features = Candidate { features: false, ..d };
    let no_queue = Candidate {
        queue_family: None,
        ..d
    };
    assert_eq!(pick_device(&[old, no_features, no_queue]), None);
    assert_eq!(pick_device(&[old, gpu(vk::PhysicalDeviceType::CPU)]), Some(1));
    assert_eq!(pick_device(&[]), None);
}

#[test]
fn surface_format_prefers_srgb() {
    let f = |format| vk::SurfaceFormatKHR {
        format,
        color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
    };
    let unorm = f(vk::Format::B8G8R8A8_UNORM);
    assert_eq!(
        choose_format(&[unorm, f(vk::Format::B8G8R8A8_SRGB)]),
        f(vk::Format::B8G8R8A8_SRGB)
    );
    assert_eq!(
        choose_format(&[unorm, f(vk::Format::R8G8B8A8_SRGB)]),
        f(vk::Format::R8G8B8A8_SRGB)
    );
    assert_eq!(choose_format(&[unorm]), unorm);
}
