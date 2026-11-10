//! Roundtable engine. A Rust port of `lib/core.sh` and `lib/mcp.py`.
//!
//! The desktop app ships this as one signed binary. It runs the council,
//! serves the MCP tool for Claude, and owns the keychain items.

pub mod attach;
pub mod clis;
pub mod coach;
pub mod council;
pub mod custom;
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
/// The one credit line the CLI prints under `version` and `help`.
pub const CREDIT_LINE: &str = "Free from Titanium Computing. Need AI in your business? https://titaniumcomputing.com/?utm_source=roundtable&utm_medium=cli";

/// The one line a Markdown export ends with (the app's `EXPORT_CREDIT`, ui/src/credits.ts).
pub const EXPORT_CREDIT: &str = "Made with Roundtable (https://roundtable.sh), free from Titanium Computing (https://titaniumcomputing.com/?utm_source=roundtable&utm_medium=export)";

/// A transcript as exported: the text, then the credit line, once, unless it is turned off.
pub fn with_credit(markdown: &str, on: bool) -> String {
    if !on || markdown.contains(EXPORT_CREDIT) {
        return markdown.to_string();
    }
    format!("{}\n\n---\n\n{EXPORT_CREDIT}\n", markdown.trim_end())
}
