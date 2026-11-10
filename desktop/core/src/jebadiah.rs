//! What the Decision model section in Settings needs: whether `roundtable decision detect`
//! finds a Jebadiah server, this computer's memory and chip, and whether one of our
//! Jebadiah builds is already in Ollama or LM Studio. The "Get Jebadiah" hint shows
//! only when detection finds nothing.
//!
//! Only our builds count. A different Hugging Face account with a similar name holds
//! unrelated models, so a model id must carry one of our namespaces. The rule is the
//! engine's `decision::detect::is_ours`; the list is `engine/src/decision/jebadiah-ids.json`.

use roundtable_engine::http;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Found {
    pub app: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct JebadiahLocal {
    pub os: &'static str,
    pub arch: &'static str,
    pub memory_gb: Option<f64>,
    pub found: Option<Found>,
    /// What `roundtable decision detect` found: a Jebadiah server Roundtable can ask.
    pub served: Option<Served>,
    /// The engine's next step when no server answered.
    pub hint: Option<String>,
    pub looked: Vec<String>,
}

/// A Jebadiah server (jeb serve, AINode, your address), as `decision detect --json` reports it.
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct Served {
    pub url: String,
    pub model: String,
    pub via: String,
}

/// Read `roundtable decision detect --json`: the server it found, its hint, and where it looked.
pub fn served_from(v: &serde_json::Value) -> (Option<Served>, Option<String>, Vec<String>) {
    let served = v.get("found").filter(|f| !f.is_null()).and_then(|f| serde_json::from_value(f.clone()).ok());
    let hint = v.get("hint").and_then(|h| h.as_str()).map(str::to_string);
    let looked = v
        .get("looked")
        .and_then(|l| l.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).map(|x| format!("decision detect: {x}")).collect())
        .unwrap_or_default();
    (served, hint, looked)
}

const PROBE_TIMEOUT: Duration = Duration::from_millis(1200);

/// True for our builds only: the engine's rule (see `isOurModel` in ui/src/jebadiah.ts).
pub fn is_our_model(id: &str) -> bool {
    roundtable_engine::decision::detect::is_ours(id)
}

fn model_ids(v: &serde_json::Value) -> Vec<String> {
    let list = v.get("data").or_else(|| v.get("models")).and_then(|d| d.as_array()).cloned().unwrap_or_default();
    list.iter()
        .filter_map(|m| m.get("id").or_else(|| m.get("name")).or_else(|| m.get("model")).and_then(|i| i.as_str()).map(str::to_string))
        .collect()
}

/// The first of our builds in a model list.
pub fn pick(ids: &[String]) -> Option<String> {
    ids.iter().find(|i| is_our_model(i)).cloned()
}

/// Base URLs to look at: Ollama and LM Studio on their default ports.
/// `ROUNDTABLE_JEBADIAH_LOCAL` ("Ollama=http://..,LM Studio=http://..") replaces them, for tests.
fn places() -> Vec<(String, String)> {
    if let Ok(list) = std::env::var("ROUNDTABLE_JEBADIAH_LOCAL") {
        return list
            .split(',')
            .filter_map(|p| p.split_once('='))
            .map(|(a, u)| (a.trim().to_string(), u.trim().to_string()))
            .collect();
    }
    vec![
        ("Ollama".into(), "http://127.0.0.1:11434/api/tags".into()),
        ("LM Studio".into(), "http://127.0.0.1:1234/v1/models".into()),
    ]
}

pub fn look() -> (Option<Found>, Vec<String>) {
    let mut looked = Vec::new();
    for (app, url) in places() {
        match http::send("GET", &url, &[], None, PROBE_TIMEOUT) {
            Ok(v) => {
                let ids = model_ids(&v);
                looked.push(format!("{app}: {} models", ids.len()));
                if let Some(model) = pick(&ids) {
                    return (Some(Found { app, model }), looked);
                }
            }
            Err(_) => looked.push(format!("{app}: not running")),
        }
    }
    (None, looked)
}

/// Total memory in GB, or None when the system does not say.
pub fn memory_gb() -> Option<f64> {
    let bytes: Option<f64> = {
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("/usr/sbin/sysctl")
                .args(["-n", "hw.memsize"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .and_then(|s| s.trim().parse::<f64>().ok())
        }
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory"])
                .creation_flags(0x0800_0000)
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .and_then(|s| s.trim().parse::<f64>().ok())
        }
        #[cfg(target_os = "linux")]
        {
            std::fs::read_to_string("/proc/meminfo").ok().and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("MemTotal:"))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|kb| kb.parse::<f64>().ok())
                    .map(|kb| kb * 1024.0)
            })
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            None
        }
    };
    bytes.filter(|b| *b > 0.0).map(|b| (b / 1_073_741_824.0 * 10.0).round() / 10.0)
}

#[tauri::command]
pub async fn jebadiah_local() -> Result<JebadiahLocal, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let (served, hint, mut looked) = match crate::commands::engine_json(&["decision", "detect", "--json"]) {
            Ok(v) => served_from(&v),
            Err(e) => (None, None, vec![format!("decision detect: {e}")]),
        };
        let (found, more) = if served.is_some() { (None, Vec::new()) } else { look() };
        looked.extend(more);
        JebadiahLocal { os: std::env::consts::OS, arch: std::env::consts::ARCH, memory_gb: memory_gb(), found, served, hint, looked }
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn ours_and_not_theirs() {
        for id in [
            "hf.co/frontier-infra/jebadiah-9b-v2-1-GGUF:Q4_K_M",
            "frontierinfra/jebadiah:27b",
            "frontierinfra/jebadiah:latest",
            "jbrashearfrontierinfra/jebadiah:9b",
            "frontier-infra/jebadiah-27b-v2-1-MLX-4bit",
        ] {
            assert!(is_our_model(id), "{id}");
        }
        // A stranger's account named Jebadiah, and bare names anyone could publish.
        for id in [
            "Jebadiah/Aria-7b-128k-v1",
            "jebadiah/hermes-lexi-fun-stone-l3-8b",
            "hf.co/Jebadiah/jebadiah-9b-uncensored-GGUF:Q4_K_M",
            "jebadiah-4b-v2-mlx-8bit",
            "llama3:8b",
            "jebadiah",
        ] {
            assert!(!is_our_model(id), "{id}");
        }
    }

    #[test]
    fn picks_from_ollama_and_lm_studio_shapes() {
        let ollama = serde_json::json!({"models": [{"name": "Jebadiah/Aria-7b-128k-v1"}, {"name": "hf.co/frontier-infra/jebadiah-9b-v2-1-GGUF:Q4_K_M"}]});
        assert_eq!(pick(&model_ids(&ollama)).as_deref(), Some("hf.co/frontier-infra/jebadiah-9b-v2-1-GGUF:Q4_K_M"));
        let lms = serde_json::json!({"data": [{"id": "jebadiah-9b-uncensored"}, {"id": "frontier-infra/jebadiah-27b-v2-1-mlx-4bit"}]});
        assert_eq!(pick(&model_ids(&lms)).as_deref(), Some("frontier-infra/jebadiah-27b-v2-1-mlx-4bit"));
        let stranger = serde_json::json!({"models": [{"name": "hf.co/Jebadiah/jebadiah-9b-uncensored-GGUF:Q4_K_M"}]});
        assert_eq!(pick(&model_ids(&stranger)), None, "a stranger's Jebadiah is never offered");
        assert_eq!(pick(&model_ids(&serde_json::json!({"data": []}))), None);
    }

    fn serve_once(body: &'static str) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = l.accept() {
                let mut buf = [0u8; 2048];
                let _ = s.read(&mut buf);
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
            }
        });
        format!("http://{addr}/api/tags")
    }

    #[test]
    fn look_finds_it_after_install() {
        let url = serve_once(r#"{"models":[{"name":"hf.co/frontier-infra/jebadiah-4b-v2-GGUF:Q4_K_M"}]}"#);
        std::env::set_var("ROUNDTABLE_JEBADIAH_LOCAL", format!("LM Studio=http://127.0.0.1:9/v1/models,Ollama={url}"));
        let (found, looked) = look();
        std::env::remove_var("ROUNDTABLE_JEBADIAH_LOCAL");
        assert_eq!(found, Some(Found { app: "Ollama".into(), model: "hf.co/frontier-infra/jebadiah-4b-v2-GGUF:Q4_K_M".into() }));
        assert_eq!(looked[0], "LM Studio: not running");
    }

    #[test]
    fn reads_decision_detect() {
        let v = serde_json::json!({"found": {"url": "http://127.0.0.1:8100", "model": "jebadiah-9b", "via": "jeb serve"}, "looked": ["http://127.0.0.1:8100: jebadiah-9b (jeb serve)"]});
        let (served, hint, looked) = served_from(&v);
        assert_eq!(served, Some(Served { url: "http://127.0.0.1:8100".into(), model: "jebadiah-9b".into(), via: "jeb serve".into() }));
        assert_eq!(hint, None);
        assert_eq!(looked, vec!["decision detect: http://127.0.0.1:8100: jebadiah-9b (jeb serve)".to_string()]);
        let none = serde_json::json!({"found": null, "looked": [], "hint": "Start Jebadiah with `jeb serve`."});
        let (served, hint, _) = served_from(&none);
        assert_eq!(served, None);
        assert_eq!(hint.as_deref(), Some("Start Jebadiah with `jeb serve`."));
    }

    #[test]
    fn memory_is_known_here() {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        assert!(memory_gb().is_some_and(|g| g > 1.0));
    }
}
