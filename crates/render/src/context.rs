//! The Vulkan instance with validation in debug builds, device selection and the logical device with its one queue.

use std::cmp::Reverse;
use std::ffi::{CStr, c_char, c_void};
use std::mem::ManuallyDrop;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use ash::{ext::debug_utils, khr, vk};
use gpu_allocator::vulkan::{Allocator, AllocatorCreateDesc};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

use crate::{Error, Result};

const VALIDATION_LAYER: &CStr = c"VK_LAYER_KHRONOS_validation";

/// The raw handles of a window to present to.
#[derive(Clone, Copy, Debug)]
pub struct Window {
    /// The window system connection.
    pub display: RawDisplayHandle,
    /// The window.
    pub window: RawWindowHandle,
}

/// What device selection needs to know about one physical device.
#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    /// Discrete, integrated, virtual, CPU or other.
    pub kind: vk::PhysicalDeviceType,
    /// The highest Vulkan version the device supports.
    pub api_version: u32,
    /// Whether it has the features the renderer needs (dynamic rendering, synchronization2, buffer device address).
    pub features: bool,
    /// A queue family with graphics and compute (and present, when there is a surface), if any.
    pub queue_family: Option<u32>,
}

/// Picks the device to render on: the first of the most capable kind (discrete, integrated, virtual, CPU) among
/// those with Vulkan 1.3, the needed features and a suitable queue family.
pub fn pick_device(candidates: &[Candidate]) -> Option<usize> {
    let rank = |kind| match kind {
        vk::PhysicalDeviceType::DISCRETE_GPU => 4,
        vk::PhysicalDeviceType::INTEGRATED_GPU => 3,
        vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
        vk::PhysicalDeviceType::CPU => 1,
        _ => 0,
    };
    candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| c.api_version >= vk::API_VERSION_1_3 && c.features && c.queue_family.is_some())
        .max_by_key(|&(i, c)| (rank(c.kind), Reverse(i)))
        .map(|(i, _)| i)
}

/// Instance-level objects, destroyed after the device.
struct Instance {
    instance: ash::Instance,
    debug: Option<(debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    // Boxed so the debug callback's pointer to it stays put.
    errors: Box<AtomicU32>,
    surface: Option<(khr::surface::Instance, vk::SurfaceKHR)>,
    // Last: unloads the library after everything above is destroyed.
    entry: ash::Entry,
}

impl Drop for Instance {
    fn drop(&mut self) {
        // SAFETY: the device using these is gone (`Context::drop` runs first) and each handle is destroyed once.
        unsafe {
            if let Some((loader, surface)) = &self.surface {
                loader.destroy_surface(*surface, None);
            }
            if let Some((loader, messenger)) = &self.debug {
                loader.destroy_debug_utils_messenger(*messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

/// The Vulkan instance, the chosen device and its one graphics and compute queue (ADR 0021).
pub struct Context {
    // Dropped before the device is destroyed.
    pub(crate) allocator: ManuallyDrop<Mutex<Allocator>>,
    device: ash::Device,
    physical: vk::PhysicalDevice,
    queue_family: u32,
    queue: vk::Queue,
    instance: Instance,
}

impl Context {
    /// A context without a window, for offscreen work and tests.
    pub fn headless(validation: bool) -> Result<Self> {
        // SAFETY: no window handles.
        unsafe { Self::new(validation, None) }
    }

    /// Creates the instance, a surface for `window` if given, and a device that can render (and present to it).
    /// With `validation`, enables the Khronos validation layer if it is installed and reports its messages to
    /// stderr; without the layer it warns and carries on.
    ///
    /// # Safety
    ///
    /// `window`'s handles must be valid and the window must outlive the context.
    pub unsafe fn new(validation: bool, window: Option<Window>) -> Result<Self> {
        // SAFETY: loading the system Vulkan library runs its initialisation; nothing else is loaded under that name.
        let entry = unsafe { ash::Entry::load() }.map_err(|e| Error::Load(e.to_string()))?;
        // SAFETY: plain queries and creation with valid create infos; the window's handles are valid per contract.
        let instance = unsafe { create_instance(entry, validation, window)? };
        let surface = instance.surface.as_ref().map(|(l, s)| (l, *s));
        let (physical, queue_family) = select(&instance.instance, surface)?;
        let priorities = [1.0];
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family)
            .queue_priorities(&priorities)];
        let extensions = if surface.is_some() {
            vec![khr::swapchain::NAME.as_ptr()]
        } else {
            vec![]
        };
        let mut features12 = vk::PhysicalDeviceVulkan12Features::default().buffer_device_address(true);
        let mut features13 = vk::PhysicalDeviceVulkan13Features::default()
            .dynamic_rendering(true)
            .synchronization2(true);
        let info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queues)
            .enabled_extension_names(&extensions)
            .push_next(&mut features13)
            .push_next(&mut features12);
        // SAFETY: `physical` came from this instance and supports everything requested (`select`).
        let device = unsafe { instance.instance.create_device(physical, &info, None)? };
        // SAFETY: the queue family was created with one queue.
        let queue = unsafe { device.get_device_queue(queue_family, 0) };
        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: instance.instance.clone(),
            device: device.clone(),
            physical_device: physical,
            debug_settings: Default::default(),
            buffer_device_address: true,
            allocation_sizes: Default::default(),
        });
        let allocator = match allocator {
            Ok(a) => a,
            Err(e) => {
                // SAFETY: nothing was created on the device yet.
                unsafe { device.destroy_device(None) };
                return Err(Error::Memory(e.to_string()));
            }
        };
        Ok(Context {
            allocator: ManuallyDrop::new(Mutex::new(allocator)),
            device,
            physical,
            queue_family,
            queue,
            instance,
        })
    }

    /// The instance.
    pub fn instance(&self) -> &ash::Instance {
        &self.instance.instance
    }

    /// The physical device rendering happens on.
    pub fn physical(&self) -> vk::PhysicalDevice {
        self.physical
    }

    /// The logical device.
    pub fn device(&self) -> &ash::Device {
        &self.device
    }

    /// The queue for graphics, compute and present.
    pub fn queue(&self) -> vk::Queue {
        self.queue
    }

    /// The queue's family.
    pub fn queue_family(&self) -> u32 {
        self.queue_family
    }

    /// The window's surface, if the context was created with one.
    pub fn surface(&self) -> Option<(&khr::surface::Instance, vk::SurfaceKHR)> {
        self.instance.surface.as_ref().map(|(l, s)| (l, *s))
    }

    /// How many error messages validation has reported so far (always 0 without validation).
    pub fn validation_errors(&self) -> u32 {
        self.instance.errors.load(Ordering::Relaxed)
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: owners of device objects (the renderer) are dropped before the context; the allocator is dropped
        // once, here, before the device it allocates from.
        unsafe {
            let _ = self.device.device_wait_idle();
            ManuallyDrop::drop(&mut self.allocator);
            self.device.destroy_device(None);
        }
    }
}

/// # Safety
///
/// `window`'s handles must be valid.
unsafe fn create_instance(entry: ash::Entry, validation: bool, window: Option<Window>) -> Result<Instance> {
    // SAFETY: plain query.
    let layers = unsafe { entry.enumerate_instance_layer_properties()? };
    let validation = validation && {
        let found = layers.iter().any(|l| l.layer_name_as_c_str() == Ok(VALIDATION_LAYER));
        if !found {
            eprintln!("civbb-render: validation requested but {VALIDATION_LAYER:?} is not installed; running without");
        }
        found
    };
    let mut extensions: Vec<*const c_char> = match window {
        Some(w) => ash_window::enumerate_required_extensions(w.display)?.to_vec(),
        None => vec![],
    };
    let mut layer_names = vec![];
    if validation {
        layer_names.push(VALIDATION_LAYER.as_ptr());
        extensions.push(debug_utils::NAME.as_ptr());
    }
    let app = vk::ApplicationInfo::default()
        .application_name(c"civbb")
        .engine_name(c"civbb")
        .api_version(vk::API_VERSION_1_3);
    let info = vk::InstanceCreateInfo::default()
        .application_info(&app)
        .enabled_layer_names(&layer_names)
        .enabled_extension_names(&extensions);
    // SAFETY: the create info and the names it points to are alive for the call.
    let instance = unsafe { entry.create_instance(&info, None) }.map_err(|e| match e {
        // A loader without any driver (CI runners).
        vk::Result::ERROR_INCOMPATIBLE_DRIVER => Error::NoDevice,
        e => e.into(),
    })?;
    let mut instance = Instance {
        instance,
        debug: None,
        errors: Box::default(),
        surface: None,
        entry,
    };
    if validation {
        let loader = debug_utils::Instance::new(&instance.entry, &instance.instance);
        let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::WARNING | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(on_message))
            .user_data(&*instance.errors as *const AtomicU32 as *mut c_void);
        // SAFETY: the user data outlives the messenger (both live in `Instance`, the messenger is destroyed first).
        let messenger = unsafe { loader.create_debug_utils_messenger(&info, None)? };
        instance.debug = Some((loader, messenger));
    }
    if let Some(w) = window {
        // SAFETY: the handles are valid per this function's contract.
        let surface =
            unsafe { ash_window::create_surface(&instance.entry, &instance.instance, w.display, w.window, None)? };
        instance.surface = Some((
            khr::surface::Instance::new(&instance.entry, &instance.instance),
            surface,
        ));
    }
    Ok(instance)
}

/// Gathers a [`Candidate`] per physical device and picks one.
fn select(
    instance: &ash::Instance,
    surface: Option<(&khr::surface::Instance, vk::SurfaceKHR)>,
) -> Result<(vk::PhysicalDevice, u32)> {
    // SAFETY: plain queries on handles from this instance.
    unsafe {
        let devices = instance.enumerate_physical_devices()?;
        let mut candidates = Vec::with_capacity(devices.len());
        for &pd in &devices {
            let props = instance.get_physical_device_properties(pd);
            let mut f12 = vk::PhysicalDeviceVulkan12Features::default();
            let mut f13 = vk::PhysicalDeviceVulkan13Features::default();
            let mut f2 = vk::PhysicalDeviceFeatures2::default()
                .push_next(&mut f13)
                .push_next(&mut f12);
            if props.api_version >= vk::API_VERSION_1_3 {
                instance.get_physical_device_features2(pd, &mut f2);
            }
            let features = f13.dynamic_rendering == vk::TRUE
                && f13.synchronization2 == vk::TRUE
                && f12.buffer_device_address == vk::TRUE;
            let families = instance.get_physical_device_queue_family_properties(pd);
            let mut queue_family = None;
            for (i, f) in families.iter().enumerate() {
                let i = i as u32;
                let present = match surface {
                    Some((loader, s)) => loader.get_physical_device_surface_support(pd, i, s)?,
                    None => true,
                };
                if f.queue_flags
                    .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
                    && present
                {
                    queue_family = Some(i);
                    break;
                }
            }
            candidates.push(Candidate {
                kind: props.device_type,
                api_version: props.api_version,
                features,
                queue_family,
            });
        }
        let i = pick_device(&candidates).ok_or(Error::NoDevice)?;
        Ok((
            devices[i],
            candidates[i].queue_family.expect("picked devices have a queue family"),
        ))
    }
}

unsafe extern "system" fn on_message(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _kind: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    errors: *mut c_void,
) -> vk::Bool32 {
    // SAFETY: the layer passes valid callback data; `errors` is the `AtomicU32` registered with the messenger.
    unsafe {
        let message = (*data).message_as_c_str().unwrap_or(c"").to_string_lossy();
        if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
            (*(errors as *const AtomicU32)).fetch_add(1, Ordering::Relaxed);
        }
        eprintln!("civbb-render: [{severity:?}] {message}");
    }
    vk::FALSE
}
