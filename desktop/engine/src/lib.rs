//! Roundtable engine. A Rust port of `lib/core.sh` and `lib/mcp.py`.
//!
//! The desktop app ships this as one signed binary. It runs the council,
//! serves the MCP tool for Claude, and owns the keychain items.

pub mod attach;
pub mod clis;
pub mod coach;
pub mod council;
pub mod decision;
pub mod history;
pub mod http;
pub mod keys;
pub mod mcp;
pub mod paths;
pub mod personas;
pub mod providers;
pub mod settings;
pub mod verdict;
pub mod verify;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Tests that set or read provider URLs in the process environment take this lock.
#[cfg(test)]
pub(crate) static TEST_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());
