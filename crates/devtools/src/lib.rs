//! Stitchcraft's dev tools: everything between "the canvas validates" and
//! "the mod is running in a game", without leaving the editor.
//!
//! * [`workspace`] keeps a private Gradle project per mod, exported from the
//!   canvas on every run, so building and running never need an Export.
//! * [`java`] finds a JDK to start Gradle with, or downloads one.
//! * [`process`] runs Gradle and streams what it prints.
//! * [`debugger`] is the editor's end of the protocol the generated `Dbg`
//!   class speaks (see `stitchcraft_export::runtime::dbg`): which block is
//!   running, breakpoints, stepping, the canvas's variables, and failures
//!   reported against the block they happened in.
//! * [`DevTools`] ties them together for the app, which polls it.
//!
//! None of it touches Qt, so all of it is tested here.

pub mod debugger;
pub mod java;
pub mod process;
pub mod session;
pub mod workspace;

pub use session::{Action, DevTools, Event, Loader};

use std::path::PathBuf;

/// Where Stitchcraft keeps what it downloads and builds: workspaces and a
/// JDK. `STITCHCRAFT_DATA` overrides it.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("STITCHCRAFT_DATA") {
        return PathBuf::from(dir);
    }
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = home().map(|h| h.join("Library/Application Support"));
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home().map(|h| h.join(".local/share")));
    #[cfg(windows)]
    let _ = home;
    base.unwrap_or_else(std::env::temp_dir).join("stitchcraft")
}
