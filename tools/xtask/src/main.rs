//! Repo automation: `cargo xtask ci | arch | codemap [--check]`.
//!
//! - `ci`: the full local gate (fmt, clippy, tests, arch rules, fresh code map). Run before every commit.
//! - `arch`: enforces the crate dependency rules from design doc §21.
//! - `codemap`: regenerates `docs/ai/CODEMAP.md`; `--check` fails if it is stale or a file lacks a module doc.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Crates with no internal dependencies at all.
const LEAF: &[&str] = &["civbb-format", "civbb-protocol", "civbb-wit"];
/// Engine crates: may depend only on `civbb-core` and `civbb-format`.
const ENGINE: &[&str] = &[
    "civbb-core",
    "civbb-render",
    "civbb-anim",
    "civbb-dynamics",
    "civbb-audio",
    "civbb-xr",
    "civbb-ui",
    "civbb-script-host",
];
/// Glue crates: binaries that wire systems together; nothing may depend on them.
const GLUE: &[&str] = &["civbb-client", "civbb-node", "civbb-pipeline"];
/// Crates exempt from classification.
const TOOLS: &[&str] = &["xtask", "civbb-corpus"];

const CODEMAP: &str = "docs/ai/CODEMAP.md";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check = args.iter().any(|a| a == "--check");
    let result = match args.first().map(String::as_str) {
        Some("ci") => ci(),
        Some("arch") => arch(),
        Some("codemap") => codemap(check),
        _ => Err("usage: cargo xtask <ci|arch|codemap [--check]>".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    // Not canonicalised: on Windows that adds a verbatim path prefix that cargo metadata paths lack.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .to_path_buf()
}

fn run(cmd: &str, args: &[&str]) -> Result<(), String> {
    eprintln!("xtask: $ {cmd} {}", args.join(" "));
    let status = Command::new(cmd)
        .args(args)
        .current_dir(root())
        .status()
        .map_err(|e| format!("{cmd}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{cmd} {}` failed", args.join(" ")))
    }
}

fn ci() -> Result<(), String> {
    run("cargo", &["fmt", "--all", "--", "--check"])?;
    run(
        "cargo",
        &["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    )?;
    run("cargo", &["test", "--workspace", "--quiet"])?;
    arch()?;
    codemap(true)?;
    eprintln!("xtask: ci passed");
    Ok(())
}

struct Package {
    name: String,
    description: Option<String>,
    dir: PathBuf,
    internal_deps: Vec<String>,
}

fn packages() -> Result<Vec<Package>, String> {
    let out = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(root())
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let mut pkgs = Vec::new();
    for p in meta["packages"].as_array().into_iter().flatten() {
        let manifest = PathBuf::from(p["manifest_path"].as_str().unwrap_or_default());
        let internal_deps = p["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            // Dev-dependencies may cross layers (tests), normal and build deps may not.
            .filter(|d| d["kind"].as_str() != Some("dev"))
            .filter_map(|d| d["name"].as_str())
            .filter(|n| n.starts_with("civbb-"))
            .map(str::to_owned)
            .collect();
        pkgs.push(Package {
            name: p["name"].as_str().unwrap_or_default().to_owned(),
            description: p["description"].as_str().map(str::to_owned),
            dir: manifest.parent().map(Path::to_path_buf).unwrap_or_default(),
            internal_deps,
        });
    }
    pkgs.sort_by(|a, b| a.dir.cmp(&b.dir));
    Ok(pkgs)
}

fn arch() -> Result<(), String> {
    let mut errors = Vec::new();
    for p in packages()? {
        let n = p.name.as_str();
        if TOOLS.contains(&n) {
            continue;
        }
        if p.description.is_none() {
            errors.push(format!("{n}: Cargo.toml needs a `description` (its one job)"));
        }
        if !p.dir.join("README.md").exists() {
            errors.push(format!("{n}: missing README.md"));
        }
        for d in &p.internal_deps {
            if GLUE.contains(&d.as_str()) {
                errors.push(format!("{n} -> {d}: nothing may depend on a glue crate"));
            }
        }
        if LEAF.contains(&n) {
            for d in &p.internal_deps {
                errors.push(format!("{n} -> {d}: leaf crates have no internal deps"));
            }
        } else if ENGINE.contains(&n) {
            for d in &p.internal_deps {
                if d != "civbb-core" && d != "civbb-format" {
                    errors.push(format!("{n} -> {d}: engine crates depend only on core and format"));
                }
            }
        } else if GLUE.contains(&n) {
            if n == "civbb-node" && p.internal_deps.iter().any(|d| d == "civbb-render") {
                errors.push("civbb-node -> civbb-render: the node never renders".into());
            }
        } else {
            errors.push(format!("{n}: unclassified crate; add it to a layer in tools/xtask"));
        }
    }
    if errors.is_empty() {
        eprintln!("xtask: arch rules ok");
        Ok(())
    } else {
        Err(format!("arch rules violated:\n  {}", errors.join("\n  ")))
    }
}

/// First line of the leading `//!` block of a Rust file, or first `//` line of a shader.
fn file_summary(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let prefix = if path.extension().is_some_and(|e| e == "rs") {
        "//!"
    } else {
        "//"
    };
    text.lines()
        .map(str::trim)
        .find(|l| l.starts_with(prefix) && l.len() > prefix.len())
        .map(|l| l.trim_start_matches(prefix).trim().to_owned())
}

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if p.is_dir() {
            if name != "target" && !name.starts_with('.') {
                source_files(&p, out);
            }
        } else if matches!(
            p.extension().and_then(|e| e.to_str()),
            Some("rs" | "slang" | "hlsl" | "glsl" | "comp" | "vert" | "frag" | "ts")
        ) {
            out.push(p);
        }
    }
}

fn rel(path: &Path) -> String {
    path.strip_prefix(root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn markdown_title(path: &Path) -> String {
    fs::read_to_string(path)
        .ok()
        .and_then(|t| {
            t.lines()
                .find(|l| l.starts_with("# "))
                .map(|l| l[2..].trim().to_owned())
        })
        .unwrap_or_default()
}

fn codemap(check: bool) -> Result<(), String> {
    let mut md = String::from(
        "# Code map\n\nGenerated by `cargo xtask codemap`; do not edit. CI fails when it is stale.\n\
         Each line is a file and the first line of its module doc. Read this instead of walking the tree.\n",
    );
    let mut missing = Vec::new();

    for p in packages()? {
        let deps = if p.internal_deps.is_empty() {
            "none".into()
        } else {
            p.internal_deps.join(", ")
        };
        md += &format!(
            "\n## `{}` ({})\n\n{}\n\nInternal deps: {deps}\n\n",
            rel(&p.dir),
            p.name,
            p.description.as_deref().unwrap_or("(no description)")
        );
        let mut files = Vec::new();
        source_files(&p.dir, &mut files);
        for f in files {
            let summary = file_summary(&f).unwrap_or_else(|| {
                missing.push(rel(&f));
                "(missing module doc)".into()
            });
            let shown = f
                .strip_prefix(&p.dir)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            md += &format!("- `{shown}`: {summary}\n");
        }
    }

    let mut docs: BTreeMap<String, String> = BTreeMap::new();
    for dir in ["docs", "docs/adr", "docs/ai"] {
        for e in fs::read_dir(root().join(dir)).into_iter().flatten().flatten() {
            let p = e.path();
            // The map itself is excluded so generation is idempotent.
            if p.extension().is_some_and(|x| x == "md") && !p.ends_with(CODEMAP) {
                docs.insert(rel(&p), markdown_title(&p));
            }
        }
    }
    md += "\n## Docs\n\n";
    for (path, title) in docs {
        md += &format!("- `{path}`: {title}\n");
    }

    let target = root().join(CODEMAP);
    let current = fs::read_to_string(&target).unwrap_or_default().replace("\r\n", "\n");
    if check {
        if !missing.is_empty() {
            return Err(format!("files without a module doc:\n  {}", missing.join("\n  ")));
        }
        if current != md {
            return Err(format!("{CODEMAP} is stale; run `cargo xtask codemap`"));
        }
        eprintln!("xtask: code map fresh");
    } else if current != md {
        fs::write(&target, md).map_err(|e| e.to_string())?;
        eprintln!("xtask: wrote {CODEMAP}");
    }
    Ok(())
}
