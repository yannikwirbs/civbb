//! `civbb`: build, validate and inspect avatar packages from the command line (design doc §9, creator tooling).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use civbb_format::package::Platform;
use civbb_format::{Caps, Package, ReadLimits, SigningKey, validate};
use civbb_pipeline::gltf_import::dir_resolver;
use civbb_pipeline::{Imported, Limits, import, package, stats};

const USAGE: &str = "usage:
  civbb keygen <key-file>
  civbb build <source.gltf|.glb|.vrm> --key <key-file> [-o <out.civpkg>] [--quest]
  civbb validate <package.civpkg> --key <key-file>
  civbb inspect <source or package.civpkg> [--key <key-file>]

A key file holds a 32-byte Ed25519 seed. Packages are verified against the key that signed them.";

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
}

fn parse(rest: &[String]) -> Result<Args, Error> {
    let mut a = Args {
        input: PathBuf::new(),
        key: None,
        out: None,
        quest: false,
    };
    let mut input = None;
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--key" => a.key = Some(it.next().ok_or(Error::Usage)?.into()),
            "-o" => a.out = Some(it.next().ok_or(Error::Usage)?.into()),
            "--quest" => a.quest = true,
            s if s.starts_with('-') || input.is_some() => return Err(Error::Usage),
            s => input = Some(PathBuf::from(s)),
        }
    }
    a.input = input.ok_or(Error::Usage)?;
    Ok(a)
}

fn run(args: &[String]) -> Result<(), Error> {
    let (cmd, rest) = args.split_first().ok_or(Error::Usage)?;
    let a = parse(rest)?;
    match cmd.as_str() {
        "keygen" => keygen(&a.input),
        "build" => {
            let key = read_key(a.key.as_deref().ok_or(Error::Usage)?)?;
            let imported = import_file(&a.input)?;
            let platform = if a.quest { Platform::Quest } else { Platform::Desktop };
            let bytes = package(imported, platform).encode_signed(&key);
            let out = a.out.unwrap_or_else(|| a.input.with_extension("civpkg"));
            fs::write(&out, &bytes).map_err(|e| format!("{}: {e}", out.display()))?;
            println!("wrote {} ({} bytes)", out.display(), bytes.len());
            Ok(())
        }
        "validate" => {
            let key = read_key(a.key.as_deref().ok_or(Error::Usage)?)?;
            let pkg = read_package(&a.input, &key)?;
            println!("ok: {} blobs, signed by this key, avatar valid", pkg.blobs.len());
            Ok(())
        }
        "inspect" => {
            let avatar = if a.input.extension().is_some_and(|x| x == "civpkg") {
                let key = read_key(a.key.as_deref().ok_or(Error::Usage)?)?;
                let pkg = read_package(&a.input, &key)?;
                println!("compiler    {}", pkg.header.compiler);
                println!("platform    {:?}", pkg.header.platform);
                pkg.avatar(&ReadLimits::default()).map_err(|e| e.to_string())?
            } else {
                import_file(&a.input)?.avatar
            };
            println!("name        {}", avatar.name);
            println!("{}", stats(&avatar));
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
