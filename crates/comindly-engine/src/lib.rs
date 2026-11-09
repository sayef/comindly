//! comindly as a server: the web app and its `/api/v1`, MCP for agents, indexing jobs, members and
//! roles, and the backends that keep them, plus this machine's preferences and credentials.
//! Re-exports the engine, so `comindly_engine::index` is `comindly_core::index`.

pub use comindly_core::*;

pub use comindly_api::agents;
#[cfg(target_os = "macos")]
pub mod app;
pub use comindly_api as api;
pub use comindly_api::client;
pub mod backends;
pub mod cache;
pub mod config;
pub mod credentials;
pub mod doctor;
pub mod indexer;
pub mod jobs;
pub mod mcp;
pub mod oidc;
pub mod run;
pub mod server;
pub mod telemetry;
pub mod ui;
pub mod web;
