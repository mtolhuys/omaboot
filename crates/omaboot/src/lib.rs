//! omaboot: design and apply the Plymouth unlock screen, the SDDM login
//! screen, and the Plymouth shutdown screen on Omarchy.
//!
//! The library holds everything the CLI and, later, the TUI do. Nothing in it
//! needs root: the one privileged act is delegated to `omaboot-apply`.
//!
//! The rule the whole design follows: omaboot writes to its own paths only.
//! `/usr/share/plymouth/themes/omarchy` and `/usr/share/sddm/themes/omarchy`
//! are never touched, which is checked in [`paths::guard_destination`] before
//! any operation is performed, dry run included.

pub mod api;
pub mod app;
pub mod apply;
pub mod auth;
pub mod cli;
pub mod error;
pub mod exec;
pub mod generate;
pub mod hash;
pub mod omarchy;
pub mod paths;
pub mod plugin;
pub mod preview;
pub mod render;
pub mod scaffold;
pub mod state;
pub mod system;
pub mod theme;

pub use error::{Error, Result};
