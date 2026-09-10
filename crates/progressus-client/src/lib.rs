#[cfg(test)]
mod ambient_playback;
// Retained for standalone audio auditions and their regression tests.
#[allow(dead_code)]
mod ambient_synthesis;
mod audio;
mod audio_observer;
mod audio_synthesis;
mod client_diagnostics;
mod i18n;
mod interaction;
mod inventory;
mod modal;
mod navigation;
pub mod presentation;
mod procedural_assets;
mod render;
mod runtime;
mod save_slots;
mod tile_connectivity;
mod ui;
mod ui_font;

pub use runtime::{ClientError, run, run_with_options, run_with_seed};

mod low_poly;

pub(crate) mod continuous_music;
// This source also contains the offline timbre comparison/export entry points.
#[allow(dead_code)]
mod score_audition;
