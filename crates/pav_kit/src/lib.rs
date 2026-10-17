//! The gameplay kit: systems a game composes inside its `Game` hook. Their state lives in the
//! game's struct (so snapshots, rewind and replays cover it); everything here is deterministic
//! (no clock, seeded randomness only, ordered maps) and runs in the browser.
//!
//! Nothing here names a particular game: vessels, objectives, archetypes and playlists are
//! data the game loads.

pub mod camera;
pub mod demo;
