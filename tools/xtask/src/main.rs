//! Repo automation: `cargo xtask ci | test | arch | codemap [--check] | shaders [--check]`.
//!
//! - `ci`: the full local gate (fmt, clippy, tests, arch rules, fresh code map and shaders). Run before every commit.
//! - `test`: tests only the crates changed since `HEAD` and the crates that depend on them; the quick check.
//! - `arch`: enforces the crate dependency rules from design doc §21.
//! - `rules`: source rules clippy can't express: no Rust file over 1,000 lines, no `unwrap` outside tests.
//! - `codemap`: regenerates `docs/ai/CODEMAP.md`; `--check` fails if it is stale or a file lacks a module doc.
//! - `shaders`: compiles `crates/*/shaders/*.slang` to committed SPIR-V with `slangc` (`$VULKAN_SDK/Bin` or PATH);
//!   `--check` fails if a source changed since (ADR 0021), without needing the compiler.

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

/// Longest a Rust file may be; past it, split along a real seam (Phase 0 health check).
const MAX_FILE_LINES: usize = 1000;
/// Split so this file does not match itself.
const UNWRAP: &str = concat!(".unwrap", "()");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check = args.iter().any(|a| a == "--check");
    let result = match args.first().map(String::as_str) {
        Some("ci") => ci(),
        Some("test") => test(),
        Some("arch") => arch(),
        Some("rules") => rules(),
        Some("codemap") => codemap(check),
        Some("shaders") => shaders(check),
        Some("schema") => schema(),
        _ => Err("usage: cargo xtask <ci|test|arch|rules|codemap [--check]|shaders [--check]|schema>".into()),
    };
    match result {
        Ok(()) => {
            if args.first().is_some_and(|a| a != "ci" && a != "test") {
                eprintln!("xtask: {} ok", args.join(" "));
            }
            ExitCode::SUCCESS
        }
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

/// Runs a command with its output captured and returns that output; on failure the error holds only the relevant
/// lines (see `relevant`).
fn quiet(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(cmd)
        .args(args)
        .current_dir(root())
        .output()
        .map_err(|e| format!("{cmd}: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    if out.status.success() {
        Ok(text)
    } else {
        Err(format!("`{cmd} {}` failed:\n{}", args.join(" "), relevant(&text)))
    }
}

/// Cargo and libtest output without progress lines and passing-test noise: errors, warnings and failures remain.
fn relevant(text: &str) -> String {
    const NOISE: &[&str] = &[
        "Compiling ",
        "Checking ",
        "Finished ",
        "Running ",
        "Doc-tests ",
        "Blocking ",
        "Downloaded ",
        "Downloading ",
        "Updating ",
        "Locking ",
        "running ",
        "test result: ok.",
        // nextest's banners and section rules; its FAIL lines, captured output and Summary remain.
        "Nextest run ID ",
        "Starting ",
        "────",
        "failures:",
        "test result: FAILED.",
        "error: test run failed",
    ];
    // CI sets `CARGO_TERM_COLOR=always`: drop ANSI colour codes (`ESC [ ... m`) so the prefixes match.
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            chars.by_ref().find(|c| c.is_ascii_alphabetic());
        } else {
            plain.push(c);
        }
    }
    plain
        .lines()
        .filter(|l| {
            let t = l.trim();
            // `--quiet` progress: dots, `i` for ignored, and a ` 100/250` counter on long runs.
            let progress = t.chars().all(|c| ".i0123456789/ ".contains(c));
            !progress && !NOISE.iter().any(|n| t.starts_with(n))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Totals of libtest's `test result:` lines and nextest's `Summary` line, e.g. `812 passed, 3 ignored`.
fn test_totals(text: &str) -> String {
    let (mut passed, mut ignored) = (0, 0);
    let summaries = text
        .lines()
        .filter(|l| l.starts_with("test result:") || l.contains(" tests run: "));
    for line in summaries {
        let words: Vec<&str> = line.split_whitespace().collect();
        for pair in words.windows(2) {
            let what = pair[1].trim_end_matches([';', ',', '.']);
            match (pair[0].parse::<u32>(), what) {
                (Ok(n), "passed") => passed += n,
                (Ok(n), "ignored" | "skipped") => ignored += n,
                _ => {}
            }
        }
    }
    format!("{passed} passed, {ignored} ignored")
}

/// Runs the tests of `packages` (the workspace when empty): with cargo-nextest when it is installed, which runs
/// test binaries in parallel (full suite 18.7 s -> 9.5 s), plus doctests, which nextest does not run.
fn run_tests(packages: &[&str]) -> Result<String, String> {
    let mut scope = Vec::new();
    if packages.is_empty() {
        scope.push("--workspace");
    }
    for p in packages {
        scope.extend(["-p", p]);
    }
    let nextest = Command::new("cargo")
        .args(["nextest", "--version"])
        .output()
        .is_ok_and(|o| o.status.success());
    if !nextest {
        return quiet("cargo", &[&["test", "--quiet"], &scope[..]].concat());
    }
    let run = [
        "nextest",
        "run",
        "--cargo-quiet",
        "--no-fail-fast",
        "--no-tests=pass",
        "--status-level",
        "fail",
        "--final-status-level",
        "fail",
    ];
    let out = quiet("cargo", &[&run[..], &scope[..]].concat())?;
    Ok(out + &quiet("cargo", &[&["test", "--doc", "--quiet"], &scope[..]].concat())?)
}

/// Tests the crates changed since `HEAD` (tracked or untracked files) and every crate whose tests depend on them.
fn test() -> Result<(), String> {
    let start = std::time::Instant::now();
    let status = quiet("git", &["status", "--porcelain", "--untracked-files=all"])?;
    // Porcelain lines are `XY path` or `XY old -> new`; the path that exists now is last.
    let changed: Vec<&str> = status
        .lines()
        .filter_map(|l| l.get(3..))
        .map(|p| p.rsplit(" -> ").next().unwrap_or(p).trim_matches('"'))
        .collect();
    let pkgs = packages()?;
    let everything = changed
        .iter()
        .any(|p| matches!(*p, "Cargo.toml" | "Cargo.lock") || p.starts_with(".cargo/"));
    let mut hit: Vec<&str> = pkgs
        .iter()
        .filter(|p| {
            let dir = rel(&p.dir) + "/";
            everything || changed.iter().any(|c| c.starts_with(&dir))
        })
        .map(|p| p.name.as_str())
        .collect();
    loop {
        let more: Vec<&str> = pkgs
            .iter()
            .filter(|p| !hit.contains(&p.name.as_str()))
            .filter(|p| p.test_deps.iter().any(|d| hit.contains(&d.as_str())))
            .map(|p| p.name.as_str())
            .collect();
        if more.is_empty() {
            break;
        }
        hit.extend(more);
    }
    if hit.is_empty() {
        eprintln!("xtask: test: no crate changed since HEAD");
        return Ok(());
    }
    hit.sort_unstable();
    let out = run_tests(&hit)?;
    eprintln!(
        "xtask: test passed ({}: {}) in {:.0} s",
        hit.join(" "),
        test_totals(&out),
        start.elapsed().as_secs_f32()
    );
    Ok(())
}

fn ci() -> Result<(), String> {
    let start = std::time::Instant::now();
    quiet("cargo", &["fmt", "--all", "--", "--check"])?;
    quiet(
        "cargo",
        &["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    )?;
    let tests = run_tests(&[])?;
    arch()?;
    rules()?;
    codemap(true)?;
    shaders(true)?;
    eprintln!(
        "xtask: ci passed (fmt, clippy, tests: {}, arch, rules, codemap, shaders) in {:.0} s",
        test_totals(&tests),
        start.elapsed().as_secs_f32()
    );
    Ok(())
}

struct Package {
    name: String,
    description: Option<String>,
    dir: PathBuf,
    internal_deps: Vec<String>,
    /// Internal dependencies of any kind, dev-dependencies included: whose changes this crate's tests see.
    test_deps: Vec<String>,
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
        let deps = |dev: bool| {
            p["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| dev || d["kind"].as_str() != Some("dev"))
                .filter_map(|d| d["name"].as_str())
                .filter(|n| n.starts_with("civbb-"))
                .map(str::to_owned)
                .collect()
        };
        pkgs.push(Package {
            name: p["name"].as_str().unwrap_or_default().to_owned(),
            description: p["description"].as_str().map(str::to_owned),
            dir: manifest.parent().map(Path::to_path_buf).unwrap_or_default(),
            // Dev-dependencies may cross layers (tests), normal and build deps may not.
            internal_deps: deps(false),
            test_deps: deps(true),
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
        Ok(())
    } else {
        Err(format!("arch rules violated:\n  {}", errors.join("\n  ")))
    }
}

/// Fails on a Rust file over [`MAX_FILE_LINES`] in any workspace crate, and on `unwrap()` in a crate's `src/`
/// before its first `#[cfg(test)]` (library and binary code): use `?`, or `expect` with why it cannot fail.
fn rules() -> Result<(), String> {
    let mut errors = Vec::new();
    for p in packages()? {
        let mut files = Vec::new();
        source_files(&p.dir, &mut files);
        for f in files.iter().filter(|f| f.extension().is_some_and(|e| e == "rs")) {
            let text = fs::read_to_string(f).map_err(|e| format!("{}: {e}", rel(f)))?;
            let lines = text.lines().count();
            if lines > MAX_FILE_LINES {
                errors.push(format!("{}: {lines} lines, at most {MAX_FILE_LINES}", rel(f)));
            }
            if !f.strip_prefix(&p.dir).is_ok_and(|r| r.starts_with("src")) {
                continue;
            }
            let code = text.lines().take_while(|l| l.trim() != "#[cfg(test)]");
            for (i, line) in code.enumerate() {
                if line.contains(UNWRAP) && !line.trim_start().starts_with("//") {
                    errors.push(format!("{}:{}: `unwrap()` outside tests", rel(f), i + 1));
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("source rules violated:\n  {}", errors.join("\n  ")))
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
    } else if current != md {
        fs::write(&target, md).map_err(|e| e.to_string())?;
        eprintln!("xtask: wrote {CODEMAP}");
    }
    Ok(())
}

/// FNV-1a over the source with LF line ends: detects a changed shader source, nothing more.
fn source_hash(path: &Path) -> Result<String, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", rel(path)))?;
    let hash = text
        .replace("\r\n", "\n")
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
        });
    Ok(format!("{hash:016x}\n"))
}

/// Rewrites `docs/spec/schema/*.schema.json` from the format types (ADR 0086 point 2); the gate's test
/// `civbb-pipeline --test schema` fails while they are stale.
fn schema() -> Result<(), String> {
    let status = Command::new("cargo")
        .args(["test", "-p", "civbb-pipeline", "--test", "schema", "--quiet"])
        .env("CIVBB_BLESS", "1")
        .current_dir(root())
        .status()
        .map_err(|e| e.to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| "schema generation failed".into())
}

/// Each `crates/<name>/shaders/<shader>.slang` compiles to `shaders/spv/<shader>.spv`, with its source's hash in
/// `shaders/spv/<shader>.hash`.
fn shaders(check: bool) -> Result<(), String> {
    let mut sources = Vec::new();
    for krate in fs::read_dir(root().join("crates"))
        .map_err(|e| e.to_string())?
        .flatten()
    {
        for e in fs::read_dir(krate.path().join("shaders"))
            .into_iter()
            .flatten()
            .flatten()
        {
            if e.path().extension().is_some_and(|x| x == "slang") {
                sources.push(e.path());
            }
        }
    }
    sources.sort();
    let slangc = match std::env::var_os("VULKAN_SDK") {
        Some(sdk) => Path::new(&sdk)
            .join("Bin")
            .join("slangc")
            .to_string_lossy()
            .into_owned(),
        None => "slangc".into(),
    };
    let mut stale = Vec::new();
    for src in sources {
        let out = src.parent().expect("in shaders/").join("spv");
        let stem = src.file_stem().expect("a file").to_string_lossy();
        let (spv, hash_file) = (out.join(format!("{stem}.spv")), out.join(format!("{stem}.hash")));
        let hash = source_hash(&src)?;
        if check {
            if !spv.exists() || fs::read_to_string(&hash_file).ok().as_deref() != Some(hash.as_str()) {
                stale.push(rel(&src));
            }
            continue;
        }
        fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        let (src_rel, spv_rel) = (rel(&src), rel(&spv));
        run(
            &slangc,
            &[
                &src_rel,
                "-target",
                "spirv",
                "-profile",
                "spirv_1_6",
                "-fvk-use-entrypoint-name",
                "-o",
                &spv_rel,
            ],
        )?;
        fs::write(&hash_file, hash).map_err(|e| e.to_string())?;
    }
    if !stale.is_empty() {
        return Err(format!(
            "shaders changed since compiled; run `cargo xtask shaders`:\n  {}",
            stale.join("\n  ")
        ));
    }
    Ok(())
}
