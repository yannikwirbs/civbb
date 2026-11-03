//! GPU buffers allocated with `gpu-allocator`, each with its device address, plus one-shot submits and uploads.

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

impl Context {
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

    /// Copies `data` to the start of `buffer` through a staging buffer, made visible to every later command.
    pub fn upload(&self, buffer: &Buffer, data: &[u8]) -> Result<()> {
        let size = data.len() as u64;
        assert!(
            size <= buffer.size,
            "upload of {size} bytes into a buffer of {}",
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
                dst_offset: 0,
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

/// Little-endian bytes of 32-bit words, the layout shaders read.
pub(crate) fn bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}
