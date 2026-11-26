//! The material records the renderer uploads (`civbb_render::material::pack`), pinned so a restructuring of the
//! material model (ADR 0090) can show it changes none. `tests/golden/material-records.txt` holds the records of the
//! generated corpus, the Unity dialect fixtures and the toon fixture (also a fuzz seed); `CIVBB_BLESS=1` rewrites it
//! after an intended change. `CIVBB_RECORDS=<parity out dir>` (ignored test, local) writes `<dir>/<name>/records.txt`
//! from each compiled `pkg.civpkg` of a `tools/parity.sh` run and `<dir>/vrm-records/<name>.txt` from
//! `corpus/local/vrm`, to diff two runs.

use std::fmt::Write;
use std::path::Path;

use civbb_format::{Document, Package, ReadLimits, SigningKey};
use civbb_pipeline::gltf_import::{Limits, import};
use civbb_render::material::pack;

/// One line per material: its index, name, draw queue (the renderer sorts by it) and record words in hex.
fn records(doc: &Document) -> String {
    let identity: Vec<u32> = (0..doc.textures.len() as u32).collect();
    let mut out = String::new();
    for (i, m) in doc.materials.iter().enumerate() {
        write!(out, "{i} {:?} queue {}", m.name, m.queue()).unwrap();
        for w in pack(m, &identity) {
            write!(out, " {w:08x}").unwrap();
        }
        out.push('\n');
    }
    out
}

#[test]
fn material_records_match_the_golden() {
    let lean = civbb_corpus::generate(&civbb_corpus::lean());
    let bloated = civbb_corpus::generate(&civbb_corpus::bloated());
    let imported = |name: &str, bytes: &[u8]| {
        import(bytes, &Limits::default(), |_| None)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .avatar
    };
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut sources = vec![
        ("lean".to_string(), lean.clone()),
        ("bloated".to_string(), bloated),
        (
            "lean.glb".to_string(),
            imported("lean.glb", &civbb_corpus::gltf::to_glb(&lean)),
        ),
        (
            "lean-source.glb".to_string(),
            imported("lean-source.glb", &civbb_corpus::gltf::to_source_glb(&lean)),
        ),
    ];
    for name in [
        "unity-dialect-1.gltf",
        "unity-dialect-1-eye-bones.gltf",
        "toon-materials.gltf",
    ] {
        let bytes = std::fs::read(fixtures.join(name)).unwrap();
        sources.push((name.to_string(), imported(name, &bytes)));
    }
    let mut got = String::new();
    for (name, doc) in &sources {
        writeln!(got, "# {name}").unwrap();
        got.push_str(&records(doc));
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/material-records.txt");
    if std::env::var_os("CIVBB_BLESS").is_some() {
        std::fs::write(&path, &got).unwrap();
    }
    let golden = std::fs::read_to_string(&path).expect("golden missing: run with CIVBB_BLESS=1");
    for (g, w) in got.lines().zip(golden.lines()) {
        assert_eq!(g, w, "a material record changed");
    }
    assert_eq!(
        got.lines().count(),
        golden.lines().count(),
        "the number of materials changed"
    );
}

#[test]
#[ignore = "local: needs a tools/parity.sh run; set CIVBB_RECORDS"]
fn material_records_of_a_parity_run() {
    let dir = std::env::var("CIVBB_RECORDS").expect("set CIVBB_RECORDS to a tools/parity.sh out dir");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/local/unity-ref/key");
    let seed: [u8; 32] = std::fs::read(&root).unwrap().try_into().unwrap();
    let key = SigningKey::from_bytes(&seed).verifying_key();
    let limits = ReadLimits::default();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let export = entry.unwrap().path();
        let Ok(bytes) = std::fs::read(export.join("pkg.civpkg")) else {
            continue;
        };
        let pkg = Package::decode_verified(&bytes, &key, &limits).unwrap();
        let doc = pkg.document(&limits).unwrap();
        std::fs::write(export.join("records.txt"), records(&doc)).unwrap();
        println!("{}: {} materials", export.display(), doc.materials.len());
    }
    // MToon and VRM 0 materials: the local VRM files, imported.
    let vrm = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/local/vrm");
    let out = Path::new(&dir).join("vrm-records");
    std::fs::create_dir_all(&out).unwrap();
    for entry in std::fs::read_dir(vrm).into_iter().flatten() {
        let path = entry.unwrap().path();
        let Ok(im) = import(&std::fs::read(&path).unwrap(), &Limits::default(), |_| None) else {
            continue;
        };
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        std::fs::write(out.join(format!("{name}.txt")), records(&im.avatar)).unwrap();
        println!("{name}: {} materials", im.avatar.materials.len());
    }
}
