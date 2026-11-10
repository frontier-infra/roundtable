//! Reads the Claude Code plugin names from `.claude-plugin/marketplace.json`,
//! so the two lines on screen 4 never drift from the repo.

fn main() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.claude-plugin/marketplace.json");
    println!("cargo:rerun-if-changed={}", manifest.display());
    let (market, plugin) = std::fs::read_to_string(&manifest)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .map(|v| {
            let m = v["name"].as_str().unwrap_or("frontier-infra").to_string();
            let p = v["plugins"][0]["name"].as_str().unwrap_or("roundtable").to_string();
            (m, p)
        })
        .unwrap_or_else(|| ("frontier-infra".into(), "roundtable".into()));
    println!("cargo:rustc-env=RT_MARKETPLACE_NAME={market}");
    println!("cargo:rustc-env=RT_PLUGIN_NAME={plugin}");
    tauri_build::build()
}
