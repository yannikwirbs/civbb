//! The per-avatar report: counts after every compiler stage, the package size and measured times, as JSON and
//! Markdown (ADR 0032). [`compile`] runs the stages in order and records it.

use std::fmt::Write;

use civbb_format::cost::Measured;
use civbb_format::package::Platform;
use civbb_format::{FORMAT_VERSION, SigningKey};
use serde::Serialize;

use crate::{COMPILER, Imported, Stats, atlas, collapse, compress, lod, merge, package, prune, stats, texture};

/// The report shape's version: bumped when a key is renamed or removed or changes meaning (ADR 0032).
pub const REPORT_VERSION: u32 = 1;

/// One avatar's report; serialised as the JSON report.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Report {
    /// [`REPORT_VERSION`].
    pub report: u32,
    /// The avatar's name.
    pub avatar: String,
    /// The compiler that built the package.
    pub compiler: String,
    /// The package's format version.
    pub format_version: u32,
    /// The package's platform.
    pub platform: Platform,
    /// Counts after import and after each stage, in compiler order.
    pub stages: Vec<Stage>,
    /// Size of the signed package.
    pub package_bytes: usize,
    /// Times measured by `civbb-client measure`; `None` unless measured.
    pub timings: Option<Measured>,
}

/// Counts after one stage.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Stage {
    /// `source` (after import), then the stage's name.
    pub stage: &'static str,
    /// The counts.
    #[serde(flatten)]
    pub stats: Stats,
}

/// Runs every compiler stage on `imported` for `platform`, logs each stage's outcome to `log`, and packages and
/// signs the result with `key`. Returns the signed package and its report, without timings.
pub fn compile(
    mut imported: Imported,
    platform: Platform,
    key: &SigningKey,
    mut log: impl FnMut(String),
) -> Result<(Vec<u8>, Report), String> {
    let mut stages = Vec::with_capacity(7);
    let mut snapshot = |stage, i: &Imported| {
        stages.push(Stage {
            stage,
            stats: stats(&i.avatar, &i.blobs),
        })
    };
    snapshot("source", &imported);
    let pruned = prune(&mut imported.avatar);
    log(format!(
        "pruned {} nodes, {} skin joints, {} blendshapes",
        pruned.nodes, pruned.joints, pruned.shapes
    ));
    snapshot("prune", &imported);
    let atlased = atlas(&mut imported, texture::max_side(platform))?;
    log(format!(
        "atlased {} atlases, {} fewer textures, {} fewer materials",
        atlased.atlases, atlased.textures, atlased.materials
    ));
    snapshot("atlas", &imported);
    let merged = merge(&mut imported.avatar);
    log(format!(
        "merged {} meshes, {} submeshes",
        merged.meshes, merged.submeshes
    ));
    snapshot("merge", &imported);
    let l = lod(&mut imported.avatar);
    log(format!(
        "lod chain on {} meshes: triangles {:?}, error {:?} of mesh extent",
        l.meshes, l.triangles, l.error
    ));
    snapshot("lod", &imported);
    log(format!("collapsed {} toggle layers", collapse(&mut imported.avatar)));
    snapshot("collapse", &imported);
    let c = compress(&mut imported, platform)?;
    log(format!(
        "compressed {} textures to KTX2 ({} UASTC, {} capped), {} to {} bytes",
        c.textures, c.uastc, c.capped, c.bytes_before, c.bytes_after
    ));
    snapshot("compress", &imported);
    let avatar = imported.avatar.name.clone();
    let bytes = package(imported, platform).encode_signed(key);
    let report = Report {
        report: REPORT_VERSION,
        avatar,
        compiler: COMPILER.into(),
        format_version: FORMAT_VERSION,
        platform,
        stages,
        package_bytes: bytes.len(),
        timings: None,
    };
    Ok((bytes, report))
}

/// Rows of the Markdown counts table: label and value.
type Row = (String, Box<dyn Fn(&Stats) -> usize>);

impl Report {
    /// The report as Markdown: counts per stage (a stage column shows only what it changed, `after` shows all),
    /// the package size and the timings.
    pub fn markdown(&self) -> String {
        let mut md = String::new();
        let _ = writeln!(md, "# Report: {}\n", self.avatar);
        let _ = writeln!(
            md,
            "{}, format v{}, {:?}, report v{}. Signed package: {} bytes.\n",
            self.compiler, self.format_version, self.platform, self.report, self.package_bytes
        );
        let levels = self.stages.iter().map(|s| s.stats.lod.len()).max().unwrap_or(1);
        let row = |label: &str, f: fn(&Stats) -> usize| -> Row { (label.into(), Box::new(f)) };
        let mut rows: Vec<Row> = vec![
            row("nodes", |s| s.nodes),
            row("bones", |s| s.bones),
            row("meshes", |s| s.meshes),
            row("skinned meshes", |s| s.skinned_meshes),
            row("draws (all meshes shown)", |s| s.draws),
            row("vertices", |s| s.vertices),
            row("triangles", |s| s.triangles),
        ];
        for n in 1..levels {
            let level = move |s: &Stats| s.lod.get(n).or(s.lod.last()).copied().unwrap_or_default();
            rows.push((format!("LOD {n} vertices"), Box::new(move |s| level(s).vertices)));
            rows.push((format!("LOD {n} triangles"), Box::new(move |s| level(s).triangles)));
        }
        rows.extend([
            row("blend shapes", |s| s.blend_shapes),
            row("shape deltas", |s| s.shape_deltas),
            row("materials", |s| s.materials),
            row("toon materials", |s| s.toon_materials),
            row("textures", |s| s.textures),
            row("texture bytes", |s| s.texture_bytes),
            row("clips", |s| s.clips),
            row("curves", |s| s.curves),
            row("parameters", |s| s.parameters),
            row("animator layers", |s| s.layers),
            row("states", |s| s.states),
            row("transitions", |s| s.transitions),
            row("animator bytes", |s| s.animator_bytes),
            row("synced bits", |s| s.synced_bits as usize),
            row("chains", |s| s.chains),
            row("colliders", |s| s.colliders),
            row("contacts", |s| s.contacts),
            row("constraints", |s| s.constraints),
        ]);
        md.push_str("| |");
        for s in &self.stages {
            let _ = write!(md, " {} |", s.stage);
        }
        md.push_str(" after |\n| --- |");
        md.push_str(&" ---: |".repeat(self.stages.len() + 1));
        md.push('\n');
        for (label, value) in &rows {
            let _ = write!(md, "| {label} |");
            let mut last = None;
            for s in &self.stages {
                let v = value(&s.stats);
                match last {
                    Some(l) if l == v => md.push_str(" |"),
                    _ => {
                        let _ = write!(md, " {v} |");
                    }
                }
                last = Some(v);
            }
            let _ = writeln!(md, " {} |", last.unwrap_or(0));
        }
        md.push_str("\nThe source column shows every count; a stage column only what that stage changed.\n");
        match &self.timings {
            None => md.push_str("\nNot measured (`civbb build --measure` runs `civbb-client measure`).\n"),
            Some(t) => {
                let c = &t.cpu;
                let _ = writeln!(
                    md,
                    "\n## CPU ({} build, {} worker of {} logical cores)\n\n{} avatars, {} frames after {} warm-up.\n",
                    t.profile, c.threads, c.logical_cores, c.avatars, c.frames, c.warmup
                );
                md.push_str("| | ms |\n| --- | ---: |\n");
                let _ = writeln!(md, "| frame, mean | {:.3} |", c.ms_per_frame);
                let _ = writeln!(md, "| frame, p99 | {:.3} |", c.ms_per_frame_p99);
                let _ = writeln!(md, "| **per avatar** | **{:.4}** |", c.ms_per_avatar);
                for (system, ms) in &c.systems_ms_per_avatar {
                    let _ = writeln!(md, "| per avatar: {system} | {ms:.4} |");
                }
                match &t.gpu {
                    None => md.push_str("\n## GPU\n\nNot measured: no Vulkan device.\n"),
                    Some(g) => {
                        let _ = writeln!(
                            md,
                            "\n## GPU ({})\n\n{} copies at {}x{}, {} frames after the warm-up; {} draws and {} \
                             skinned meshes per frame.\n",
                            g.device, g.copies, g.width, g.height, g.frames, g.draws, g.skinned_meshes
                        );
                        md.push_str("| | ms |\n| --- | ---: |\n");
                        let _ = writeln!(md, "| GPU per frame | {:.3} |", g.gpu_ms_per_frame);
                        let _ = writeln!(md, "| **GPU per avatar** | **{:.4}** |", g.gpu_ms_per_avatar);
                        let _ = writeln!(md, "| viewer CPU per frame | {:.3} |", g.cpu_ms_per_frame);
                        let _ = writeln!(md, "| avatar systems per frame | {:.3} |", g.systems_ms_per_frame);
                    }
                }
            }
        }
        md
    }
}
