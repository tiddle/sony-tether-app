use thiserror::Error;

/// Everything that can go wrong at the boundary between the app and a camera.
#[derive(Debug, Error)]
pub enum TetherError {
    /// The build did not find Sony's SDK. The stub backend returns this.
    #[error("the Sony Camera Remote SDK is not configured: set SONY_SDK_ROOT and rebuild")]
    SdkNotConfigured,

    /// A code path that exists but is not written yet. Carries where it stopped.
    #[error("not implemented yet: {0}")]
    Unimplemented(&'static str),

    /// A command arrived while no camera was connected.
    #[error("no camera is connected")]
    NotConnected,

    /// No body was found on any transport.
    #[error("no camera found")]
    NotFound,

    /// The camera rejected a command or reported a failure.
    #[error("camera reported: {0}")]
    Camera(String),

    /// The connection itself failed: USB, network, or the session layer.
    #[error("transport error: {0}")]
    Transport(String),

    /// A local configuration or data file is wrong.
    #[error("configuration error: {0}")]
    Config(String),

    /// Writing to the local disk failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, TetherError>;
