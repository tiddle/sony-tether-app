use std::sync::Arc;

/// The body's own identifier, as the camera reports it. For the A7 IV this is
/// `ILCE-7M4`. The capability matrix is keyed on this, never on marketing names.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CameraId(String);

impl CameraId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CameraId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// How we are talking to the camera.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    Usb,
    Network,
}

/// What a connected body told us about itself.
#[derive(Clone, Debug)]
pub struct CameraInfo {
    pub id: CameraId,
    pub model: String,
    pub firmware: Option<String>,
    pub transport: Transport,
}

/// Shutter button position. A real camera distinguishes these, and so does the
/// SDK: half press locks focus and exposure, full press releases the shutter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonState {
    Released,
    HalfPress,
    FullPress,
}

/// One live-view frame, still compressed exactly as the SDK delivered it.
///
/// Decoding is the UI's job and happens off the UI thread. The frame is behind
/// an `Arc` because the same frame is read by the renderer while the next one
/// arrives from the camera.
#[derive(Debug)]
pub struct LiveViewFrame {
    pub width: u32,
    pub height: u32,
    pub jpeg: Vec<u8>,
}

pub type FrameRef = Arc<LiveViewFrame>;

/// A capture that exists on the camera's card and can be downloaded.
///
/// `handle` is opaque to the app. It is whatever the backend needs in order to
/// find the image again later.
#[derive(Clone, Debug)]
pub struct CapturedImage {
    pub handle: u64,
    pub filename_hint: String,
    pub size_bytes: u64,
}

/// Everything a backend reports back to the app.
///
/// All variants are plain data on purpose: if the SDK has to move into its own
/// process, this enum becomes a wire format and nothing else changes.
#[derive(Debug)]
pub enum BackendEvent {
    Connected(CameraInfo),
    Disconnected,
    Frame(FrameRef),
    Captured(CapturedImage),
    DownloadProgress {
        handle: u64,
        received: u64,
        total: u64,
    },
    Log(String),
    Error(String),
}
