//! Past roundtables on this computer: one Markdown transcript plus one index row each.
//! Nothing leaves the computer.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub id: String,
    /// Seconds since 1970.
    pub ts: u64,
    pub question: String,
    /// "app", "claude" or "terminal".
    pub source: String,
    pub mode: String,
    pub heads: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consensus: Option<bool>,
}

const INDEX: &str = "index.jsonl";

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() < 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

pub fn new_id() -> String {
    let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    format!("{}-{:04x}", d.as_secs(), (d.subsec_nanos() ^ std::process::id()) & 0xffff)
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn save(dir: &Path, entry: &Entry, transcript: &str) -> std::io::Result<()> {
    if !valid_id(&entry.id) {
        return Err(std::io::Error::other("bad id"));
    }
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(format!("{}.md", entry.id)), transcript)?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(dir.join(INDEX))?;
    writeln!(f, "{}", serde_json::to_string(entry)?)
}

/// Newest first. Rows whose transcript is gone, and flag-only or test runs, are dropped.
pub fn list(dir: &Path) -> Vec<Entry> {
    let text = std::fs::read_to_string(dir.join(INDEX)).unwrap_or_default();
    let mut v: Vec<Entry> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
        .filter(|e| valid_id(&e.id) && worth_keeping(&e.question, &e.source) && dir.join(format!("{}.md", e.id)).exists())
        .collect();
    v.sort_by(|a, b| b.ts.cmp(&a.ts).then(b.id.cmp(&a.id)));
    v
}

pub fn read(dir: &Path, id: &str) -> Option<String> {
    if !valid_id(id) {
        return None;
    }
    std::fs::read_to_string(dir.join(format!("{id}.md"))).ok()
}

/// Files that belong to one roundtable: its transcript and any sidecar (`<id>.<anything>`).
fn files_of(dir: &Path, id: &str) -> Vec<std::path::PathBuf> {
    let prefix = format!("{id}.");
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.file_name().to_str().is_some_and(|n| n.starts_with(&prefix)) && e.path().is_file())
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

/// Rewrite the index without the rows for `ids`. Returns the rows taken out.
fn take_rows(dir: &Path, ids: &[&str]) -> std::io::Result<Vec<String>> {
    let text = std::fs::read_to_string(dir.join(INDEX)).unwrap_or_default();
    let (mut out, mut taken) = (String::new(), Vec::new());
    for l in text.lines() {
        match serde_json::from_str::<Entry>(l) {
            Ok(e) if ids.contains(&e.id.as_str()) => taken.push(l.to_string()),
            Ok(_) => {
                out.push_str(l);
                out.push('\n');
            }
            Err(_) => {}
        }
    }
    if dir.exists() {
        std::fs::write(dir.join(INDEX), out)?;
    }
    Ok(taken)
}

/// Delete one roundtable for good: its transcript, its sidecars and its index row.
pub fn delete(dir: &Path, id: &str) -> std::io::Result<()> {
    if !valid_id(id) {
        return Err(std::io::Error::other("bad id"));
    }
    for f in files_of(dir, id) {
        std::fs::remove_file(f)?;
    }
    take_rows(dir, &[id]).map(|_| ())
}

const TRASH: &str = ".trash";

/// Take roundtables out of history, but keep them aside so Undo can put them back.
/// Returns a token for [`restore`] or [`purge`]. Nothing is listed or readable until restored.
pub fn trash(dir: &Path, ids: &[String]) -> std::io::Result<String> {
    if ids.is_empty() || !ids.iter().all(|i| valid_id(i)) {
        return Err(std::io::Error::other("bad id"));
    }
    let token = new_id();
    let bin = dir.join(TRASH).join(&token);
    std::fs::create_dir_all(&bin)?;
    for id in ids {
        for f in files_of(dir, id) {
            if let Some(name) = f.file_name() {
                std::fs::rename(&f, bin.join(name))?;
            }
        }
    }
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let rows = take_rows(dir, &refs)?;
    let mut body = rows.join("\n");
    if !body.is_empty() {
        body.push('\n');
    }
    std::fs::write(bin.join(INDEX), body)?;
    Ok(token)
}

/// Put back what [`trash`] took out, exactly as it was.
pub fn restore(dir: &Path, token: &str) -> std::io::Result<usize> {
    if !valid_id(token) {
        return Err(std::io::Error::other("bad token"));
    }
    let bin = dir.join(TRASH).join(token);
    if !bin.is_dir() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "already deleted"));
    }
    let rows = std::fs::read_to_string(bin.join(INDEX)).unwrap_or_default();
    for e in std::fs::read_dir(&bin)?.filter_map(Result::ok) {
        if e.file_name() != INDEX {
            std::fs::rename(e.path(), dir.join(e.file_name()))?;
        }
    }
    let have: Vec<String> = std::fs::read_to_string(dir.join(INDEX))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<Entry>(l).ok().map(|e| e.id))
        .collect();
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(dir.join(INDEX))?;
    let mut n = 0;
    for l in rows.lines() {
        if let Ok(e) = serde_json::from_str::<Entry>(l) {
            if !have.contains(&e.id) {
                writeln!(f, "{l}")?;
                n += 1;
            }
        }
    }
    std::fs::remove_dir_all(&bin)?;
    Ok(n)
}

/// Delete what [`trash`] took out, for good.
pub fn purge(dir: &Path, token: &str) -> std::io::Result<()> {
    if !valid_id(token) {
        return Err(std::io::Error::other("bad token"));
    }
    match std::fs::remove_dir_all(dir.join(TRASH).join(token)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        r => r,
    }
}

/// Delete everything still waiting in the trash (left by a quit during the Undo window).
pub fn purge_all(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir.join(TRASH)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        r => r,
    }
}

/// Every roundtable, the trash included, gone from this computer.
pub fn clear(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        r => r,
    }
}

/// A question that is only flags (`--help`, `-h`, `--version`) or empty. Never a roundtable.
pub fn flag_only(question: &str) -> bool {
    question.split_whitespace().all(|w| w.starts_with('-'))
}

/// Runs started by tests or the setup check never go in history.
pub fn keeps_source(source: &str) -> bool {
    !matches!(source, "setup" | "test")
}

/// Should this run be kept in history?
pub fn worth_keeping(question: &str, source: &str) -> bool {
    !flag_only(question) && keeps_source(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(id: &str, ts: u64) -> Entry {
        Entry { id: id.into(), ts, question: "Q".into(), source: "app".into(), mode: "advisory".into(), heads: vec![], consensus: None }
    }

    #[test]
    fn save_list_delete_clear() {
        let d = tempfile::tempdir().unwrap();
        let h = d.path().join("history");
        save(&h, &e("100-aaaa", 100), "one").unwrap();
        save(&h, &e("200-bbbb", 200), "two").unwrap();
        let l = list(&h);
        assert_eq!(l.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), vec!["200-bbbb", "100-aaaa"]);
        assert_eq!(read(&h, "100-aaaa").as_deref(), Some("one"));
        delete(&h, "200-bbbb").unwrap();
        assert_eq!(list(&h).len(), 1);
        clear(&h).unwrap();
        assert!(list(&h).is_empty());
        clear(&h).unwrap();
    }

    #[test]
    fn rejects_path_tricks() {
        let d = tempfile::tempdir().unwrap();
        assert!(read(d.path(), "../secret").is_none());
        assert!(delete(d.path(), "../x").is_err());
        assert!(save(d.path(), &e("a/b", 1), "x").is_err());
    }

    fn ids(h: &Path) -> Vec<String> {
        list(h).into_iter().map(|x| x.id).collect()
    }

    #[test]
    fn delete_removes_transcript_and_sidecars_for_real() {
        let d = tempfile::tempdir().unwrap();
        let h = d.path().join("history");
        save(&h, &e("100-aaaa", 100), "one").unwrap();
        save(&h, &e("100-aaa", 99), "near name").unwrap();
        std::fs::write(h.join("100-aaaa.attached.md"), "summary of plan.pdf").unwrap();
        delete(&h, "100-aaaa").unwrap();
        assert!(!h.join("100-aaaa.md").exists());
        assert!(!h.join("100-aaaa.attached.md").exists());
        assert!(h.join("100-aaa.md").exists(), "a similar id is left alone");
        assert_eq!(ids(&h), vec!["100-aaa"]);
        assert!(!std::fs::read_to_string(h.join(INDEX)).unwrap().contains("100-aaaa"));
    }

    #[test]
    fn trash_hides_restore_puts_back_purge_deletes() {
        let d = tempfile::tempdir().unwrap();
        let h = d.path().join("history");
        for (id, ts) in [("100-aaaa", 100), ("200-bbbb", 200), ("300-cccc", 300)] {
            save(&h, &e(id, ts), id).unwrap();
        }
        std::fs::write(h.join("200-bbbb.attached.md"), "summary").unwrap();
        let before = std::fs::read_to_string(h.join(INDEX)).unwrap();

        // Several at once, then Undo: everything back, byte for byte.
        let t = trash(&h, &["100-aaaa".into(), "200-bbbb".into()]).unwrap();
        assert_eq!(ids(&h), vec!["300-cccc"]);
        assert!(read(&h, "200-bbbb").is_none());
        assert_eq!(restore(&h, &t).unwrap(), 2);
        assert_eq!(ids(&h), vec!["300-cccc", "200-bbbb", "100-aaaa"]);
        assert_eq!(read(&h, "200-bbbb").as_deref(), Some("200-bbbb"));
        assert_eq!(std::fs::read_to_string(h.join("200-bbbb.attached.md")).unwrap(), "summary");
        let mut a: Vec<_> = before.lines().collect();
        let after = std::fs::read_to_string(h.join(INDEX)).unwrap();
        let mut b: Vec<_> = after.lines().collect();
        a.sort();
        b.sort();
        assert_eq!(a, b);
        assert!(!h.join(TRASH).join(&t).exists());

        // No Undo: purge deletes the files from disk.
        let t = trash(&h, &["200-bbbb".into()]).unwrap();
        purge(&h, &t).unwrap();
        assert!(!h.join("200-bbbb.md").exists());
        assert!(!h.join("200-bbbb.attached.md").exists());
        assert!(!h.join(TRASH).join(&t).exists());
        assert!(restore(&h, &t).is_err(), "too late to undo");
        assert_eq!(ids(&h), vec!["300-cccc", "100-aaaa"]);

        // A quit during the Undo window: the next start purges it.
        let _t = trash(&h, &["100-aaaa".into()]).unwrap();
        purge_all(&h).unwrap();
        assert!(!h.join(TRASH).exists());
        assert!(!h.join("100-aaaa.md").exists());
        assert_eq!(ids(&h), vec!["300-cccc"]);
    }

    #[test]
    fn trash_rejects_bad_ids_and_tokens() {
        let d = tempfile::tempdir().unwrap();
        assert!(trash(d.path(), &[]).is_err());
        assert!(trash(d.path(), &["../x".into()]).is_err());
        assert!(restore(d.path(), "../x").is_err());
        assert!(purge(d.path(), "../x").is_err());
    }

    #[test]
    fn clear_removes_trash_too() {
        let d = tempfile::tempdir().unwrap();
        let h = d.path().join("history");
        save(&h, &e("100-aaaa", 100), "one").unwrap();
        save(&h, &e("200-bbbb", 200), "two").unwrap();
        trash(&h, &["100-aaaa".into()]).unwrap();
        clear(&h).unwrap();
        assert!(!h.exists());
    }

    #[test]
    fn flags_and_test_runs_are_not_roundtables() {
        for q in ["--help", "-h", "--version", "-V", "", "   ", "--help --json"] {
            assert!(flag_only(q), "{q:?}");
            assert!(!worth_keeping(q, "terminal"));
        }
        for q in ["Should we hire?", "-- is this a dash question?", "Is -h a flag?"] {
            assert!(!flag_only(q), "{q:?}");
        }
        assert!(!worth_keeping("Should we hire?", "setup"));
        assert!(!worth_keeping("Should we hire?", "test"));
        assert!(worth_keeping("Should we hire?", "app"));
        // Old rows saved before the filter stay hidden.
        let d = tempfile::tempdir().unwrap();
        let mut bad = e("100-aaaa", 100);
        bad.question = "--help".into();
        save(d.path(), &bad, "x").unwrap();
        let mut setup = e("150-abcd", 150);
        setup.source = "setup".into();
        save(d.path(), &setup, "x").unwrap();
        save(d.path(), &e("200-bbbb", 200), "y").unwrap();
        assert_eq!(ids(d.path()), vec!["200-bbbb"]);
    }
}
