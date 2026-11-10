//! The credit line: once under `version` and `help`, once at the end of an export,
//! off when the setting says so, and in the MCP tool description.

use roundtable_engine::settings::Settings;
use roundtable_engine::{mcp, with_credit, CREDIT_LINE, EXPORT_CREDIT};
use std::process::Command;

fn engine(args: &[&str]) -> String {
    let home = tempfile::tempdir().unwrap();
    let mut c = Command::new(env!("CARGO_BIN_EXE_roundtable-engine"));
    c.args(args);
    for k in ["HOME", "ROUNDTABLE_DATA_DIR"] {
        c.env(k, home.path());
    }
    let out = c.output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn version_and_help_carry_one_credit_line() {
    let v = engine(&["--version"]);
    let lines: Vec<&str> = v.lines().collect();
    assert!(lines[0].starts_with("roundtable "), "{v}");
    assert_eq!(lines[1], CREDIT_LINE);
    assert!(CREDIT_LINE.contains("https://titaniumcomputing.com/?utm_source=roundtable"));
    let h = engine(&["help"]);
    assert_eq!(h.matches(CREDIT_LINE).count(), 1, "{h}");
    assert_eq!(h.trim_end().lines().last(), Some(CREDIT_LINE));
}

#[test]
fn version_json_is_unchanged() {
    let v: serde_json::Value = serde_json::from_str(engine(&["version", "--json"]).trim()).unwrap();
    assert!(v.get("version").is_some());
}

#[test]
fn export_ends_with_the_line_once_and_can_be_off() {
    let md = "# Round Table\n\nThe verdict.\n";
    let out = with_credit(md, true);
    assert!(out.ends_with(&format!("{EXPORT_CREDIT}\n")));
    assert_eq!(with_credit(&out, true), out);
    assert_eq!(with_credit(md, false), md);
    assert!(Settings::default().export_credit);
    let old: Settings = serde_json::from_str(r#"{"history": true}"#).unwrap();
    assert!(old.export_credit, "settings saved before the switch keep the line on");
}

#[test]
fn mcp_tool_names_titanium() {
    for remote in [false, true] {
        let d = mcp::tool(remote)["description"].as_str().unwrap().to_string();
        assert!(d.contains("free from Titanium Computing (https://titaniumcomputing.com/?utm_source=roundtable&utm_medium=mcp)"), "{d}");
    }
}
