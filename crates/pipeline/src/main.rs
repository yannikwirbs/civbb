//! `civbb`: build, validate and inspect avatar packages from the command line (design doc §9, creator tooling).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use civbb_format::cost::{Measured, Parity};
use civbb_format::package::Platform;
use civbb_format::{Caps, Package, ReadLimits, SigningKey, validate};
use civbb_pipeline::gltf_import::dir_resolver;
use civbb_pipeline::{Imported, Limits, compile, import, package, stats};

const USAGE: &str = "usage:
  civbb keygen <key-file>
  civbb build <source.gltf|.glb|.vrm> --key <key-file> [-o <out.civpkg>] [--quest] [--report <r.json> [--measure]]
  civbb validate <package.civpkg> --key <key-file>
  civbb inspect <source or package.civpkg> [--key <key-file>]

A key file holds a 32-byte Ed25519 seed. Packages are verified against the key that signed them.
--report writes the per-avatar report as JSON and, next to it, Markdown (`r.md`); --measure adds CPU and GPU
times from `civbb-client measure`, which must sit next to `civbb` and be a release build, and FLIP parity of the
compiled avatar against the import before any stage from `civbb-client parity`, with images of the rest state and
of every state that is not green in `r.parity/`.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::Usage) => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
        Err(Error::Failed(e)) => {
            eprintln!("civbb: {e}");
            ExitCode::FAILURE
        }
    }
}

enum Error {
    Usage,
    Failed(String),
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Self::Failed(s)
    }
}

/// Parsed arguments: one positional input plus options.
struct Args {
    input: PathBuf,
    key: Option<PathBuf>,
    out: Option<PathBuf>,
    quest: bool,
    report: Option<PathBuf>,
    measure: bool,
}

fn parse(rest: &[String]) -> Result<Args, Error> {
    let mut a = Args {
        input: PathBuf::new(),
        key: None,
        out: None,
        quest: false,
        report: None,
        measure: false,
    };
    let mut input = None;
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--key" => a.key = Some(it.next().ok_or(Error::Usage)?.into()),
            "-o" => a.out = Some(it.next().ok_or(Error::Usage)?.into()),
            "--quest" => a.quest = true,
            "--report" => a.report = Some(it.next().ok_or(Error::Usage)?.into()),
            "--measure" => a.measure = true,
            s if s.starts_with('-') || input.is_some() => return Err(Error::Usage),
            s => input = Some(PathBuf::from(s)),
        }
    }
    a.input = input.ok_or(Error::Usage)?;
    if a.measure && a.report.is_none() {
        return Err(Error::Usage);
    }
    Ok(a)
}

fn run(args: &[String]) -> Result<(), Error> {
    let (cmd, rest) = args.split_first().ok_or(Error::Usage)?;
    let a = parse(rest)?;
    match cmd.as_str() {
        "keygen" => keygen(&a.input),
        "build" => {
            let key_path = a.key.as_deref().ok_or(Error::Usage)?;
            let key = read_key(key_path)?;
            let platform = if a.quest { Platform::Quest } else { Platform::Desktop };
            let imported = import_file(&a.input)?;
            // The parity reference: the import packaged without any compiler stage (ADR 0033).
            let source = a.measure.then(|| {
                let copy = Imported {
                    avatar: imported.avatar.clone(),
                    blobs: imported.blobs.clone(),
                    warnings: Vec::new(),
                };
                package(copy, platform).encode_signed(&key)
            });
            let (bytes, mut report) = compile(imported, platform, &key, |l| println!("{l}"))?;
            let out = a.out.unwrap_or_else(|| a.input.with_extension("civpkg"));
            fs::write(&out, &bytes).map_err(|e| format!("{}: {e}", out.display()))?;
            println!("wrote {} ({} bytes)", out.display(), bytes.len());
            if let Some(path) = a.report {
                if let Some(source) = source {
                    report.timings = Some(measure(&out, key_path)?);
                    let reference = out.with_extension("source.civpkg");
                    fs::write(&reference, source).map_err(|e| format!("{}: {e}", reference.display()))?;
                    let images = path.with_extension("parity");
                    let parity = parity(&reference, &out, key_path, &images);
                    let _ = fs::remove_file(&reference);
                    report.parity = parity?;
                }
                let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
                fs::write(
                    &path,
                    json + "
",
                )
                .map_err(|e| format!("{}: {e}", path.display()))?;
                let md = path.with_extension("md");
                fs::write(&md, report.markdown()).map_err(|e| format!("{}: {e}", md.display()))?;
                println!("wrote {} and {}", path.display(), md.display());
            }
            Ok(())
        }
        "validate" => {
            let key = read_key(a.key.as_deref().ok_or(Error::Usage)?)?;
            let pkg = read_package(&a.input, &key)?;
            println!("ok: {} blobs, signed by this key, avatar valid", pkg.blobs.len());
            Ok(())
        }
        "inspect" => {
            let (avatar, blobs) = if a.input.extension().is_some_and(|x| x == "civpkg") {
                let key = read_key(a.key.as_deref().ok_or(Error::Usage)?)?;
                let pkg = read_package(&a.input, &key)?;
                println!("compiler    {}", pkg.header.compiler);
                println!("platform    {:?}", pkg.header.platform);
                (
                    pkg.avatar(&ReadLimits::default()).map_err(|e| e.to_string())?,
                    pkg.blobs,
                )
            } else {
                let imported = import_file(&a.input)?;
                (imported.avatar, imported.blobs)
            };
            println!("name        {}", avatar.name);
            println!("{}", stats(&avatar, &blobs));
            Ok(())
        }
        _ => Err(Error::Usage),
    }
}

fn keygen(path: &Path) -> Result<(), Error> {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| format!("randomness: {e}"))?;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    f.write_all(&seed).map_err(|e| e.to_string())?;
    let public = SigningKey::from_bytes(&seed).verifying_key();
    let hex: String = public.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
    println!("wrote {}; public key {hex}", path.display());
    Ok(())
}

/// Runs `civbb-client measure` on the written package; the client binary sits next to this one (ADR 0032).
fn measure(package: &Path, key: &Path) -> Result<Measured, Error> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let client = exe.with_file_name(format!("civbb-client{}", std::env::consts::EXE_SUFFIX));
    let output = std::process::Command::new(&client)
        .arg("measure")
        .arg(package)
        .arg("--key")
        .arg(key)
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|e| format!("{}: {e}", client.display()))?;
    if !output.status.success() {
        return Err(format!("{} measure failed ({})", client.display(), output.status).into());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("civbb-client measure output: {e}").into())
}

/// Runs `civbb-client parity` on the source and compiled packages, images into `images` (ADR 0033); `None` without
/// a Vulkan device.
fn parity(reference: &Path, test: &Path, key: &Path, images: &Path) -> Result<Option<Parity>, Error> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let client = exe.with_file_name(format!("civbb-client{}", std::env::consts::EXE_SUFFIX));
    let output = std::process::Command::new(&client)
        .arg("parity")
        .arg(reference)
        .arg(test)
        .arg("--key")
        .arg(key)
        .arg("--images")
        .arg(images)
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|e| format!("{}: {e}", client.display()))?;
    if !output.status.success() {
        return Err(format!("{} parity failed ({})", client.display(), output.status).into());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("civbb-client parity output: {e}").into())
}

fn read_key(path: &Path) -> Result<SigningKey, Error> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let seed: [u8; 32] = bytes
        .try_into()
        .map_err(|_| format!("{}: a key file holds exactly 32 bytes", path.display()))?;
    Ok(SigningKey::from_bytes(&seed))
}

fn import_file(path: &Path) -> Result<Imported, Error> {
    let limits = Limits::default();
    let len = fs::metadata(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .len();
    if len > limits.bytes as u64 {
        return Err(format!("{}: {len} bytes exceeds limit {}", path.display(), limits.bytes).into());
    }
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let imported = import(&bytes, &limits, dir_resolver(dir, limits.bytes)).map_err(|e| e.to_string())?;
    for w in &imported.warnings {
        eprintln!("warning: {w}");
    }
    Ok(imported)
}

fn read_package(path: &Path, key: &SigningKey) -> Result<Package, Error> {
    let limits = ReadLimits::default();
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let pkg = Package::decode_verified(&bytes, &key.verifying_key(), &limits).map_err(|e| e.to_string())?;
    let avatar = pkg.avatar(&limits).map_err(|e| e.to_string())?;
    validate(&avatar, &Caps::default(), |id| pkg.blob(id).is_some()).map_err(|errors| {
        let list: Vec<String> = errors.iter().map(ToString::to_string).collect();
        format!("invalid avatar: {}", list.join("; "))
    })?;
    Ok(pkg)
}
