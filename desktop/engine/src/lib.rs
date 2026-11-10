//! Roundtable engine. A Rust port of `lib/core.sh` and `lib/mcp.py`.
//!
//! The desktop app ships this as one signed binary. It runs the council,
//! serves the MCP tool for Claude, and owns the keychain items.

pub mod clis;
pub mod council;
pub mod history;
pub mod http;
pub mod keys;
pub mod mcp;
pub mod paths;
pub mod providers;
pub mod settings;
pub mod verify;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
