//! The Sony Camera Remote SDK backend.
//!
//! Today this crate contains the shape of the backend and nothing else: every
//! camera command returns an error. That is intentional. The real binding lands
//! in milestone M1, when the SDK is available, and it replaces the bodies of the
//! `CameraBackend` methods below.
//!
//! Two rules apply to everything added here:
//!
//! 1. No Sony header, library or documentation file is ever committed. The build
//!    reads `SONY_SDK_ROOT` (see `build.rs`).
//! 2. The C++ interop lives in `native/` behind a C ABI, wrapped by the `cxx`
//!    crate. This file must stay free of `unsafe` that touches the SDK directly.
//! 3. Sony's libraries are found at **runtime**, not linked at build time. Sony's
//!    licence decides whether they ship with the app or the user installs them,
//!    and one binary covers both answers. See `loader` and `docs/sdk-layout.md`.

pub mod loader;

use std::path::Path;
use std::sync::mpsc::Sender;

use camera_core::{BackendEvent, ButtonState, CameraBackend, CapturedImage, Result, TetherError};

/// True when this build found the SDK. False means the stub is compiled in.
pub const SDK_CONFIGURED: bool = cfg!(sony_sdk);

/// The Sony backend. One instance owns at most one camera session.
#[derive(Debug, Default)]
pub struct SdkBackend {
    connected: bool,
}

impl SdkBackend {
    pub fn new() -> Self {
        Self { connected: false }
    }

    fn pending(operation: &'static str) -> TetherError {
        TetherError::Unimplemented(operation)
    }
}

impl CameraBackend for SdkBackend {
    fn name(&self) -> &'static str {
        "sony-camera-remote-sdk"
    }

    fn connect(&mut self, _events: Sender<BackendEvent>) -> Result<()> {
        if !SDK_CONFIGURED {
            return Err(TetherError::SdkNotConfigured);
        }

        // Runtime discovery, never static linkage: the same binary works whether
        // the SDK ships with the app or the user installed it.
        let location =
            loader::SdkLocation::discover().map_err(|err| TetherError::Config(err.to_string()))?;

        // M1: dlopen `location.file(loader::SHIM_FILE)` and start the SDK session
        // through the shim. Until the shim exists, stop with a clear error rather
        // than pretending a camera is there.
        let _ = location;
        Err(Self::pending("SdkBackend::connect (milestone M1)"))
    }

    fn disconnect(&mut self) -> Result<()> {
        self.connected = false;
        Ok(())
    }

    fn shutter(&mut self, _state: ButtonState) -> Result<()> {
        if !self.connected {
            return Err(TetherError::NotConnected);
        }
        Err(Self::pending("SdkBackend::shutter (milestone M2)"))
    }

    fn download(&mut self, _image: &CapturedImage, _dest: &Path) -> Result<()> {
        if !self.connected {
            return Err(TetherError::NotConnected);
        }
        Err(Self::pending("SdkBackend::download (milestone M2)"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_without_a_session_are_refused() {
        let mut backend = SdkBackend::new();
        assert!(matches!(
            backend.shutter(ButtonState::FullPress),
            Err(TetherError::NotConnected)
        ));
        assert!(matches!(
            backend.download(
                &CapturedImage {
                    handle: 1,
                    filename_hint: "x.ARW".into(),
                    size_bytes: 0
                },
                Path::new("x.ARW")
            ),
            Err(TetherError::NotConnected)
        ));
    }

    #[test]
    fn connect_without_the_sdk_explains_itself() {
        if SDK_CONFIGURED {
            return;
        }
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut backend = SdkBackend::new();
        assert!(matches!(
            backend.connect(tx),
            Err(TetherError::SdkNotConfigured)
        ));
    }
}
