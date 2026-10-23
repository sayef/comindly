//! comindly — deterministic, cross-repo code intelligence for coding agents. This crate is the
//! `comindly` command; its library re-exports the server and the engine, and adds the command
//! line's own parts.

pub use comindly_server::*;

#[cfg(target_os = "macos")]
pub mod app;
pub mod client;
pub mod tui;
