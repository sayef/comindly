//! comindly as a server: the web app and its `/api/v1`, MCP for agents, indexing jobs, members and
//! roles, and the backends that keep them, plus this machine's preferences and credentials.
//! Re-exports the engine, so `comindly_server::index` is `comindly_core::index`.

pub use comindly_core::*;

pub mod backends;
pub mod cache;
pub mod config;
pub mod credentials;
pub mod doctor;
pub mod jobs;
pub mod mcp;
pub mod server;
pub mod telemetry;
pub mod ui;
pub mod web;
