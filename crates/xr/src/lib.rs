//! OpenXR (ADR 0122): the instance and head-mounted system, and the runtime's creation of the Vulkan instance and
//! device (XR_KHR_vulkan_enable2) in raw handles.

use std::fmt;
use std::path::PathBuf;

pub use openxr as xr;
use xr::sys::platform::{
    VkDevice, VkDeviceCreateInfo, VkGetInstanceProcAddr, VkInstance, VkInstanceCreateInfo, VkPhysicalDevice,
};

pub mod session;

pub use session::{Frames, Session, Status};

/// The environment variable naming the OpenXR loader library to load instead of the system search's.
pub const LOADER_VAR: &str = "CIVBB_OPENXR_LOADER";

/// Why OpenXR is not available or a call failed.
#[derive(Debug)]
pub enum Error {
    /// The OpenXR loader library is missing or could not be loaded.
    NoLoader(String),
    /// No OpenXR runtime is installed or it failed to start.
    NoRuntime(xr::sys::Result),
    /// The runtime has no headset attached.
    NoHeadset,
    /// The runtime lacks something the client needs.
    Unsupported(String),
    /// An OpenXR call failed.
    Xr(xr::sys::Result),
    /// A Vulkan call the runtime made for us failed (a `VkResult`).
    Vulkan(i32),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoLoader(e) => write!(f, "no OpenXR loader ({e}); set {LOADER_VAR} to openxr_loader's path"),
            Error::NoRuntime(r) => write!(f, "no OpenXR runtime: {r}"),
            Error::NoHeadset => write!(f, "the OpenXR runtime has no headset"),
            Error::Unsupported(what) => write!(f, "the OpenXR runtime lacks {what}"),
            Error::Xr(r) => write!(f, "OpenXR error: {r}"),
            Error::Vulkan(r) => write!(f, "Vulkan error in the OpenXR runtime: {r}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<xr::sys::Result> for Error {
    fn from(r: xr::sys::Result) -> Self {
        Error::Xr(r)
    }
}

/// A result with [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// The view configuration the client renders: one view per eye.
pub const VIEWS: xr::ViewConfigurationType = xr::ViewConfigurationType::PRIMARY_STEREO;

/// An OpenXR instance on a head-mounted system.
pub struct Xr {
    pub(crate) instance: xr::Instance,
    pub(crate) system: xr::SystemId,
    /// Each eye's recommended and largest image sizes.
    pub views: Vec<xr::ViewConfigurationView>,
    /// How the runtime blends our frames with the world: opaque where it can.
    pub blend: xr::EnvironmentBlendMode,
    /// The Vulkan versions the runtime accepts, as `VK_MAKE_API_VERSION` values: the lowest and the highest tested.
    pub vulkan: (u32, u32),
}

impl Xr {
    /// Loads the loader, creates an instance with XR_KHR_vulkan_enable2 and finds the head-mounted system. A
    /// runtime may start itself here (SteamVR does).
    pub fn new() -> Result<Self> {
        let entry = match std::env::var_os(LOADER_VAR) {
            // SAFETY: the named library is an OpenXR loader, by this variable's meaning.
            Some(path) => unsafe { xr::Entry::load_from(&PathBuf::from(path), &()) },
            // SAFETY: the platform's loader name resolves to the Khronos loader or one conforming to it.
            None => unsafe { xr::Entry::load(&()) },
        }
        .map_err(|e| Error::NoLoader(e.to_string()))?;
        let available = entry.enumerate_extensions().map_err(Error::NoRuntime)?;
        if !available.khr_vulkan_enable2 {
            return Err(Error::Unsupported("XR_KHR_vulkan_enable2".into()));
        }
        let mut extensions = xr::ExtensionSet::default();
        extensions.khr_vulkan_enable2 = true;
        let app = xr::ApplicationInfo {
            application_name: "civbb",
            application_version: 0,
            engine_name: "civbb",
            engine_version: 0,
            api_version: xr::Version::new(1, 0, 0),
        };
        let instance = (entry.create_instance(&app, &extensions, &[], &())).map_err(Error::NoRuntime)?;
        let system = instance
            .system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)
            .map_err(|e| match e {
                xr::sys::Result::ERROR_FORM_FACTOR_UNAVAILABLE => Error::NoHeadset,
                e => Error::Xr(e),
            })?;
        let views = instance.enumerate_view_configuration_views(system, VIEWS)?;
        if views.len() != 2 {
            return Err(Error::Unsupported("a stereo view configuration".into()));
        }
        let modes = instance.enumerate_environment_blend_modes(system, VIEWS)?;
        let blend = (modes.iter().copied())
            .find(|&m| m == xr::EnvironmentBlendMode::OPAQUE)
            .or(modes.first().copied())
            .ok_or_else(|| Error::Unsupported("a blend mode".into()))?;
        // Also required before the runtime creates Vulkan objects or a session.
        let req = instance.graphics_requirements::<xr::Vulkan>(system)?;
        let version = |v: xr::Version| (u32::from(v.major()) << 22) | (u32::from(v.minor()) << 12) | v.patch();
        let vulkan = (
            version(req.min_api_version_supported),
            version(req.max_api_version_supported),
        );
        Ok(Self {
            instance,
            system,
            views,
            blend,
            vulkan,
        })
    }

    /// The runtime's name and version.
    pub fn runtime(&self) -> Result<String> {
        let p = self.instance.properties()?;
        Ok(format!("{} {}", p.runtime_name, p.runtime_version))
    }

    /// Has the runtime create the Vulkan instance from `info`, adding the extensions it needs.
    ///
    /// # Safety
    ///
    /// `info` points to a valid `VkInstanceCreateInfo` and `get_instance_proc_addr` is the Vulkan loader's.
    pub unsafe fn create_vulkan_instance(
        &self,
        get_instance_proc_addr: VkGetInstanceProcAddr,
        info: *const VkInstanceCreateInfo,
    ) -> Result<VkInstance> {
        // SAFETY: per this function's contract.
        unsafe {
            self.instance
                .create_vulkan_instance(self.system, get_instance_proc_addr, info)
        }?
        .map_err(Error::Vulkan)
    }

    /// The physical device the headset is attached to.
    ///
    /// # Safety
    ///
    /// `instance` came from [`Xr::create_vulkan_instance`].
    pub unsafe fn vulkan_physical_device(&self, instance: VkInstance) -> Result<VkPhysicalDevice> {
        // SAFETY: per this function's contract.
        Ok(unsafe { self.instance.vulkan_graphics_device(self.system, instance) }?)
    }

    /// Has the runtime create the Vulkan device on `physical` from `info`, adding the extensions it needs.
    ///
    /// # Safety
    ///
    /// `physical` came from [`Xr::vulkan_physical_device`], `info` points to a valid `VkDeviceCreateInfo` and
    /// `get_instance_proc_addr` is the Vulkan loader's.
    pub unsafe fn create_vulkan_device(
        &self,
        get_instance_proc_addr: VkGetInstanceProcAddr,
        physical: VkPhysicalDevice,
        info: *const VkDeviceCreateInfo,
    ) -> Result<VkDevice> {
        // SAFETY: per this function's contract.
        unsafe { (self.instance).create_vulkan_device(self.system, get_instance_proc_addr, physical, info) }?
            .map_err(Error::Vulkan)
    }
}
