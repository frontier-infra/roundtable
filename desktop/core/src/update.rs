//! Updates (spec section 9). The app reads a signed manifest on GitHub Releases:
//! `desktop-latest` for Stable, `desktop-test` for Test builds. It checks at launch and
//! every 6 hours (the UI keeps the clock). With "Install updates automatically" on it
//! downloads in the background and installs on the next quit. **Update now** installs
//! and relaunches at once. Every package is checked against the updater key in
//! `tauri.conf.json` before it is installed.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_updater::{Update, UpdaterExt};

pub const STABLE_URL: &str = "https://github.com/frontier-infra/roundtable/releases/download/desktop-latest/latest.json";
pub const TEST_URL: &str = "https://github.com/frontier-infra/roundtable/releases/download/desktop-test/latest.json";

/// The manifest for a channel. Anything but "test" is Stable.
pub fn endpoint(channel: &str) -> &'static str {
    if channel == "test" {
        TEST_URL
    } else {
        STABLE_URL
    }
}

#[derive(Default)]
pub struct Updates {
    found: Mutex<Option<Update>>,
    /// The downloaded, verified package and the version it is.
    bytes: Mutex<Option<(String, Vec<u8>)>>,
    on_quit: AtomicBool,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    /// That version's CHANGELOG.md section, from the manifest.
    pub notes: String,
    pub date: Option<String>,
    pub channel: String,
    /// Downloaded and ready to install.
    pub ready: bool,
}

fn info(u: &Update, channel: &str, ready: bool) -> UpdateInfo {
    UpdateInfo {
        version: u.version.clone(),
        current: u.current_version.clone(),
        notes: u.body.clone().unwrap_or_default(),
        date: u.raw_json.get("pub_date").and_then(|v| v.as_str()).map(String::from),
        channel: channel.to_string(),
        ready,
    }
}

fn ready_for(state: &Updates, version: &str) -> bool {
    state.bytes.lock().unwrap().as_ref().is_some_and(|(v, _)| v == version)
}

/// Is there a newer version on this channel? None when this is the newest.
#[tauri::command]
pub async fn update_check<R: Runtime>(app: AppHandle<R>, state: State<'_, Updates>, channel: String) -> Result<Option<UpdateInfo>, String> {
    let url = endpoint(&channel).parse().map_err(|e| format!("{e}"))?;
    let updater = app.updater_builder().endpoints(vec![url]).map_err(|e| e.to_string())?.build().map_err(|e| e.to_string())?;
    let found = updater.check().await.map_err(|e| e.to_string())?;
    let out = found.as_ref().map(|u| info(u, &channel, ready_for(&state, &u.version)));
    if found.as_ref().map(|u| &u.version) != state.bytes.lock().unwrap().as_ref().map(|(v, _)| v) {
        // A switch back to Stable, or a newer build: the old download is no longer the one.
        *state.bytes.lock().unwrap() = None;
        state.on_quit.store(false, Ordering::SeqCst);
    }
    *state.found.lock().unwrap() = found;
    Ok(out)
}

async fn download(state: &Updates) -> Result<(Update, Vec<u8>), String> {
    let u = state.found.lock().unwrap().clone().ok_or("No update to download. Check for updates first.")?;
    if let Some((v, b)) = state.bytes.lock().unwrap().as_ref() {
        if *v == u.version {
            return Ok((u, b.clone()));
        }
    }
    // `download` checks the signature before it returns the bytes.
    let bytes = u.download(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    *state.bytes.lock().unwrap() = Some((u.version.clone(), bytes.clone()));
    Ok((u, bytes))
}

/// Download the update now. With `install_on_quit` it is installed when Roundtable quits.
#[tauri::command]
pub async fn update_download(state: State<'_, Updates>, install_on_quit: bool) -> Result<(), String> {
    download(&state).await?;
    state.on_quit.store(install_on_quit, Ordering::SeqCst);
    Ok(())
}

/// Install on the next quit, or not ("Install updates automatically" off).
#[tauri::command]
pub fn update_on_quit(state: State<'_, Updates>, on: bool) {
    state.on_quit.store(on && state.bytes.lock().unwrap().is_some(), Ordering::SeqCst);
}

/// Update now: install, then start the new version.
#[tauri::command]
pub async fn update_install<R: Runtime>(app: AppHandle<R>, state: State<'_, Updates>) -> Result<(), String> {
    let (u, bytes) = download(&state).await?;
    state.on_quit.store(false, Ordering::SeqCst);
    // Windows: the installer closes Roundtable, installs and starts it again.
    u.install(bytes).map_err(|e| e.to_string())?;
    // Mac: the new app is in place; start it.
    app.restart();
}

/// On quit: install a downloaded update when the person chose that. Never restarts.
pub fn install_on_exit(state: &Updates) {
    if !state.on_quit.swap(false, Ordering::SeqCst) {
        return;
    }
    let Some(u) = state.found.lock().unwrap().clone() else { return };
    let Some((v, bytes)) = state.bytes.lock().unwrap().take() else { return };
    if v != u.version {
        return;
    }
    if let Err(e) = u.restart_after_install(false).install(bytes) {
        eprintln!("roundtable: the update to {v} did not install on quit: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;

    #[test]
    fn channels() {
        assert_eq!(endpoint("stable"), STABLE_URL);
        assert_eq!(endpoint(""), STABLE_URL);
        assert_eq!(endpoint("test"), TEST_URL);
    }

    #[test]
    fn stable_endpoint_matches_the_config() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["plugins"]["updater"]["endpoints"][0], STABLE_URL);
        assert!(conf.get("version").is_none(), "the version comes from desktop/Cargo.toml only");
    }

    /// The updater installs a version only when it is greater than this one, by semver.
    #[test]
    fn test_builds_order_the_way_the_updater_compares() {
        let order = ["0.1.1", "0.1.2-rc.1", "0.1.2-rc.2", "0.1.2-rc.9", "0.1.2-rc.10", "0.1.2", "0.1.3", "0.1.10", "0.1.99", "0.2.0"];
        for w in order.windows(2) {
            assert!(Version::parse(w[0]).unwrap() < Version::parse(w[1]).unwrap(), "{} < {}", w[0], w[1]);
        }
        // The old rc tags without a dot sorted rc10 before rc9: why rc.N.
        assert!(Version::parse("0.1.1-rc10").unwrap() < Version::parse("0.1.1-rc9").unwrap());
    }

    #[test]
    fn app_and_engine_share_one_version() {
        assert_eq!(env!("CARGO_PKG_VERSION"), roundtable_engine::VERSION);
    }
}
