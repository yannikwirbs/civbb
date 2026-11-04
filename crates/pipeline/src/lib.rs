//! Content compiler: imports glTF/VRM sources into the format model and builds runtime packages (design doc §9).

mod accessor;
pub mod atlas;
pub mod collapse;
pub mod gltf_import;
pub mod lod;
pub mod merge;
pub mod package;
pub mod prune;
pub mod report;
pub mod texture;

pub use atlas::{Atlased, atlas};
pub use collapse::collapse;
pub use gltf_import::{ImportError, Imported, Limits, import};
pub use lod::{Lods, lod};
pub use merge::{Merged, merge};
pub use package::{COMPILER, Level, Stats, package, stats};
pub use prune::{Pruned, prune};
pub use report::{Report, compile};
pub use texture::{Compressed, compress};
