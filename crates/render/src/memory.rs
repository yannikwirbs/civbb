//! GPU buffers (each with its device address) and images allocated with `gpu-allocator`, plus one-shot submits and
//! uploads.

use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme};

pub use gpu_allocator::MemoryLocation;

use crate::{Context, Error, Result};

/// A buffer, its memory and its device address (shaders reach buffers by address, ADR 0022).
pub struct Buffer {
    /// The buffer.
    pub handle: vk::Buffer,
    /// Its device address.
    pub address: vk::DeviceAddress,
    /// Its size in bytes.
    pub size: u64,
    allocation: Allocation,
}

impl Buffer {
    /// The buffer's bytes, if its memory is host-visible.
    pub fn mapped(&self) -> Option<&[u8]> {
        self.allocation.mapped_slice()
    }

    /// The buffer's bytes for writing, if its memory is host-visible.
    pub fn mapped_mut(&mut self) -> Option<&mut [u8]> {
        self.allocation.mapped_slice_mut()
    }
}

/// An image, its memory and a view of all its mips and layers.
pub struct Image {
    /// The image.
    pub handle: vk::Image,
    /// The view.
    pub view: vk::ImageView,
    allocation: Allocation,
}

impl Context {
    /// Creates an image in device memory and a 2D view of `aspect` over all its mips and layers.
    pub fn create_image(&self, name: &str, info: &vk::ImageCreateInfo, aspect: vk::ImageAspectFlags) -> Result<Image> {
        let d = self.device();
        // SAFETY: valid create infos; on failure what was created is destroyed before returning.
        unsafe {
            let handle = d.create_image(info, None)?;
            let desc = AllocationCreateDesc {
                name,
                requirements: d.get_image_memory_requirements(handle),
                location: MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            };
            let allocation = match self.allocator.lock().expect("allocator lock").allocate(&desc) {
                Ok(a) => a,
                Err(e) => {
                    d.destroy_image(handle, None);
                    return Err(Error::Memory(e.to_string()));
                }
            };
            let view = d
                .bind_image_memory(handle, allocation.memory(), allocation.offset())
                .and_then(|()| {
                    let range = vk::ImageSubresourceRange {
                        aspect_mask: aspect,
                        base_mip_level: 0,
                        level_count: vk::REMAINING_MIP_LEVELS,
                        base_array_layer: 0,
                        layer_count: vk::REMAINING_ARRAY_LAYERS,
                    };
                    let view = vk::ImageViewCreateInfo::default()
                        .image(handle)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(info.format)
                        .subresource_range(range);
                    d.create_image_view(&view, None)
                });
            match view {
                Ok(view) => Ok(Image {
                    handle,
                    view,
                    allocation,
                }),
                Err(e) => {
                    d.destroy_image(handle, None);
                    let _ = self.allocator.lock().expect("allocator lock").free(allocation);
                    Err(e.into())
                }
            }
        }
    }

    /// Destroys an image, its view and its memory.
    ///
    /// # Safety
    ///
    /// The GPU no longer uses the image.
    pub unsafe fn destroy_image(&self, image: Image) {
        // SAFETY: per the contract.
        unsafe {
            self.device().destroy_image_view(image.view, None);
            self.device().destroy_image(image.handle, None);
        }
        let _ = self.allocator.lock().expect("allocator lock").free(image.allocation);
    }

    /// Creates a buffer of `size` bytes (at least 4) with `usage` plus device addressing, in `location`.
    pub fn create_buffer(
        &self,
        name: &str,
        size: u64,
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
    ) -> Result<Buffer> {
        let d = self.device();
        let size = size.max(4);
        let info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS);
        // SAFETY: valid create info; on failure the buffer is destroyed before returning.
        unsafe {
            let handle = d.create_buffer(&info, None)?;
            let desc = AllocationCreateDesc {
                name,
                requirements: d.get_buffer_memory_requirements(handle),
                location,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            };
            let allocation = self.allocator.lock().expect("allocator lock").allocate(&desc);
            let allocation = match allocation {
                Ok(a) => a,
                Err(e) => {
                    d.destroy_buffer(handle, None);
                    return Err(Error::Memory(e.to_string()));
                }
            };
            if let Err(e) = d.bind_buffer_memory(handle, allocation.memory(), allocation.offset()) {
                d.destroy_buffer(handle, None);
                let _ = self.allocator.lock().expect("allocator lock").free(allocation);
                return Err(e.into());
            }
            let address = d.get_buffer_device_address(&vk::BufferDeviceAddressInfo::default().buffer(handle));
            Ok(Buffer {
                handle,
                address,
                size,
                allocation,
            })
        }
    }

    /// Destroys a buffer and frees its memory.
    ///
    /// # Safety
    ///
    /// The GPU no longer uses the buffer.
    pub unsafe fn destroy_buffer(&self, buffer: Buffer) {
        // SAFETY: per the contract.
        unsafe { self.device().destroy_buffer(buffer.handle, None) };
        let _ = self.allocator.lock().expect("allocator lock").free(buffer.allocation);
    }

    /// Records commands into a fresh command buffer, submits it and waits for it to finish.
    pub fn one_shot(&self, record: impl FnOnce(&ash::Device, vk::CommandBuffer)) -> Result<()> {
        let d = self.device();
        // SAFETY: objects are created, used and destroyed here; the fence wait ends all GPU use before destruction.
        unsafe {
            let pool = d.create_command_pool(
                &vk::CommandPoolCreateInfo::default().queue_family_index(self.queue_family()),
                None,
            )?;
            let result = (|| -> Result<()> {
                let info = vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .command_buffer_count(1);
                let cmd = d.allocate_command_buffers(&info)?[0];
                d.begin_command_buffer(
                    cmd,
                    &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )?;
                record(d, cmd);
                d.end_command_buffer(cmd)?;
                let fence = d.create_fence(&vk::FenceCreateInfo::default(), None)?;
                let cmds = [vk::CommandBufferSubmitInfo::default().command_buffer(cmd)];
                let submitted = d
                    .queue_submit2(
                        self.queue(),
                        &[vk::SubmitInfo2::default().command_buffer_infos(&cmds)],
                        fence,
                    )
                    .and_then(|()| d.wait_for_fences(&[fence], true, u64::MAX));
                d.destroy_fence(fence, None);
                Ok(submitted?)
            })();
            d.destroy_command_pool(pool, None);
            result
        }
    }

    /// Copies `data` to `buffer` at byte `offset` through a staging buffer, made visible to every later command.
    pub fn upload(&self, buffer: &Buffer, offset: u64, data: &[u8]) -> Result<()> {
        let size = data.len() as u64;
        assert!(
            offset + size <= buffer.size,
            "upload of {size} bytes at {offset} into a buffer of {}",
            buffer.size
        );
        if data.is_empty() {
            return Ok(());
        }
        let mut staging = self.create_buffer(
            "staging",
            size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
        )?;
        staging.mapped_mut().expect("CpuToGpu memory is mapped")[..data.len()].copy_from_slice(data);
        let result = self.one_shot(|d, cmd| {
            let region = vk::BufferCopy {
                src_offset: 0,
                dst_offset: offset,
                size,
            };
            let visible = vk::MemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COPY)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .dst_access_mask(vk::AccessFlags2::MEMORY_READ);
            // SAFETY: both buffers are alive and large enough.
            unsafe {
                d.cmd_copy_buffer(cmd, staging.handle, buffer.handle, &[region]);
                d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().memory_barriers(&[visible]));
            }
        });
        // SAFETY: `one_shot` waited for the copy, or never submitted it.
        unsafe { self.destroy_buffer(staging) };
        result
    }
}

/// `vkCmdUpdateBuffer`'s limit per call.
const UPDATE_LIMIT: usize = 65_536;

/// Writes `data` to `buffer` at byte `offset` from the command buffer, in chunks `vkCmdUpdateBuffer` accepts.
///
/// # Safety
///
/// `cmd` is recording outside a rendering scope; `buffer` holds `offset + data.len()` bytes; `offset` and the
/// length are multiples of 4.
pub(crate) unsafe fn update(device: &ash::Device, cmd: vk::CommandBuffer, buffer: &Buffer, offset: u64, data: &[u8]) {
    for (i, chunk) in data.chunks(UPDATE_LIMIT).enumerate() {
        // SAFETY: per the contract; chunks are whole words, at most 65536 bytes long.
        unsafe { device.cmd_update_buffer(cmd, buffer.handle, offset + (i * UPDATE_LIMIT) as u64, chunk) };
    }
}

/// Little-endian bytes of 32-bit words, the layout shaders read.
pub(crate) fn bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}
