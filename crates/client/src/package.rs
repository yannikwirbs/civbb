//! Packages as shared handles: a verified avatar, its compiled model and decoded textures, loaded on the I/O pool
//! and shared by every island showing it (design doc §5 "Streaming / IO", ADR 0104).

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bevy_tasks::{IoTaskPool, Task, TaskPoolBuilder};
use civbb_format::{Caps, Document, ReadLimits, SigningKey, validate};

use crate::island::Model;
use crate::view::{TextureImage, decode_textures};

/// A loaded package, shared by every island that shows it; it lives as long as its last handle.
pub type Handle = Arc<Package>;

/// A package's avatar ready to show: validated, compiled once, textures decoded.
pub struct Package {
    /// Unique in the process: the render thread keys its uploads by it.
    pub id: u64,
    /// The avatar, validated.
    pub avatar: Document,
    /// Its systems compiled once; islands share it.
    pub model: Arc<Model>,
    /// Its decoded textures, kept for uploads to a rebuilt scene (ADR 0104).
    pub textures: Vec<Option<TextureImage>>,
}

impl Package {
    /// A handle on a validated `avatar` with its decoded `textures`.
    pub fn new(avatar: Document, textures: Vec<Option<TextureImage>>) -> Handle {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Arc::new(Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            model: Arc::new(Model::new(&avatar)),
            avatar,
            textures,
        })
    }
}

/// Runs `load` on the I/O pool (a quarter of the logical cores, at least one thread); poll the task between frames.
pub fn spawn(load: impl FnOnce() -> Result<Handle, String> + Send + 'static) -> Task<Result<Handle, String>> {
    IoTaskPool::get_or_init(|| {
        let n = std::thread::available_parallelism().map_or(1, |n| n.get() / 4).max(1);
        TaskPoolBuilder::new()
            .num_threads(n)
            .thread_name("civbb io".into())
            .build()
    })
    .spawn(async move { load() })
}

/// Reads, verifies and validates a package signed by the key whose seed is in `key`, and decodes its textures.
#[allow(clippy::type_complexity)]
pub fn read(path: &Path, key: &Path) -> Result<(Document, Vec<Option<TextureImage>>), String> {
    let seed: [u8; 32] = std::fs::read(key)
        .map_err(|e| format!("{}: {e}", key.display()))?
        .try_into()
        .map_err(|_| format!("{}: a key file holds exactly 32 bytes", key.display()))?;
    let limits = ReadLimits::default();
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let key = SigningKey::from_bytes(&seed).verifying_key();
    let pkg = civbb_format::Package::decode_verified(&bytes, &key, &limits).map_err(|e| e.to_string())?;
    let avatar = pkg.document(&limits).map_err(|e| e.to_string())?;
    validate(&avatar, &Caps::default(), |id| pkg.blob(id).is_some()).map_err(|errors| {
        let list: Vec<String> = errors.iter().map(ToString::to_string).collect();
        format!("invalid avatar: {}", list.join("; "))
    })?;
    let textures = decode_textures(&avatar, |id| pkg.blob(id));
    Ok((avatar, textures))
}
