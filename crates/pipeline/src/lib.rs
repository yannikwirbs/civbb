//! Content compiler: imports glTF/VRM sources into the format model and builds runtime packages (design doc §9).

mod accessor;
pub mod gltf_import;

pub use gltf_import::{ImportError, Imported, Limits, import};
