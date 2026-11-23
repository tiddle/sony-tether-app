//! Core types shared by the tether app and every camera backend.
//!
//! This crate knows nothing about Sony's SDK. It defines the boundary: what a
//! backend must do, what it reports back, and what the app is allowed to assume
//! about a given camera body.

pub mod backend;
pub mod capability;
pub mod error;
pub mod session;
pub mod types;

pub use backend::CameraBackend;
pub use capability::{CameraCapabilities, CameraEntry, FeatureSupport};
pub use error::{Result, TetherError};
pub use session::Session;
pub use types::{
    BackendEvent, ButtonState, CameraId, CameraInfo, CapturedImage, FrameRef, LiveViewFrame,
    Transport,
};
