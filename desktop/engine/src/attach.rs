//! Files and links every seat reads. Same rules and block as `lib/core.sh`.
//!
//! A source is a text file, a PDF, or an http(s) link. Each is read once. One that is
//! longer than [`SHARE_LIMIT`] characters is summarized once by the chair's seat, and every
//! seat reads the same summary. With no chair, the start of the text is shared instead.

use serde::Serialize;
use std::io::Read;
use std::time::Duration;

/// Largest file or download read, in bytes.
pub const MAX_BYTES: u64 = 20 * 1024 * 1024;
/// Text longer than this many characters is summarized once and the summary shared.
pub const SHARE_LIMIT: usize = 12_000;
/// The most text the summary prompt carries.
pub const SUMMARY_INPUT: usize = 150_000;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Attachment {
    /// The file name or the link.
    pub name: String,
    /// "text", "pdf" or "link".
    pub kind: String,
    /// Characters read from the source.
    pub chars: usize,
    /// "full", "summary" or "start".
    pub shared: String,
    /// What every seat reads.
    pub text: String,
}

fn char_prefix(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// Collapse runs of blank lines and trailing spaces.
pub fn tidy(s: &str) -> String {
    let mut out = String::new();
    let mut blank = 0;
    for line in s.replace("\r\n", "\n").replace('\r', "\n").lines() {
        let l = line.trim_end();
        if l.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(l);
        out.push('\n');
    }
    out.trim().to_string()
}

/// Plain text from HTML: drops scripts, styles and tags, decodes the common entities.
pub fn html_to_text(html: &str) -> String {
    let mut s = html.to_string();
    for tag in ["script", "style", "noscript", "svg", "head"] {
        loop {
            let low = s.to_ascii_lowercase();
            let Some(a) = low.find(&format!("<{tag}")) else { break };
            let end = format!("</{tag}>");
            match low[a..].find(&end) {
                Some(b) => s.replace_range(a..a + b + end.len(), " "),
                None => s.replace_range(a.., " "),
            }
        }
    }
    let mut out = String::new();
    let mut in_tag = false;
    let mut tag = String::new();
    for c in s.chars() {
        match (in_tag, c) {
            (false, '<') => {
                in_tag = true;
                tag.clear();
            }
            (true, '>') => {
                in_tag = false;
                let t = tag.trim_start_matches('/').split(|c: char| c.is_whitespace()).next().unwrap_or("").to_ascii_lowercase();
                if ["p", "br", "div", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6", "section", "article"].contains(&t.as_str()) {
                    out.push('\n');
                }
            }
            (true, c) => tag.push(c),
            (false, c) => out.push(c),
        }
    }
    let out = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    let lines: Vec<String> = out.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).collect();
    tidy(&lines.join("\n"))
}

fn is_pdf(name: &str, bytes: &[u8]) -> bool {
    bytes.starts_with(b"%PDF") || name.to_ascii_lowercase().ends_with(".pdf")
}

/// Text from PDF bytes.
pub fn pdf_text(bytes: &[u8]) -> Result<String, String> {
    let owned = bytes.to_vec();
    let r = std::panic::catch_unwind(move || pdf_extract::extract_text_from_mem(&owned));
    match r {
        Ok(Ok(t)) if !t.trim().is_empty() => Ok(tidy(&t)),
        Ok(Ok(_)) => Err("That PDF has no text to read. It may be a scan.".into()),
        _ => Err("Could not read that PDF.".into()),
    }
}

fn text_of(name: &str, bytes: Vec<u8>) -> Result<(String, String), String> {
    if is_pdf(name, &bytes) {
        return Ok(("pdf".into(), pdf_text(&bytes)?));
    }
    if bytes.iter().take(8000).filter(|b| **b == 0).count() > 0 {
        return Err(format!("{name} is not a text file or a PDF."));
    }
    Ok(("text".into(), tidy(&String::from_utf8_lossy(&bytes))))
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string())
}

/// Read one source: (name, kind, text).
pub fn read_source(src: &str, timeout: Duration) -> Result<(String, String, String), String> {
    let s = src.trim();
    if s.starts_with("http://") || s.starts_with("https://") {
        return fetch(s, timeout);
    }
    let meta = std::fs::metadata(s).map_err(|_| format!("Could not find {}.", file_name(s)))?;
    if meta.len() > MAX_BYTES {
        return Err(format!("{} is larger than {} MB.", file_name(s), MAX_BYTES / 1024 / 1024));
    }
    let bytes = std::fs::read(s).map_err(|e| format!("Could not read {}: {e}", file_name(s)))?;
    let name = file_name(s);
    let (kind, text) = text_of(&name, bytes)?;
    Ok((name, kind, text))
}

fn fetch(url: &str, timeout: Duration) -> Result<(String, String, String), String> {
    let agent = ureq::AgentBuilder::new().timeout(timeout).try_proxy_from_env(true).user_agent(concat!("roundtable-engine/", env!("CARGO_PKG_VERSION"))).build();
    let r = agent.get(url).call().map_err(|e| match e {
        ureq::Error::Status(c, _) => format!("That link answered with error {c}."),
        ureq::Error::Transport(_) => "Could not open that link. Check it and your connection.".to_string(),
    })?;
    read_response(url, r)
}

fn read_response(url: &str, r: ureq::Response) -> Result<(String, String, String), String> {
    let ctype = r.content_type().to_ascii_lowercase();
    let mut bytes = Vec::new();
    r.into_reader().take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(format!("That link is larger than {} MB.", MAX_BYTES / 1024 / 1024));
    }
    let text = if ctype.contains("pdf") || bytes.starts_with(b"%PDF") {
        pdf_text(&bytes)?
    } else if ctype.contains("html") || String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase().contains("<html") {
        html_to_text(&String::from_utf8_lossy(&bytes))
    } else {
        text_of(url, bytes)?.1
    };
    if text.trim().is_empty() {
        return Err("That link has no text to read.".into());
    }
    Ok((url.to_string(), "link".into(), text))
}

/// The chair's prompt to summarize one long source, once, for every seat.
pub fn summary_prompt(name: &str, text: &str) -> String {
    format!(
        "You are the CHAIR of an AI council. Every councilor will read your summary of this material instead of the whole thing.\n\nSOURCE: {name}\n\n--- MATERIAL ---\n{}\n\n--- YOUR JOB ---\nSummarize it in at most 600 words. Keep every number, name, date, limit and decision that a councilor would need. Use short bullets. Do not add facts.\n",
        char_prefix(text, SUMMARY_INPUT)
    )
}

/// Make one attachment from its text. `summarize` is the chair; None means no chair answered.
pub fn share(name: String, kind: String, text: String, summarize: &dyn Fn(&str) -> Option<String>) -> Attachment {
    let chars = text.chars().count();
    if chars <= SHARE_LIMIT {
        return Attachment { name, kind, chars, shared: "full".into(), text };
    }
    match summarize(&summary_prompt(&name, &text)).filter(|s| !s.trim().is_empty()) {
        Some(s) => Attachment { name, kind, chars, shared: "summary".into(), text: s.trim().to_string() },
        None => Attachment { name, kind, chars, shared: "start".into(), text: char_prefix(&text, SHARE_LIMIT).to_string() },
    }
}

/// The section every seat reads, in front of the question.
pub fn block(atts: &[Attachment]) -> String {
    let mut out = String::from("Every seat reads the same material.\n");
    for a in atts {
        let note = match a.shared.as_str() {
            "summary" => " (summary)".to_string(),
            "start" => format!(" (first {SHARE_LIMIT} characters)"),
            _ => String::new(),
        };
        out.push_str(&format!("\n### {}{note}\n{}\n", a.name, a.text.trim()));
    }
    out.trim_end().to_string()
}

/// Read every source once and share each: whole, or summarized once by the chair.
pub fn prepare(sources: &[String], keys: &crate::keys::Keys, settings: &crate::settings::Settings, timeout: Duration) -> Result<Vec<Attachment>, String> {
    prepare_from(sources, &[], false, keys, settings, timeout)
}

/// `prepare` for a remote MCP caller: links to public addresses only (never a path on this
/// computer, never a private or loopback address), plus documents sent inline as text.
pub fn prepare_remote(links: &[String], files: &[(String, String)], keys: &crate::keys::Keys, settings: &crate::settings::Settings, timeout: Duration) -> Result<Vec<Attachment>, String> {
    prepare_from(links, files, true, keys, settings, timeout)
}

/// Sources and inline documents, each shared once. `public_only` is the remote rule.
pub fn prepare_from(sources: &[String], files: &[(String, String)], public_only: bool, keys: &crate::keys::Keys, settings: &crate::settings::Settings, timeout: Duration) -> Result<Vec<Attachment>, String> {
    let mut out = Vec::new();
    let sum = |p: &str| Some(crate::council::chair(keys, settings, p, timeout)).filter(|t| !t.trim().is_empty());
    for src in sources.iter().filter(|s| !s.trim().is_empty()) {
        let (name, kind, text) = if public_only { read_public_link(src, timeout)? } else { read_source(src, timeout)? };
        out.push(share(name, kind, text, &sum));
    }
    for (name, text) in files {
        out.push(share(name.clone(), "text".into(), tidy(text), &sum));
    }
    Ok(out)
}

/// True for an address on the open internet: not loopback, private, link-local,
/// shared (100.64/10, which tailnets use), multicast, documentation or unspecified.
pub fn is_public_ip(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            !(v.is_loopback()
                || v.is_private()
                || v.is_link_local()
                || v.is_broadcast()
                || v.is_multicast()
                || v.is_unspecified()
                || v.is_documentation()
                || o[0] == 0
                || (o[0] == 100 && (o[1] & 0xC0) == 64)
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)
                || (o[0] == 198 && (o[1] & 0xFE) == 18)
                || o[0] >= 240)
        }
        IpAddr::V6(v) => {
            if let Some(v4) = v.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            let s = v.segments();
            !(v.is_loopback() || v.is_unspecified() || v.is_multicast() || (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80 || s[0] == 0x2001 && s[1] == 0x0db8)
        }
    }
}

/// Every connection (redirects too) resolves through this, so a link cannot reach this
/// computer or its network, even by a name that changes its address between checks.
struct PublicOnly;

impl ureq::Resolver for PublicOnly {
    fn resolve(&self, netloc: &str) -> std::io::Result<Vec<std::net::SocketAddr>> {
        use std::net::ToSocketAddrs;
        let all: Vec<_> = netloc.to_socket_addrs()?.collect();
        let ok: Vec<_> = all.into_iter().filter(|a| is_public_ip(a.ip())).collect();
        if ok.is_empty() {
            return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "not a public address"));
        }
        Ok(ok)
    }
}

/// Read a link for a remote caller: http(s) to a public address only.
pub fn read_public_link(src: &str, timeout: Duration) -> Result<(String, String, String), String> {
    let s = src.trim();
    let l = s.to_ascii_lowercase();
    if !(l.starts_with("https://") || l.starts_with("http://")) {
        return Err("A remote connection can attach links only.".into());
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(timeout)
        .redirects(3)
        .resolver(PublicOnly)
        .user_agent(concat!("roundtable-engine/", env!("CARGO_PKG_VERSION")))
        .build();
    let r = agent.get(s).call().map_err(|e| match e {
        ureq::Error::Status(c, _) => format!("That link answered with error {c}."),
        ureq::Error::Transport(_) => "Could not open that link. A remote connection can only read links on the public internet.".to_string(),
    })?;
    read_response(s, r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_links_reach_only_public_addresses() {
        use std::net::IpAddr;
        for bad in ["127.0.0.1", "10.1.2.3", "192.168.1.5", "172.16.0.1", "169.254.169.254", "100.100.1.1", "0.0.0.0", "::1", "fd00::1", "fe80::1", "::ffff:127.0.0.1"] {
            assert!(!is_public_ip(bad.parse::<IpAddr>().unwrap()), "{bad}");
        }
        for good in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(is_public_ip(good.parse::<IpAddr>().unwrap()), "{good}");
        }
        assert!(read_public_link("/etc/hosts", Duration::from_secs(1)).is_err());
        assert!(read_public_link("http://127.0.0.1:9/x", Duration::from_secs(2)).unwrap_err().contains("public internet"));
        assert!(read_public_link("http://localhost:9/x", Duration::from_secs(2)).is_err());
    }

    #[test]
    fn inline_files_are_shared_like_files() {
        let store = crate::keys::MemoryStore::default();
        let k = crate::keys::Keys { store: &store, env: Default::default(), file: Default::default() };
        let s = crate::settings::Settings::default();
        let a = prepare_from(&[], &[("n.md".into(), "short notes".into())], true, &k, &s, Duration::from_secs(1)).unwrap();
        assert_eq!((a[0].name.as_str(), a[0].shared.as_str(), a[0].text.as_str()), ("n.md", "full", "short notes"));
        assert!(prepare_from(&["/etc/hosts".into()], &[], true, &k, &s, Duration::from_secs(1)).is_err());
    }

    #[test]
    fn html_becomes_text() {
        let t = html_to_text("<html><head><title>x</title></head><body><script>bad()</script><h1>Plan</h1><p>Rent is &pound; &amp; 5.</p><p>Two</p></body></html>");
        assert_eq!(t, "Plan\n\nRent is &pound; & 5.\n\nTwo");
    }

    #[test]
    fn short_text_is_shared_whole_long_is_summarized_once() {
        let calls = std::cell::Cell::new(0);
        let sum = |_: &str| {
            calls.set(calls.get() + 1);
            Some("- the gist".to_string())
        };
        let a = share("a.md".into(), "text".into(), "short".into(), &sum);
        assert_eq!((a.shared.as_str(), a.text.as_str(), calls.get()), ("full", "short", 0));
        let long = "word ".repeat(4000);
        let b = share("b.md".into(), "text".into(), long.clone(), &sum);
        assert_eq!((b.shared.as_str(), b.text.as_str(), calls.get()), ("summary", "- the gist", 1));
        let c = share("c.md".into(), "text".into(), long, &|_| None);
        assert_eq!(c.shared, "start");
        assert_eq!(c.text.chars().count(), SHARE_LIMIT);
        assert_eq!(block(&[a, b]), "Every seat reads the same material.\n\n### a.md\nshort\n\n### b.md (summary)\n- the gist");
    }

    #[test]
    fn files_are_checked() {
        let d = tempfile::tempdir().unwrap();
        let t = d.path().join("notes.md");
        std::fs::write(&t, "Line one\r\n\r\n\r\nLine two  \n").unwrap();
        assert_eq!(read_source(t.to_str().unwrap(), Duration::from_secs(5)).unwrap(), ("notes.md".into(), "text".into(), "Line one\n\nLine two".into()));
        let b = d.path().join("x.bin");
        std::fs::write(&b, [0u8, 1, 2, 0, 3]).unwrap();
        assert!(read_source(b.to_str().unwrap(), Duration::from_secs(5)).unwrap_err().contains("not a text file"));
        assert!(read_source("/no/such/file.txt", Duration::from_secs(5)).unwrap_err().contains("Could not find"));
        let bad = d.path().join("broken.pdf");
        std::fs::write(&bad, b"%PDF-1.4 not really").unwrap();
        assert!(read_source(bad.to_str().unwrap(), Duration::from_secs(5)).is_err());
    }

    /// A one-page PDF with real text, built by hand with a correct cross-reference table.
    fn tiny_pdf(text: &str) -> Vec<u8> {
        let stream = format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET");
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
            format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offs = Vec::new();
        for (i, o) in objs.iter().enumerate() {
            offs.push(out.len());
            out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).bytes());
        for o in offs {
            out.extend(format!("{o:010} 00000 n \n").bytes());
        }
        out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).bytes());
        out
    }

    #[test]
    fn pdf_text_is_read() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("plan.pdf");
        std::fs::write(&p, tiny_pdf("Budget is 40 thousand")).unwrap();
        let (name, kind, text) = read_source(p.to_str().unwrap(), Duration::from_secs(5)).unwrap();
        assert_eq!((name.as_str(), kind.as_str()), ("plan.pdf", "pdf"));
        assert!(text.contains("Budget is 40 thousand"), "{text:?}");
    }
}
