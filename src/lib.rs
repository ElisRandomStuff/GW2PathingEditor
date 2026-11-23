//! Pathing Editor: create and edit GW2 marker packs (TacO / Blish HUD Pathing / TaimiHUD format)
//! in game. A Raidcore Nexus addon.
//!
//! Everything except `addon` is platform-independent and unit-tested on the host with
//! `cargo test`. The Nexus glue (`addon`) only builds for Windows.

// Parts of the core are only used by the Windows-only addon module.
#![cfg_attr(not(windows), allow(dead_code))]

mod attrs;
mod clipboard;
mod editor;
mod files;
mod format;
mod geometry;
mod guid;
mod images;
mod namespace;
mod pack;
mod settings;
mod trail;
mod view;

#[cfg(windows)]
mod addon;
