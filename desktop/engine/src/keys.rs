//! API keys. The engine owns the OS keychain items (spec D2).
//!
//! Mac: login Keychain. Windows: Credential Manager. Both through the `keyring` crate.
//! Service `sh.roundtable.engine`, account = provider name (anthropic, google, ...).
//!
//! Read order for a head's key:
//! 1. the process environment (for example keys a Claude Code plugin passes in),
//! 2. the OS keychain,
//! 3. the command-line version's dotenv key file, read only.

use crate::providers::{Head, HEADS};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

pub const SERVICE: &str = "sh.roundtable.engine";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Env,
    Keychain,
    File,
}

pub trait KeyStore: Send + Sync {
    fn get(&self, provider: &str) -> Option<String>;
    fn set(&self, provider: &str, key: &str) -> Result<(), String>;
    fn remove(&self, provider: &str) -> Result<(), String>;
}

/// The real OS store.
pub struct KeychainStore;

impl KeyStore for KeychainStore {
    fn get(&self, provider: &str) -> Option<String> {
        keyring::Entry::new(SERVICE, provider)
            .ok()?
            .get_password()
            .ok()
            .filter(|k| !k.trim().is_empty())
    }
    fn set(&self, provider: &str, key: &str) -> Result<(), String> {
        keyring::Entry::new(SERVICE, provider)
            .and_then(|e| e.set_password(key))
            .map_err(|e| format!("keychain: {e}"))
    }
    fn remove(&self, provider: &str) -> Result<(), String> {
        match keyring::Entry::new(SERVICE, provider).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("keychain: {e}")),
        }
    }
}

/// In-memory store for tests.
#[derive(Default)]
pub struct MemoryStore(pub Mutex<BTreeMap<String, String>>);

impl KeyStore for MemoryStore {
    fn get(&self, provider: &str) -> Option<String> {
        self.0.lock().unwrap().get(provider).cloned()
    }
    fn set(&self, provider: &str, key: &str) -> Result<(), String> {
        self.0.lock().unwrap().insert(provider.into(), key.into());
        Ok(())
    }
    fn remove(&self, provider: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(provider);
        Ok(())
    }
}

/// Parse a dotenv file: `KEY=value`, `export KEY="value"`, `#` comments.
pub fn parse_dotenv(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let l = l.strip_prefix("export ").unwrap_or(l);
        let Some((k, v)) = l.split_once('=') else { continue };
        let k = k.trim();
        if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let mut v = v.trim().to_string();
        if v.len() >= 2
            && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')))
        {
            v = v[1..v.len() - 1].to_string();
        }
        out.insert(k.to_string(), v);
    }
    out
}

/// Clean up a pasted key: trim spaces, newlines and wrapping quotes.
pub fn clean_key(raw: &str) -> String {
    let t = raw.trim();
    let t = t.trim_matches(|c| c == '"' || c == '\'');
    t.trim().to_string()
}

/// Where keys come from, in read order.
pub struct Keys<'a> {
    pub store: &'a dyn KeyStore,
    pub env: BTreeMap<String, String>,
    pub file: BTreeMap<String, String>,
}

impl<'a> Keys<'a> {
    /// Live environment, the given store, and the CLI key file.
    pub fn load(store: &'a dyn KeyStore) -> Keys<'a> {
        let env = std::env::vars().collect();
        let file = std::fs::read_to_string(crate::paths::cli_config_file())
            .map(|t| parse_dotenv(&t))
            .unwrap_or_default();
        Keys { store, env, file }
    }

    pub fn resolve(&self, head: &Head) -> Option<(String, Source)> {
        if head.env_keys.is_empty() {
            return None;
        }
        for k in head.env_keys {
            if let Some(v) = self.env.get(*k).filter(|v| !v.trim().is_empty()) {
                return Some((v.clone(), Source::Env));
            }
        }
        if let Some(v) = self.store.get(head.provider) {
            return Some((v, Source::Keychain));
        }
        for k in head.env_keys {
            if let Some(v) = self.file.get(*k).filter(|v| !v.trim().is_empty()) {
                return Some((v.clone(), Source::File));
            }
        }
        None
    }

    pub fn key(&self, head: &Head) -> Option<String> {
        self.resolve(head).map(|(k, _)| k)
    }
}

/// Keys found in the CLI key file, by provider. Values never leave the engine.
pub fn importable(file: &BTreeMap<String, String>) -> Vec<&'static Head> {
    HEADS
        .iter()
        .filter(|h| h.env_keys.iter().any(|k| file.get(*k).is_some_and(|v| !v.trim().is_empty())))
        .collect()
}

/// Copy keys from the CLI key file into the store. Returns the providers imported.
pub fn import_from_file(store: &dyn KeyStore, file: &BTreeMap<String, String>) -> Result<Vec<String>, String> {
    let mut done = Vec::new();
    for h in importable(file) {
        let v = h.env_keys.iter().find_map(|k| file.get(*k).filter(|v| !v.trim().is_empty())).unwrap();
        store.set(h.provider, &clean_key(v))?;
        done.push(h.provider.to_string());
    }
    Ok(done)
}

/// Remove the imported key lines from the CLI key file. Other lines stay as they were.
pub fn strip_keys_from_file(path: &Path) -> std::io::Result<usize> {
    let text = std::fs::read_to_string(path)?;
    let names: Vec<&str> = HEADS.iter().flat_map(|h| h.env_keys.iter().copied()).collect();
    let mut removed = 0;
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| {
            let l = line.trim();
            let l = l.strip_prefix("export ").unwrap_or(l);
            let hit = l.split_once('=').is_some_and(|(k, _)| names.contains(&k.trim()));
            if hit {
                removed += 1;
            }
            !hit
        })
        .collect();
    let mut out = kept.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    std::fs::write(path, out)?;
    Ok(removed)
}

/// Keychain test hook: `ROUNDTABLE_KEYSTORE=memory` swaps in an empty memory store.
pub fn default_store() -> Box<dyn KeyStore> {
    if std::env::var("ROUNDTABLE_KEYSTORE").as_deref() == Ok("memory") {
        Box::new(MemoryStore::default())
    } else {
        Box::new(KeychainStore)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::find;

    fn keys<'a>(store: &'a MemoryStore, env: &[(&str, &str)], file: &[(&str, &str)]) -> Keys<'a> {
        Keys {
            store,
            env: env.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
            file: file.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
        }
    }

    #[test]
    fn read_order_env_then_keychain_then_file() {
        let store = MemoryStore::default();
        let claude = find("claude").unwrap();
        let k = keys(&store, &[], &[("ANTHROPIC_API_KEY", "from-file")]);
        assert_eq!(k.resolve(claude), Some(("from-file".into(), Source::File)));

        store.set("anthropic", "from-keychain").unwrap();
        let k = keys(&store, &[], &[("ANTHROPIC_API_KEY", "from-file")]);
        assert_eq!(k.resolve(claude), Some(("from-keychain".into(), Source::Keychain)));

        let k = keys(&store, &[("ANTHROPIC_API_KEY", "from-env")], &[("ANTHROPIC_API_KEY", "x")]);
        assert_eq!(k.resolve(claude), Some(("from-env".into(), Source::Env)));
    }

    #[test]
    fn empty_env_value_does_not_win() {
        let store = MemoryStore::default();
        store.set("openai", "kc").unwrap();
        let k = keys(&store, &[("OPENAI_API_KEY", "  ")], &[]);
        assert_eq!(k.key(find("openai").unwrap()).as_deref(), Some("kc"));
    }

    #[test]
    fn gemini_accepts_google_api_key() {
        let store = MemoryStore::default();
        let k = keys(&store, &[("GOOGLE_API_KEY", "g")], &[]);
        assert_eq!(k.key(find("gemini").unwrap()).as_deref(), Some("g"));
    }

    #[test]
    fn codex_never_has_a_key() {
        let store = MemoryStore::default();
        assert!(keys(&store, &[], &[]).resolve(find("codex").unwrap()).is_none());
    }

    #[test]
    fn set_and_remove() {
        let store = MemoryStore::default();
        store.set("xai", "k1").unwrap();
        assert_eq!(store.get("xai").as_deref(), Some("k1"));
        store.remove("xai").unwrap();
        assert!(store.get("xai").is_none());
        store.remove("xai").unwrap();
    }

    #[test]
    fn dotenv_parsing() {
        let m = parse_dotenv("# c\nexport OPENAI_API_KEY=\"sk-1\"\nZAI_API_KEY='z'\nBAD LINE\nROUNDTABLE_HEADS=claude\n");
        assert_eq!(m.get("OPENAI_API_KEY").unwrap(), "sk-1");
        assert_eq!(m.get("ZAI_API_KEY").unwrap(), "z");
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn clean_pasted_key() {
        assert_eq!(clean_key("  \"sk-abc\"\n"), "sk-abc");
        assert_eq!(clean_key("sk-abc\r\n"), "sk-abc");
    }

    #[test]
    fn import_then_strip() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("keys");
        std::fs::write(&p, "OPENAI_API_KEY=o\nROUNDTABLE_HEADS=claude\nexport GEMINI_API_KEY=g\n").unwrap();
        let file = parse_dotenv(&std::fs::read_to_string(&p).unwrap());
        let store = MemoryStore::default();
        let got = import_from_file(&store, &file).unwrap();
        assert_eq!(got, vec!["openai", "google"]);
        assert_eq!(store.get("google").as_deref(), Some("g"));
        assert_eq!(strip_keys_from_file(&p).unwrap(), 2);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "ROUNDTABLE_HEADS=claude\n");
    }

    /// Writes and deletes a real item. Run by hand: cargo test -- --ignored
    #[test]
    #[ignore]
    fn real_keychain_round_trip() {
        let s = KeychainStore;
        let acct = "roundtable-selftest";
        s.set(acct, "not-a-real-key").unwrap();
        assert_eq!(s.get(acct).as_deref(), Some("not-a-real-key"));
        s.remove(acct).unwrap();
        assert!(s.get(acct).is_none());
    }
}
