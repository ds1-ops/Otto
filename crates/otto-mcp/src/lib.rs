//! Otto's MCP server. Every `otto_engine::Command` is published as an MCP tool, so
//! Claude can do everything a user can do in the app.
//!
//! Backends:
//! - [`LocalBackend`]: a headless engine in this process (`otto mcp` with no app running).
//! - [`ChannelBackend`]: forwards to the running GUI, which applies the command live.
//! - [`RemoteBackend`]: forwards stdio calls to a running GUI's HTTP endpoint (`otto mcp` bridge).

mod backend;
mod discovery;
mod server;
mod transport;

pub use backend::{Backend, ChannelBackend, HybridBackend, LocalBackend, McpRequest, RemoteBackend, SWITCHED_OFF};
pub use discovery::{discovery_path, Discovery};
pub use server::{outcome_to_result, tools, OttoServer, INSTRUCTIONS};
pub use transport::{run_stdio, serve_http, HttpHandle, DEFAULT_PORT};
