//! Turns simulation frames into render scenes: camera rig, interpolation, style settings.

pub mod arpg;
pub mod assets;
pub mod atmos;
pub mod build;
pub mod camera;
pub mod creatures;
pub mod cue_fx;
pub mod fx;
pub mod look;
pub mod materials;
pub mod play_view;
pub mod vehicles;
pub mod water;

pub use build::{ViewBuilder, ViewSettings};
pub use camera::{CameraParams, CameraRig};
