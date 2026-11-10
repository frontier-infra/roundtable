//! Screen 2, "Set up": put the bundled engine in a stable per-user place and check it runs.
//!
//! The engine ships inside the app as a Tauri sidecar named `roundtable-engine`.
//! Setup copies it to the app-data engine folder as `roundtable` (`roundtable.exe`),
//! so Claude's config keeps working if the app moves or updates.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Error codes shown on "Get help".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum InstallError {
    /// E_SIDECAR: the app bundle has no engine.
    MissingSidecar(String),
    /// E_COPY: could not write the engine folder.
    Copy(String),
    /// E_VERSION: the copied engine reports another version, or no version.
    Version(String),
    /// E_SELFTEST: the engine's MCP selftest failed.
    Selftest(String),
}

impl InstallError {
    pub fn code(&self) -> &'static str {
        match self {
            InstallError::MissingSidecar(_) => "E_SIDECAR",
            InstallError::Copy(_) => "E_COPY",
            InstallError::Version(_) => "E_VERSION",
            InstallError::Selftest(_) => "E_SELFTEST",
        }
    }
    pub fn detail(&self) -> &str {
        match self {
            InstallError::MissingSidecar(s) | InstallError::Copy(s) | InstallError::Version(s) | InstallError::Selftest(s) => s,
        }
    }
    /// Which of the three ticks failed (0-based).
    pub fn step(&self) -> usize {
        match self {
            InstallError::MissingSidecar(_) | InstallError::Copy(_) => 0,
            InstallError::Version(_) | InstallError::Selftest(_) => 1,
        }
    }
}

/// Runs the engine and returns (success, stdout).
pub trait Probe {
    fn run(&self, exe: &Path, args: &[&str]) -> Result<(bool, String), String>;
}

/// The real thing: runs the binary, with no console window on Windows.
pub struct RealProbe;

impl Probe for RealProbe {
    fn run(&self, exe: &Path, args: &[&str]) -> Result<(bool, String), String> {
        let mut c = Command::new(exe);
        c.args(args);
        no_window(&mut c);
        let o = c.output().map_err(|e| e.to_string())?;
        Ok((o.status.success(), String::from_utf8_lossy(&o.stdout).into_owned()))
    }
}

pub fn no_window(_c: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        _c.creation_flags(0x0800_0000);
    }
}

/// The sidecar next to the app's own executable.
pub fn sidecar_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(format!("roundtable-engine{}", std::env::consts::EXE_SUFFIX)))
}

pub fn installed_path(dir: &Path) -> PathBuf {
    dir.join(roundtable_engine::paths::engine_file_name())
}

/// Version the engine at `exe` reports, from `version --json`.
pub fn engine_version(exe: &Path, probe: &dyn Probe) -> Option<String> {
    let (ok, out) = probe.run(exe, &["version", "--json"]).ok()?;
    if !ok {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(out.trim())
        .ok()?
        .get("version")?
        .as_str()
        .map(str::to_string)
}

/// True when there is no engine in place or it is another version.
pub fn needs_install(dir: &Path, expected: &str, probe: &dyn Probe) -> bool {
    let p = installed_path(dir);
    !p.is_file() || engine_version(&p, probe).as_deref() != Some(expected)
}

/// Copy `src` into `dir`, replacing any old engine, even one Claude has open.
fn place(src: &Path, dir: &Path) -> Result<PathBuf, InstallError> {
    let dest = installed_path(dir);
    std::fs::create_dir_all(dir).map_err(|e| InstallError::Copy(e.to_string()))?;
    let tmp = dir.join(format!("{}.new", roundtable_engine::paths::engine_file_name()));
    std::fs::copy(src, &tmp).map_err(|e| InstallError::Copy(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(|e| InstallError::Copy(e.to_string()))?;
    }
    if dest.exists() {
        // Windows cannot overwrite a running exe, but it can rename one.
        let old = dir.join(format!("{}.old-{}", roundtable_engine::paths::engine_file_name(), roundtable_engine::history::now()));
        if std::fs::rename(&dest, &old).is_err() {
            let _ = std::fs::remove_file(&dest);
        }
    }
    std::fs::rename(&tmp, &dest).map_err(|e| InstallError::Copy(e.to_string()))?;
    // Best effort: clear old copies that are no longer running.
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().contains(".old-") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    Ok(dest)
}

/// The three ticks on screen 2: put in place, check it runs, ready.
/// `on_step(i)` fires as each tick turns green.
pub fn install(
    src: Option<&Path>,
    dir: &Path,
    expected: &str,
    probe: &dyn Probe,
    on_step: &dyn Fn(usize),
) -> Result<PathBuf, InstallError> {
    let src = match src {
        Some(s) if s.is_file() => s,
        Some(s) => return Err(InstallError::MissingSidecar(s.display().to_string())),
        None => return Err(InstallError::MissingSidecar("no app folder".into())),
    };
    let dest = place(src, dir)?;
    on_step(0);
    match engine_version(&dest, probe) {
        Some(v) if v == expected => {}
        Some(v) => return Err(InstallError::Version(format!("engine {v}, app {expected}"))),
        None => return Err(InstallError::Version("the engine did not start".into())),
    }
    match probe.run(&dest, &["mcp", "selftest"]) {
        Ok((true, _)) => {}
        Ok((false, out)) => return Err(InstallError::Selftest(out.trim().chars().take(200).collect())),
        Err(e) => return Err(InstallError::Selftest(e)),
    }
    on_step(1);
    on_step(2);
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct FakeProbe {
        version: &'static str,
        selftest_ok: bool,
        calls: RefCell<Vec<String>>,
    }

    impl Probe for FakeProbe {
        fn run(&self, exe: &Path, args: &[&str]) -> Result<(bool, String), String> {
            self.calls.borrow_mut().push(format!("{} {}", exe.file_name().unwrap().to_string_lossy(), args.join(" ")));
            match args {
                ["version", "--json"] => Ok((true, format!("{{\"version\":\"{}\"}}", self.version))),
                ["mcp", "selftest"] => Ok((self.selftest_ok, "{\"ok\":false}".into())),
                _ => Err("unexpected".into()),
            }
        }
    }

    fn probe(version: &'static str, ok: bool) -> FakeProbe {
        FakeProbe { version, selftest_ok: ok, calls: RefCell::new(vec![]) }
    }

    fn sidecar(dir: &Path) -> PathBuf {
        let s = dir.join("roundtable-engine");
        std::fs::write(&s, b"engine-bytes").unwrap();
        s
    }

    #[test]
    fn installs_and_checks_in_order() {
        let t = tempfile::tempdir().unwrap();
        let src = sidecar(t.path());
        let dir = t.path().join("data/engine");
        let p = probe("0.1.1", true);
        let steps = RefCell::new(vec![]);
        let dest = install(Some(&src), &dir, "0.1.1", &p, &|i| steps.borrow_mut().push(i)).unwrap();
        assert_eq!(dest, installed_path(&dir));
        assert_eq!(std::fs::read(&dest).unwrap(), b"engine-bytes");
        assert_eq!(*steps.borrow(), vec![0, 1, 2]);
        let name = roundtable_engine::paths::engine_file_name();
        assert_eq!(*p.calls.borrow(), vec![format!("{name} version --json"), format!("{name} mcp selftest")]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&dest).unwrap().permissions().mode() & 0o777, 0o755);
        }
    }

    #[test]
    fn replaces_an_old_engine_and_leaves_no_temp_files() {
        let t = tempfile::tempdir().unwrap();
        let src = sidecar(t.path());
        let dir = t.path().join("engine");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(installed_path(&dir), b"old").unwrap();
        install(Some(&src), &dir, "0.1.1", &probe("0.1.1", true), &|_| {}).unwrap();
        assert_eq!(std::fs::read(installed_path(&dir)).unwrap(), b"engine-bytes");
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 1, "{names:?}");
    }

    #[test]
    fn missing_sidecar_is_e_sidecar() {
        let t = tempfile::tempdir().unwrap();
        let e = install(Some(&t.path().join("nope")), t.path(), "0.1.1", &probe("0.1.1", true), &|_| {}).unwrap_err();
        assert_eq!(e.code(), "E_SIDECAR");
        assert_eq!(install(None, t.path(), "0.1.1", &probe("0.1.1", true), &|_| {}).unwrap_err().code(), "E_SIDECAR");
    }

    #[test]
    fn version_mismatch_is_e_version() {
        let t = tempfile::tempdir().unwrap();
        let src = sidecar(t.path());
        let steps = RefCell::new(vec![]);
        let e = install(Some(&src), &t.path().join("e"), "0.1.1", &probe("0.1.0", true), &|i| steps.borrow_mut().push(i)).unwrap_err();
        assert_eq!(e.code(), "E_VERSION");
        assert_eq!(e.step(), 1);
        assert_eq!(*steps.borrow(), vec![0]);
    }

    #[test]
    fn failed_selftest_is_e_selftest() {
        let t = tempfile::tempdir().unwrap();
        let src = sidecar(t.path());
        let e = install(Some(&src), &t.path().join("e"), "0.1.1", &probe("0.1.1", false), &|_| {}).unwrap_err();
        assert_eq!(e.code(), "E_SELFTEST");
    }

    #[test]
    fn needs_install_when_missing_or_other_version() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("engine");
        assert!(needs_install(&dir, "0.1.1", &probe("0.1.1", true)));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(installed_path(&dir), b"x").unwrap();
        assert!(!needs_install(&dir, "0.1.1", &probe("0.1.1", true)));
        assert!(needs_install(&dir, "0.1.2", &probe("0.1.1", true)));
    }

    /// The real engine binary from this workspace, through the real probe.
    #[test]
    fn real_engine_passes_setup() {
        let bin = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/debug")
            .join(format!("roundtable-engine{}", std::env::consts::EXE_SUFFIX));
        if !bin.is_file() {
            eprintln!("skip: build the engine first (cargo build -p roundtable-engine)");
            return;
        }
        let t = tempfile::tempdir().unwrap();
        let dest = install(Some(&bin), &t.path().join("engine"), roundtable_engine::VERSION, &RealProbe, &|_| {}).unwrap();
        assert!(dest.is_file());
    }
}
