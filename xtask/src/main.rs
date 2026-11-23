//! Repository tasks. Run them as `cargo xtask <command>`.
//!
//! The only command today is `release`, which builds and packages a distributable
//! zip. Packaging lives here, in Rust, rather than in a PowerShell script, so it
//! behaves the same on every machine and can be tested without Windows.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

const APP_NAME: &str = "tether-app";
const DIST: &str = "dist";
const STUB_WARNING: &str = "THIS BUILD HAS NO SONY SDK AND CANNOT TETHER A CAMERA";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let outcome = match args.first().map(String::as_str) {
        Some("release") => release(&args[1..]),
        Some("help") | Some("-h") | Some("--help") | None => {
            usage();
            Ok(())
        }
        Some(other) => Err(format!("unknown command {other:?}; try `cargo xtask help`")),
    };

    if let Err(message) = outcome {
        eprintln!("\nerror: {message}\n");
        std::process::exit(1);
    }
}

fn usage() {
    println!(
        "\
cargo xtask <command>

Commands
  release              Build and package a release zip into {DIST}/
  help                 Show this text

Options for `release`
  --bundle-sdk <dir>   Copy the runtime SDK directory into the package as sdk/.
                       The directory must contain a manifest.json; see
                       docs/sdk-layout.md.
  --allow-stub         Allow packaging a build made without the SDK. The artifact
                       name then ends in -stub and its BUILD-INFO.txt says so.
  --no-build           Package the binary already in target/release.

Environment
  SONY_SDK_ROOT        Where the Sony Camera Remote SDK lives at build time.
                       Without it the backend compiles as a stub.

Exit codes
  0  packaged
  1  nothing packaged; the message says what to fix"
    );
}

#[derive(Default)]
struct ReleaseOptions {
    allow_stub: bool,
    no_build: bool,
    bundle_sdk: Option<PathBuf>,
}

fn release(args: &[String]) -> Result<(), String> {
    let options = parse_release_options(args)?;

    let version = env!("CARGO_PKG_VERSION");
    let sdk_at_build_time = env::var("SONY_SDK_ROOT")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);

    if !sdk_at_build_time && !options.allow_stub {
        return Err(format!(
            "SONY_SDK_ROOT is not set, so this build could not tether a camera.\n\n\
             Set SONY_SDK_ROOT to the extracted Sony Camera Remote SDK to build a working \
             release, or pass --allow-stub to package a build that is named as unable to.\n\
             See docs/sdk-layout.md for the three distribution modes."
        ));
    }

    if !options.no_build {
        build_release()?;
    }

    let binary = release_binary()?;
    let target = format!("{}-{}", env::consts::OS, env::consts::ARCH);
    let suffix = if sdk_at_build_time { "" } else { "-stub" };
    let staging_name = format!("{APP_NAME}-{version}-{target}{suffix}");

    let staging = Path::new(DIST).join(&staging_name);
    reset_dir(&staging)?;

    fs::copy(&binary, staging.join(binary.file_name().unwrap()))
        .map_err(|err| format!("cannot copy {}: {err}", binary.display()))?;

    for doc in ["LICENSE", "README.md", "docs/sdk-layout.md"] {
        copy_into(&staging, Path::new(doc))?;
    }
    if let Some(dir) = &options.bundle_sdk {
        bundle_sdk(dir, &staging)?;
    }

    let info = build_info(
        version,
        &target,
        sdk_at_build_time,
        options.bundle_sdk.is_some(),
    );
    fs::write(staging.join("BUILD-INFO.txt"), info)
        .map_err(|err| format!("cannot write BUILD-INFO.txt: {err}"))?;

    let zip_path = Path::new(DIST).join(format!("{staging_name}.zip"));
    let files = collect_files(&staging)?;
    write_zip(&zip_path, &files)?;
    remove_dir(&staging)?;

    let digest = sha256_of(&zip_path)?;
    let bytes = fs::metadata(&zip_path).map(|meta| meta.len()).unwrap_or(0);

    println!("\npackaged {}", zip_path.display());
    println!("  size    {:.1} MiB", bytes as f64 / 1_048_576.0);
    println!("  sha256  {digest}");
    println!(
        "  tether  {}",
        if sdk_at_build_time {
            "yes, if the runtime SDK is present"
        } else {
            "NO - stub build"
        }
    );
    println!("  note    see BUILD-INFO.txt inside the zip\n");

    if !sdk_at_build_time {
        println!("! {STUB_WARNING}.");
        println!("! The artifact name ends in -stub and its BUILD-INFO.txt says the same.");
        println!("! Do not describe it as a working release.\n");
    }

    Ok(())
}

fn parse_release_options(args: &[String]) -> Result<ReleaseOptions, String> {
    let mut options = ReleaseOptions::default();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--allow-stub" => options.allow_stub = true,
            "--no-build" => options.no_build = true,
            "--bundle-sdk" => {
                index += 1;
                let dir = args
                    .get(index)
                    .ok_or("--bundle-sdk needs a directory path after it")?;
                options.bundle_sdk = Some(PathBuf::from(dir));
            }
            other => return Err(format!("unknown option {other:?}; try `cargo xtask help`")),
        }
        index += 1;
    }

    Ok(options)
}

fn build_release() -> Result<(), String> {
    println!("building {APP_NAME} in release mode...");
    let status = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned()))
        .args(["build", "--release", "--locked", "-p", APP_NAME])
        .status()
        .map_err(|err| format!("cannot run cargo: {err}"))?;

    if !status.success() {
        return Err("cargo build failed; nothing was packaged".to_owned());
    }
    Ok(())
}

fn release_binary() -> Result<PathBuf, String> {
    let name = format!("{APP_NAME}{}", env::consts::EXE_SUFFIX);
    let path = Path::new("target").join("release").join(name);
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "{} does not exist; build it first or drop --no-build",
            path.display()
        ))
    }
}

/// Copy a repository file into the package, keeping its relative path so links
/// inside the bundled README still resolve.
fn copy_into(staging: &Path, source: &Path) -> Result<(), String> {
    let destination = staging.join(source);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("cannot create {}: {err}", parent.display()))?;
    }
    fs::copy(source, &destination)
        .map(|_| ())
        .map_err(|err| format!("cannot copy {}: {err}", source.display()))
}

fn bundle_sdk(dir: &Path, staging: &Path) -> Result<(), String> {
    if !dir.is_dir() {
        return Err(format!("{} is not a directory", dir.display()));
    }
    if !dir.join("manifest.json").is_file() {
        return Err(format!(
            "{} has no manifest.json; the packaged sdk/ directory must describe its contents \
             (see docs/sdk-layout.md)",
            dir.display()
        ));
    }

    let destination = staging.join("sdk");
    copy_dir(dir, &destination)?;
    println!("bundled runtime SDK from {}", dir.display());
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|err| format!("cannot create {}: {err}", to.display()))?;
    for entry in
        fs::read_dir(from).map_err(|err| format!("cannot read {}: {err}", from.display()))?
    {
        let entry = entry.map_err(|err| format!("cannot read {}: {err}", from.display()))?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir(&source, &destination)?;
        } else {
            fs::copy(&source, &destination)
                .map_err(|err| format!("cannot copy {}: {err}", source.display()))?;
        }
    }
    Ok(())
}

fn build_info(version: &str, target: &str, sdk: bool, bundled: bool) -> String {
    let mut info = String::new();
    let _ = writeln!(info, "{APP_NAME} {version}");
    let _ = writeln!(info, "platform: {target}");
    let _ = writeln!(info, "commit: {}", git_commit());
    let _ = writeln!(info, "built unix time: {}", unix_now());
    let _ = writeln!(
        info,
        "sony sdk compiled in: {}",
        if sdk { "yes" } else { "no" }
    );
    let _ = writeln!(
        info,
        "sony sdk bundled: {}",
        if bundled { "yes" } else { "no" }
    );
    let _ = writeln!(info);

    if sdk {
        let _ = writeln!(info, "This build can tether a camera.");
        if !bundled {
            let _ = writeln!(
                info,
                "It needs the Sony SDK at runtime. Set TETHER_SDK_DIR, or place it in an sdk/\n\
                 directory next to the executable. See docs/sdk-layout.md."
            );
        }
    } else {
        let _ = writeln!(info, "{STUB_WARNING}.");
        let _ = writeln!(
            info,
            "It was compiled without SONY_SDK_ROOT. Every camera command fails with\n\
             'the Sony Camera Remote SDK is not configured'. Useful for reviewing the\n\
             interface and for testing packaging. Not useful for taking photographs."
        );
    }

    info
}

fn git_commit() -> String {
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn reset_dir(path: &Path) -> Result<(), String> {
    remove_dir(path)?;
    fs::create_dir_all(path).map_err(|err| format!("cannot create {}: {err}", path.display()))
}

fn remove_dir(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_dir_all(path)
            .map_err(|err| format!("cannot clear {}: {err}", path.display()))?;
    }
    Ok(())
}

/// Every file under `root`, as paths relative to it, sorted for a stable zip.
fn collect_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries =
            fs::read_dir(&dir).map_err(|err| format!("cannot read {}: {err}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|err| format!("cannot read {}: {err}", dir.display()))?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }

    let mut relative: Vec<PathBuf> = files
        .into_iter()
        .filter_map(|path| path.strip_prefix(root).ok().map(Path::to_path_buf))
        .collect();
    relative.sort();
    Ok(relative)
}

fn write_zip(zip_path: &Path, files: &[PathBuf]) -> Result<(), String> {
    let handle = fs::File::create(zip_path)
        .map_err(|err| format!("cannot create {}: {err}", zip_path.display()))?;
    let mut writer = zip::ZipWriter::new(handle);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    // The staging directory name becomes the zip's root, so unzipping makes one
    // folder instead of scattering files into the current directory.
    let root = zip_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| APP_NAME.to_owned());

    for file in files {
        let name = format!("{root}/{}", file.to_string_lossy().replace('\\', "/"));
        writer
            .start_file(name.clone(), options)
            .map_err(|err| format!("cannot add {name}: {err}"))?;

        let mut source =
            fs::File::open(file).map_err(|err| format!("cannot open {}: {err}", file.display()))?;
        std::io::copy(&mut source, &mut writer)
            .map_err(|err| format!("cannot write {name}: {err}"))?;
    }

    writer
        .finish()
        .map_err(|err| format!("cannot finish {}: {err}", zip_path.display()))?;
    Ok(())
}

fn sha256_of(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|err| format!("cannot read {}: {err}", path.display()))?;
    let digest = Sha256::digest(&bytes);
    let mut text = String::with_capacity(64);
    for byte in digest {
        let _ = write!(text, "{byte:02x}");
    }
    Ok(text)
}
