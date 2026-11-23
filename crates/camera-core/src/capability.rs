use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{Result, TetherError};

/// What we know about one body's support for one feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureSupport {
    /// Tested on this body, on the firmware recorded in the same row.
    Verified,
    /// Believed to work, not yet tested here. The UI must not rely on it.
    Unverified,
    /// The body or the SDK does not offer this.
    Unsupported,
    /// Outside the current scope. Says nothing about the hardware.
    OutOfScope,
}

impl FeatureSupport {
    /// Only verified features may enable a control in the UI.
    pub fn allows_ui(self) -> bool {
        matches!(self, Self::Verified)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Unverified => "unverified",
            Self::Unsupported => "unsupported",
            Self::OutOfScope => "out of scope",
        }
    }
}

/// One row of the capability matrix.
#[derive(Clone, Debug, Deserialize)]
pub struct CameraEntry {
    /// The identifier the camera reports, for example `ILCE-7M4`.
    pub id: String,
    /// Human name, for example `A7 IV`.
    pub name: String,
    /// Which backend drives this body.
    pub backend: String,
    pub transports: Vec<String>,
    /// Firmware versions the row's `verified` claims were tested on.
    #[serde(default)]
    pub tested_firmware: Vec<String>,
    #[serde(default)]
    pub features: HashMap<String, FeatureSupport>,
}

#[derive(Deserialize)]
struct MatrixFile {
    schema_version: u32,
    cameras: Vec<CameraEntry>,
}

const EXPECTED_SCHEMA_VERSION: u32 = 1;

/// The embedded copy of `data/cameras.json`.
const EMBEDDED_MATRIX: &str = include_str!("../data/cameras.json");

/// The whole matrix, loaded from the file embedded at build time.
#[derive(Clone, Debug)]
pub struct CameraCapabilities {
    entries: Vec<CameraEntry>,
}

impl CameraCapabilities {
    /// An empty matrix. Every lookup then reports `Unverified`, which is the safe
    /// answer. Used when the embedded matrix fails to load.
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn embedded() -> Result<Self> {
        let file: MatrixFile = serde_json::from_str(EMBEDDED_MATRIX).map_err(|e| {
            TetherError::Config(format!("capability matrix is not valid JSON: {e}"))
        })?;

        if file.schema_version != EXPECTED_SCHEMA_VERSION {
            return Err(TetherError::Config(format!(
                "capability matrix schema is version {}, this build expects {}",
                file.schema_version, EXPECTED_SCHEMA_VERSION
            )));
        }

        Ok(Self {
            entries: file.cameras,
        })
    }

    pub fn all(&self) -> &[CameraEntry] {
        &self.entries
    }

    /// Case-insensitive lookup, because firmware versions differ in case.
    pub fn find(&self, camera_id: &str) -> Option<&CameraEntry> {
        self.entries
            .iter()
            .find(|entry| entry.id.eq_ignore_ascii_case(camera_id))
    }

    /// Unknown bodies and unknown features report `Unverified`, never a guess at
    /// support. This is the only sanctioned way to ask what a body can do.
    pub fn support(&self, camera_id: &str, feature: &str) -> FeatureSupport {
        self.find(camera_id)
            .and_then(|entry| entry.features.get(feature).copied())
            .unwrap_or(FeatureSupport::Unverified)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_matrix_parses() {
        let caps = CameraCapabilities::embedded().expect("embedded matrix must parse");
        assert!(!caps.all().is_empty(), "matrix must list at least one body");
    }

    #[test]
    fn known_body_is_found_case_insensitively() {
        let caps = CameraCapabilities::embedded().unwrap();
        assert!(caps.find("ilce-7m4").is_some());
    }

    #[test]
    fn unknown_body_is_never_reported_as_supported() {
        let caps = CameraCapabilities::embedded().unwrap();
        assert_eq!(
            caps.support("ILCE-9999", "live_view"),
            FeatureSupport::Unverified
        );
    }

    #[test]
    fn unknown_feature_is_never_reported_as_supported() {
        let caps = CameraCapabilities::embedded().unwrap();
        assert_eq!(
            caps.support("ILCE-7M4", "teleport"),
            FeatureSupport::Unverified
        );
    }
}
