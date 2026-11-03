//! The window's swapchain: format and size choice, its images and views, and one "rendered" semaphore per image.

use ash::{khr, vk};

use crate::graph::COLOR;
use crate::{Context, Result};

/// Picks the surface format: 8-bit BGRA or RGBA sRGB if offered, else the first.
pub fn choose_format(formats: &[vk::SurfaceFormatKHR]) -> vk::SurfaceFormatKHR {
    formats
        .iter()
        .copied()
        .find(|f| {
            matches!(f.format, vk::Format::B8G8R8A8_SRGB | vk::Format::R8G8B8A8_SRGB)
                && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .or(formats.first().copied())
        .unwrap_or_default()
}

pub(crate) struct Swapchain {
    pub(crate) loader: khr::swapchain::Device,
    pub(crate) handle: vk::SwapchainKHR,
    pub(crate) extent: vk::Extent2D,
    pub(crate) images: Vec<vk::Image>,
    pub(crate) views: Vec<vk::ImageView>,
    /// Signalled when rendering to the image is done, waited on by its present. Per image, not per frame: a
    /// present holds its semaphore until the image is acquired again.
    pub(crate) rendered: Vec<vk::Semaphore>,
}

impl Swapchain {
    /// Creates a swapchain of about `extent` (the surface decides), replacing `old` if not null.
    ///
    /// # Safety
    ///
    /// `ctx` has a surface; `old` is null or a swapchain of it no longer in use.
    pub(crate) unsafe fn new(ctx: &Context, extent: vk::Extent2D, old: vk::SwapchainKHR) -> Result<Self> {
        let (surface_fn, surface) = ctx.surface().expect("renderer contexts have a surface");
        // SAFETY: handles from `ctx`; create infos are alive for each call.
        unsafe {
            let caps = surface_fn.get_physical_device_surface_capabilities(ctx.physical(), surface)?;
            let format = choose_format(&surface_fn.get_physical_device_surface_formats(ctx.physical(), surface)?);
            let extent = if caps.current_extent.width != u32::MAX {
                caps.current_extent
            } else {
                vk::Extent2D {
                    width: extent
                        .width
                        .clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                    height: extent
                        .height
                        .clamp(caps.min_image_extent.height, caps.max_image_extent.height),
                }
            };
            let mut count = caps.min_image_count + 1;
            if caps.max_image_count > 0 {
                count = count.min(caps.max_image_count);
            }
            let info = vk::SwapchainCreateInfoKHR::default()
                .surface(surface)
                .min_image_count(count)
                .image_format(format.format)
                .image_color_space(format.color_space)
                .image_extent(extent)
                .image_array_layers(1)
                .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST)
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .pre_transform(caps.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                // Always available; vsync (ADR 0021).
                .present_mode(vk::PresentModeKHR::FIFO)
                .clipped(true)
                .old_swapchain(old);
            let loader = khr::swapchain::Device::new(ctx.instance(), ctx.device());
            let handle = loader.create_swapchain(&info, None)?;
            let mut sc = Swapchain {
                loader,
                handle,
                extent,
                images: vec![],
                views: vec![],
                rendered: vec![],
            };
            // On failure below, `destroy` cleans up what exists so far.
            let result = (|| -> Result<()> {
                sc.images = sc.loader.get_swapchain_images(handle)?;
                for &image in &sc.images {
                    let view = vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(format.format)
                        .subresource_range(COLOR);
                    sc.views.push(ctx.device().create_image_view(&view, None)?);
                    sc.rendered.push(
                        ctx.device()
                            .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?,
                    );
                }
                Ok(())
            })();
            match result {
                Ok(()) => Ok(sc),
                Err(e) => {
                    sc.destroy(ctx.device());
                    Err(e)
                }
            }
        }
    }

    /// # Safety
    ///
    /// The device is idle with respect to this swapchain.
    pub(crate) unsafe fn destroy(&mut self, device: &ash::Device) {
        // SAFETY: per the contract; each handle is destroyed once and the lists are emptied.
        unsafe {
            for s in self.rendered.drain(..) {
                device.destroy_semaphore(s, None);
            }
            for v in self.views.drain(..) {
                device.destroy_image_view(v, None);
            }
            self.loader.destroy_swapchain(self.handle, None);
        }
        self.handle = vk::SwapchainKHR::null();
    }
}
