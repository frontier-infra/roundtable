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

/// Newest first. Rows whose transcript is gone are dropped.
pub fn list(dir: &Path) -> Vec<Entry> {
    let text = std::fs::read_to_string(dir.join(INDEX)).unwrap_or_default();
    let mut v: Vec<Entry> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
        .filter(|e| valid_id(&e.id) && dir.join(format!("{}.md", e.id)).exists())
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

pub fn delete(dir: &Path, id: &str) -> std::io::Result<()> {
    if !valid_id(id) {
        return Err(std::io::Error::other("bad id"));
    }
    let _ = std::fs::remove_file(dir.join(format!("{id}.md")));
    let text = std::fs::read_to_string(dir.join(INDEX)).unwrap_or_default();
    let kept: Vec<&str> = text
        .lines()
        .filter(|l| serde_json::from_str::<Entry>(l).map(|e| e.id != id).unwrap_or(false))
        .collect();
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    std::fs::write(dir.join(INDEX), out)
}

pub fn clear(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        r => r,
    }
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
}
