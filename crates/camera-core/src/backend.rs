use std::path::Path;
use std::sync::mpsc::Sender;

use crate::error::Result;
use crate::types::{BackendEvent, ButtonState, CapturedImage};

/// One camera control API, seen from the app.
///
/// Every method is synchronous and every event goes back through the `Sender`
/// given to [`CameraBackend::connect`]. The implementations push work onto their
/// own threads; the app must never block its UI thread on this trait.
pub trait CameraBackend: Send {
    /// Stable name for logs, for example `sony-camera-remote-sdk`.
    fn name(&self) -> &'static str;

    /// Find a body and open a session. On success the backend must emit
    /// [`BackendEvent::Connected`] once, and [`BackendEvent::Disconnected`] when
    /// the session ends for any reason.
    fn connect(&mut self, events: Sender<BackendEvent>) -> Result<()>;

    /// Close the session. Safe to call when not connected.
    fn disconnect(&mut self) -> Result<()>;

    /// Move the shutter button. Backends must accept `Released` even when idle.
    fn shutter(&mut self, state: ButtonState) -> Result<()>;

    /// Copy one capture from the camera to `dest`, emitting progress events.
    /// `dest` is a full file path chosen by the caller.
    fn download(&mut self, image: &CapturedImage, dest: &Path) -> Result<()>;
}
