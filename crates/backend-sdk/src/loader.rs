//! Finding the Sony SDK at runtime.
//!
//! Sony's licence decides whether we may ship their libraries or whether the
//! user installs them. Loading at runtime, instead of linking at build time,
//! keeps both answers open: the same binary works either way and tells the user
//! exactly what is missing and where it looked.
//!
//! Nothing here names a Sony file. The directory carries a `manifest.json`
//! listing the files that must be present. Vendor filenames are recorded there
//! in milestone M1, from the real SDK, and never hard-coded here.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Environment variable that points straight at the SDK directory.
pub const SDK_DIR_ENV: &str = "TETHER_SDK_DIR";

/// Manifest file that must sit in the SDK directory.
pub const MANIFEST_NAME: &str = "manifest.json";

/// The shim we build ourselves. Sony's libraries sit beside it.
pub const SHIM_FILE: &str = if cfg!(windows) {
    "tether_sdk_shim.dll"
} else if cfg!(target_os = "macos") {
    "libtether_sdk_shim.dylib"
} else {
    "libtether_sdk_shim.so"
};

/// The schema version this build understands.
pub const EXPECTED_SCHEMA: u32 = 1;

/// Contents of `manifest.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    /// Files that must exist in the same directory as this manifest.
    pub files: Vec<String>,
    /// Free text for whoever maintains the deployment.
    #[serde(default)]
    pub notes: Option<String>,
}

/// A validated SDK directory: manifest parsed, every listed file present.
#[derive(Debug, Clone)]
pub struct SdkLocation {
    dir: PathBuf,
    manifest: Manifest,
}

/// Why an SDK directory was rejected. Every variant says what to fix.
#[derive(Debug, thiserror::Error)]
pub enum SdkLoadError {
    #[error("Sony SDK not found; looked in: {}", join_paths(.tried))]
    NotFound { tried: Vec<PathBuf> },

    #[error("SDK directory {} does not exist", .0.display())]
    NotADirectory(PathBuf),

    #[error("cannot read {}: {source}", .path.display())]
    ManifestUnreadable {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("invalid manifest {}: {message}", .path.display())]
    ManifestInvalid { path: PathBuf, message: String },

    #[error(
        "manifest schema is version {}, this build understands {EXPECTED_SCHEMA}",
        .found
    )]
    SchemaMismatch { found: u32 },

    #[error(
        "{} is missing required files: {}",
        .dir.display(),
        .missing.join(", ")
    )]
    MissingFiles { dir: PathBuf, missing: Vec<String> },
}

impl SdkLocation {
    /// Look for a usable SDK directory.
    ///
    /// Order:
    /// 1. `TETHER_SDK_DIR`, if set.
    /// 2. `sdk/` beside the executable, which is what an installer creates.
    /// 3. `sdk/` in the current working directory, for development.
    ///
    /// The first path that exists wins. If it is present but wrong, its error is
    /// returned rather than hidden behind a later candidate: a bad directory
    /// near the executable is the common real failure, and silence makes it hard
    /// to diagnose.
    pub fn discover() -> Result<Self, SdkLoadError> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        let mut tried: Vec<PathBuf> = Vec::new();

        if let Some(dir) = std::env::var_os(SDK_DIR_ENV) {
            if !dir.is_empty() {
                candidates.push(PathBuf::from(dir));
            }
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                candidates.push(parent.join("sdk"));
            }
        }
        candidates.push(PathBuf::from("sdk"));

        for candidate in candidates {
            if candidate.is_dir() {
                return Self::from_dir(candidate);
            }
            tried.push(candidate);
        }

        Err(SdkLoadError::NotFound { tried })
    }

    /// Validate one directory: read its manifest and check every listed file.
    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Self, SdkLoadError> {
        let dir = dir.as_ref().to_path_buf();

        if !dir.is_dir() {
            return Err(SdkLoadError::NotADirectory(dir));
        }

        let manifest_path = dir.join(MANIFEST_NAME);
        let text = std::fs::read_to_string(&manifest_path).map_err(|source| {
            SdkLoadError::ManifestUnreadable {
                path: manifest_path.clone(),
                source,
            }
        })?;

        let manifest: Manifest =
            serde_json::from_str(&text).map_err(|err| SdkLoadError::ManifestInvalid {
                path: manifest_path.clone(),
                message: err.to_string(),
            })?;

        if manifest.schema_version != EXPECTED_SCHEMA {
            return Err(SdkLoadError::SchemaMismatch {
                found: manifest.schema_version,
            });
        }

        for name in &manifest.files {
            if !is_plain_filename(name) {
                return Err(SdkLoadError::ManifestInvalid {
                    path: manifest_path,
                    message: format!("file entry {name:?} must be a plain filename"),
                });
            }
        }

        let missing: Vec<String> = manifest
            .files
            .iter()
            .filter(|name| !dir.join(name.as_str()).is_file())
            .cloned()
            .collect();

        if !missing.is_empty() {
            return Err(SdkLoadError::MissingFiles { dir, missing });
        }

        Ok(Self { dir, manifest })
    }

    /// The validated directory. Paths to load come from here, never from the UI.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Full path of one file listed in the manifest.
    pub fn file(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
}

/// A manifest entry must be a bare filename. No separators, no drive letters, no
/// `..`. A manifest is data from outside the program and is treated as hostile.
fn is_plain_filename(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['/', '\\'])
        && name != "."
        && name != ".."
        && !name.contains(':')
}

fn join_paths(paths: &[PathBuf]) -> String {
    if paths.is_empty() {
        return "(no candidates)".to_owned();
    }
    paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_manifest(dir: &Path, body: &str) {
        fs::write(dir.join(MANIFEST_NAME), body).unwrap();
    }

    #[test]
    fn accepts_a_complete_layout() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(SHIM_FILE), b"not really a library").unwrap();
        write_manifest(
            tmp.path(),
            &format!(r#"{{"schema_version":1,"files":["{SHIM_FILE}"],"notes":"test"}}"#),
        );

        let location = SdkLocation::from_dir(tmp.path()).unwrap();
        assert_eq!(location.dir(), tmp.path());
        assert_eq!(location.manifest().files, vec![SHIM_FILE.to_owned()]);
        assert!(location.file(SHIM_FILE).is_file());
    }

    #[test]
    fn reports_every_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(
            tmp.path(),
            r#"{"schema_version":1,"files":["a.dll","b.dll"]}"#,
        );

        match SdkLocation::from_dir(tmp.path()) {
            Err(SdkLoadError::MissingFiles { missing, .. }) => {
                assert_eq!(missing, vec!["a.dll".to_owned(), "b.dll".to_owned()]);
            }
            other => panic!("expected MissingFiles, got {other:?}"),
        }
    }

    #[test]
    fn rejects_a_missing_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let absent = tmp.path().join("not-here");
        assert!(matches!(
            SdkLocation::from_dir(&absent),
            Err(SdkLoadError::NotADirectory(_))
        ));
    }

    #[test]
    fn rejects_a_missing_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(
            SdkLocation::from_dir(tmp.path()),
            Err(SdkLoadError::ManifestUnreadable { .. })
        ));
    }

    #[test]
    fn rejects_broken_json() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(tmp.path(), "{ not json");
        assert!(matches!(
            SdkLocation::from_dir(tmp.path()),
            Err(SdkLoadError::ManifestInvalid { .. })
        ));
    }

    #[test]
    fn rejects_a_future_schema() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(tmp.path(), r#"{"schema_version":99,"files":[]}"#);
        assert!(matches!(
            SdkLocation::from_dir(tmp.path()),
            Err(SdkLoadError::SchemaMismatch { found: 99 })
        ));
    }

    #[test]
    fn rejects_paths_that_escape_the_directory() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(
            tmp.path(),
            r#"{"schema_version":1,"files":["../../../etc/passwd"]}"#,
        );
        assert!(matches!(
            SdkLocation::from_dir(tmp.path()),
            Err(SdkLoadError::ManifestInvalid { .. })
        ));
    }

    #[test]
    fn plain_filenames_are_recognised() {
        assert!(is_plain_filename("tether_sdk_shim.dll"));
        assert!(is_plain_filename("libtether_sdk_shim.so"));
        assert!(!is_plain_filename("../x.dll"));
        assert!(!is_plain_filename(r"C:\x.dll"));
        assert!(!is_plain_filename("a/b"));
        assert!(!is_plain_filename(""));
    }
}
