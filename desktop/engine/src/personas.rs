//! Personas: seat roles at the table. Same packs and rules as `lib/personas.py`.
//!
//! A persona sits on an AI seat. It adds a short brief in front of that seat's prompt
//! and its name to the seat's label. No persona means a Generic seat (the old behaviour).
//!
//! Built-in packs live in the repo's `personas/` folder and are compiled in. People add
//! their own in `<data dir>/personas/*.json` (the app writes `custom.json` there), or in
//! `ROUNDTABLE_PERSONAS_DIR`.
//!
//! The Frontier labs pack names seats for a lab's public view, never a person, and every
//! lab seat carries [`disclaimer`]. One switch hides it: the `lab-pack` build feature,
//! the `show_lab_pack` setting, or `ROUNDTABLE_HIDE_LAB_PACK=1`.

use crate::providers::{self, Head};
use crate::settings::Settings;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Most seats one run may have.
pub const MAX_SEATS: usize = 16;
pub const CUSTOM_PACK_ID: &str = "custom";
pub const GENERIC: &str = "generic";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Persona {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub expertise: String,
    #[serde(default)]
    pub brief: String,
    /// Preferred head id. Used when that AI sits at the table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai: Option<String>,
    /// Set on lab seats: the lab whose public view this simulates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lab: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Preset {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub seats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Pack {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The Frontier labs pack (or any pack the lab switch should hide).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub labs: bool,
    #[serde(default)]
    pub personas: Vec<Persona>,
    #[serde(default)]
    pub presets: Vec<Preset>,
    /// Compiled into the engine. Built-ins are duplicated, not edited, in the app.
    #[serde(skip_deserializing, default)]
    pub builtin: bool,
}

const BUILTIN_BUSINESS: &str = include_str!("../../../personas/business-roles.json");
const BUILTIN_PANELS: &str = include_str!("../../../personas/expert-panels.json");
#[cfg(feature = "lab-pack")]
const BUILTIN_LABS: Option<&str> = Some(include_str!("../../../personas/frontier-labs.json"));
#[cfg(not(feature = "lab-pack"))]
const BUILTIN_LABS: Option<&str> = None;

/// The label every lab seat carries in the app and in transcripts.
pub fn disclaimer(lab: &str) -> String {
    format!("Simulated perspective drawn from public positions. Not affiliated with or endorsed by {lab}.")
}

fn truthy(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// True when the build includes the lab pack.
pub fn lab_pack_built() -> bool {
    BUILTIN_LABS.is_some()
}

/// The single switch: build feature, setting, or environment.
pub fn labs_hidden(settings: &Settings) -> bool {
    !lab_pack_built()
        || !settings.show_lab_pack
        || std::env::var("ROUNDTABLE_HIDE_LAB_PACK").is_ok_and(|v| truthy(&v))
}

pub fn builtin_packs() -> Vec<Pack> {
    let mut v = Vec::new();
    for src in BUILTIN_LABS
        .into_iter()
        .chain([BUILTIN_BUSINESS, BUILTIN_PANELS])
    {
        let mut p: Pack = serde_json::from_str(src).expect("built-in persona pack is valid JSON");
        p.builtin = true;
        v.push(p);
    }
    v
}

/// Folders people keep their own packs in, in load order: the app's data folder,
/// the command line's config folder, then `ROUNDTABLE_PERSONAS_DIR`.
pub fn user_dirs() -> Vec<PathBuf> {
    let cli = crate::paths::cli_config_file()
        .parent()
        .map(|d| d.join("personas"));
    let mut v = vec![crate::paths::data_dir().join("personas")];
    v.extend(cli);
    if let Some(d) = std::env::var_os("ROUNDTABLE_PERSONAS_DIR").filter(|d| !d.is_empty()) {
        v.push(PathBuf::from(d));
    }
    v
}

pub fn custom_file() -> PathBuf {
    crate::paths::data_dir()
        .join("personas")
        .join("custom.json")
}

/// Check a pack someone wrote. Returns it cleaned up, or what is wrong in one line.
pub fn validate(mut p: Pack) -> Result<Pack, String> {
    p.id = p.id.trim().to_string();
    if p.id.is_empty() {
        return Err("The pack has no id.".into());
    }
    if p.name.trim().is_empty() {
        p.name = p.id.clone();
    }
    let mut seen = std::collections::BTreeSet::new();
    for x in &mut p.personas {
        x.id = x.id.trim().to_string();
        x.name = x.name.trim().to_string();
        if x.id.is_empty() {
            x.id = slug(&x.name);
        }
        if x.id.is_empty() || x.name.is_empty() {
            return Err("Every persona needs a name.".into());
        }
        if !seen.insert(x.id.clone()) {
            return Err(format!("Two personas share the id '{}'.", x.id));
        }
        if x.ai.as_deref().is_some_and(|a| a.trim().is_empty()) {
            x.ai = None;
        }
        if let Some(a) = &x.ai {
            if providers::find(a).is_none() {
                return Err(format!("'{}' prefers an unknown AI '{a}'.", x.name));
            }
        }
    }
    Ok(p)
}

fn read_pack(path: &Path) -> Option<Pack> {
    let s = std::fs::read_to_string(path).ok()?;
    validate(serde_json::from_str(&s).ok()?).ok()
}

/// Every pack this run can use: built-ins, then the person's own, lab seats removed when hidden.
#[derive(Debug, Clone, Default)]
pub struct Library {
    pub packs: Vec<Pack>,
}

impl Library {
    pub fn load(settings: &Settings) -> Library {
        let mut packs = builtin_packs();
        for dir in user_dirs() {
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut files: Vec<_> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect();
            files.sort();
            for f in files {
                if let Some(p) = read_pack(&f) {
                    if !packs.iter().any(|q| q.id == p.id) {
                        packs.push(p);
                    }
                }
            }
        }
        Self::from_packs(packs, labs_hidden(settings))
    }

    pub fn from_packs(mut packs: Vec<Pack>, hide_labs: bool) -> Library {
        if hide_labs {
            packs.retain(|p| !p.labs);
            for p in &mut packs {
                p.personas.retain(|x| x.lab.is_none());
            }
        }
        Library { packs }
    }

    pub fn persona(&self, key: &str) -> Option<&Persona> {
        let all = || self.packs.iter().flat_map(|p| &p.personas);
        let k = key.trim();
        let ks = slug(k);
        all()
            .find(|x| x.id == k)
            .or_else(|| all().find(|x| x.name.eq_ignore_ascii_case(k)))
            .or_else(|| all().find(|x| x.id == ks || slug(&x.name) == ks))
    }

    pub fn preset(&self, id: &str) -> Option<&Preset> {
        self.packs
            .iter()
            .flat_map(|p| &p.presets)
            .find(|x| x.id == id.trim())
    }

    pub fn pack(&self, id: &str) -> Option<&Pack> {
        self.packs.iter().find(|p| p.id == id.trim())
    }
}

/// Lower-case words joined by dashes: "Head of Sales" -> "head-of-sales".
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// One place at the table: an AI, and the persona sitting on it (None is Generic).
#[derive(Debug, Clone)]
pub struct Seat {
    pub head: &'static Head,
    pub persona: Option<Persona>,
}

impl Seat {
    pub fn generic(head: &'static Head) -> Seat {
        Seat {
            head,
            persona: None,
        }
    }
}

/// Today's table: one Generic seat per head.
pub fn generic_seats(heads: &[&'static Head]) -> Vec<Seat> {
    heads.iter().map(|h| Seat::generic(h)).collect()
}

/// Is this spec Generic (no personas)?
pub fn is_generic(spec: &str) -> bool {
    let s = spec.trim();
    s.is_empty() || s.eq_ignore_ascii_case(GENERIC)
}

/// Turn a `--personas` value into seats on the given heads.
///
/// The value is a comma list. Each item is a preset id, a pack id, a persona id or name,
/// or `generic`, and may end in `@<head>` to fix the AI. A persona sits on its preferred AI
/// when that AI is at the table; every other seat goes to the least-used AI, in head order.
pub fn resolve(spec: &str, heads: &[&'static Head], lib: &Library) -> Result<Vec<Seat>, String> {
    if is_generic(spec) {
        return Ok(generic_seats(heads));
    }
    if heads.is_empty() {
        return Err("No AI is at the table to seat the personas.".into());
    }
    // (persona, forced head)
    let mut want: Vec<(Option<Persona>, Option<&'static Head>)> = Vec::new();
    for item in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (key, at) = match item.rsplit_once('@') {
            Some((k, h)) => (k.trim(), Some(h.trim())),
            None => (item, None),
        };
        let forced = match at {
            Some(h) => {
                Some(providers::find(h).ok_or_else(|| format!("Unknown AI '{h}' in '{item}'."))?)
            }
            None => None,
        };
        if key.eq_ignore_ascii_case(GENERIC) {
            want.push((None, forced));
        } else if let Some(pr) = lib.preset(key) {
            for s in &pr.seats {
                let p = lib.persona(s).ok_or_else(|| {
                    format!("The preset '{}' names an unknown persona '{s}'.", pr.id)
                })?;
                want.push((Some(p.clone()), forced));
            }
        } else if let Some(pk) = lib.pack(key) {
            want.extend(pk.personas.iter().map(|p| (Some(p.clone()), forced)));
        } else if let Some(p) = lib.persona(key) {
            want.push((Some(p.clone()), forced));
        } else {
            return Err(format!(
                "Unknown persona '{key}'. Run `roundtable personas` to see them."
            ));
        }
    }
    if want.len() > MAX_SEATS {
        return Err(format!(
            "That is {} seats. The most is {MAX_SEATS}.",
            want.len()
        ));
    }
    let mut seats: Vec<Option<&'static Head>> = want
        .iter()
        .map(|(p, f)| {
            f.or_else(|| {
                p.as_ref()
                    .and_then(|p| p.ai.as_deref())
                    .and_then(|a| heads.iter().find(|h| h.id == a).copied())
            })
        })
        .collect();
    for i in 0..seats.len() {
        if seats[i].is_none() {
            let used = |h: &Head| {
                seats
                    .iter()
                    .filter(|s| s.is_some_and(|x| x.id == h.id))
                    .count()
            };
            let pick = heads.iter().copied().min_by_key(|h| used(h)).unwrap();
            seats[i] = Some(pick);
        }
    }
    Ok(want
        .into_iter()
        .zip(seats)
        .map(|((persona, _), h)| Seat {
            head: h.unwrap(),
            persona,
        })
        .collect())
}

/// One key per seat, stable across a run and its retries: the head id on a Generic
/// seat, `<persona>@<head>` on a persona seat, with `#2`, `#3` on repeats.
pub fn seat_keys(seats: &[Seat]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in seats {
        let base = match &s.persona {
            Some(p) => format!("{}@{}", p.id, s.head.id),
            None => s.head.id.to_string(),
        };
        let mut k = base.clone();
        let mut n = 2;
        while out.contains(&k) {
            k = format!("{base}#{n}");
            n += 1;
        }
        out.push(k);
    }
    out
}

/// The seat spec that puts exactly these seats at the table again.
pub fn seats_spec(seats: &[Seat]) -> String {
    seats
        .iter()
        .map(|s| match &s.persona {
            Some(p) => format!("{}@{}", p.id, s.head.id),
            None => format!("{GENERIC}@{}", s.head.id),
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// Everything the app lists: packs, whether the lab pack is built and shown, the custom pack's id.
pub fn library_json(settings: &Settings) -> serde_json::Value {
    let lib = Library::load(settings);
    serde_json::json!({
        "packs": lib.packs,
        "lab_pack_built": lab_pack_built(),
        "labs_hidden": labs_hidden(settings),
        "custom_pack": CUSTOM_PACK_ID,
    })
}

/// The block a persona seat reads before the question.
pub fn seat_brief(p: &Persona) -> String {
    let mut s = format!("## YOUR SEAT\nYou sit at this roundtable as {}", p.name);
    if !p.expertise.trim().is_empty() {
        s.push_str(&format!(" ({})", p.expertise.trim()));
    }
    s.push_str(".\n");
    if !p.brief.trim().is_empty() {
        s.push_str(p.brief.trim());
        s.push('\n');
    }
    s.push_str("Answer from this seat. Stay in role, and say so plainly when a point is outside this seat's expertise.\n");
    if let Some(lab) = &p.lab {
        s.push_str(&format!(
            "This is a simulated perspective drawn from public positions. Do not claim to speak for {lab} or for any person, and do not invent quotes.\n"
        ));
    }
    s.push('\n');
    s
}

/// The prompt one seat gets: its brief, then the shared prompt.
pub fn seat_prompt(seat: &Seat, prompt: &str) -> String {
    match &seat.persona {
        Some(p) => format!("{}{prompt}", seat_brief(p)),
        None => prompt.to_string(),
    }
}

/// Transcript and table label: "CFO · 🔵 Claude (...)", or the head label alone.
pub fn seat_label(seat: &Seat, head_label: &str) -> String {
    match &seat.persona {
        Some(p) => format!("{} \u{b7} {head_label}", p.name),
        None => head_label.to_string(),
    }
}

/// The lab label for a seat, when it has one.
pub fn seat_disclaimer(seat: &Seat) -> Option<String> {
    seat.persona
        .as_ref()
        .and_then(|p| p.lab.as_deref())
        .map(disclaimer)
}

// ---- The person's own personas (Settings, Personas in the app) ----

fn read_custom_at(path: &Path) -> Pack {
    read_pack(path).unwrap_or_else(|| Pack {
        id: CUSTOM_PACK_ID.into(),
        name: "My personas".into(),
        ..Pack::default()
    })
}

fn write_pack_at(path: &Path, p: &Pack) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(
        &tmp,
        serde_json::to_string_pretty(p).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(tmp, path).map_err(|e| e.to_string())
}

/// A free id near `want` that no pack in `lib` or `taken` uses.
fn free_id(want: &str, lib: &Library, taken: &[Persona]) -> String {
    let base = if want.is_empty() {
        "persona".to_string()
    } else {
        want.to_string()
    };
    let used =
        |id: &str| lib.persona(id).is_some_and(|p| p.id == id) || taken.iter().any(|p| p.id == id);
    if !used(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|c| !used(c))
        .unwrap()
}

/// Create or update one of the person's own personas. A new one (empty id) gets an id
/// from its name. Returns the saved persona.
pub fn save_custom_at(path: &Path, lib: &Library, mut p: Persona) -> Result<Persona, String> {
    p.name = p.name.trim().to_string();
    if p.name.is_empty() {
        return Err("Give the persona a name.".into());
    }
    let mut pack = read_custom_at(path);
    if p.id.trim().is_empty() || !pack.personas.iter().any(|x| x.id == p.id) {
        let others: Vec<Persona> = pack.personas.clone();
        let want = if p.id.trim().is_empty() {
            slug(&p.name)
        } else {
            slug(&p.id)
        };
        p.id = free_id(&want, lib, &others);
        pack.personas.push(p.clone());
    } else if let Some(x) = pack.personas.iter_mut().find(|x| x.id == p.id) {
        *x = p.clone();
    }
    let pack = validate(pack)?;
    write_pack_at(path, &pack)?;
    Ok(p)
}

/// Copy any persona (built-in or not) into the person's own pack as "<name> (copy)".
pub fn duplicate_at(path: &Path, lib: &Library, id: &str) -> Result<Persona, String> {
    let src = lib
        .persona(id)
        .cloned()
        .or_else(|| {
            read_custom_at(path)
                .personas
                .into_iter()
                .find(|x| x.id == id)
        })
        .ok_or("That persona is gone.")?;
    let copy = Persona {
        id: String::new(),
        name: format!("{} (copy)", src.name),
        ..src
    };
    save_custom_at(path, lib, copy)
}

pub fn delete_custom_at(path: &Path, id: &str) -> Result<(), String> {
    let mut pack = read_custom_at(path);
    let n = pack.personas.len();
    pack.personas.retain(|x| x.id != id);
    if pack.personas.len() == n {
        return Err("Only your own personas can be deleted.".into());
    }
    for pr in &mut pack.presets {
        pr.seats.retain(|s| s != id);
    }
    write_pack_at(path, &pack)
}

/// Write the person's own pack to a file they can share.
pub fn export_at(path: &Path, to: &Path) -> Result<usize, String> {
    let pack = read_custom_at(path);
    write_pack_at(to, &pack)?;
    Ok(pack.personas.len())
}

/// Add every persona from a pack file to the person's own pack. Ids that clash get a suffix.
/// Lab seats keep their label. Returns how many were added.
pub fn import_at(path: &Path, lib: &Library, from: &Path) -> Result<usize, String> {
    let s = std::fs::read_to_string(from).map_err(|e| format!("Could not read that file: {e}"))?;
    let incoming = validate(
        serde_json::from_str::<Pack>(&s)
            .map_err(|_| "That file is not a persona pack.".to_string())?,
    )?;
    let mut pack = read_custom_at(path);
    let mut renamed = std::collections::BTreeMap::new();
    for mut p in incoming.personas {
        let old = p.id.clone();
        p.id = free_id(&slug(&p.id), lib, &pack.personas);
        if incoming.labs && p.lab.is_none() {
            p.lab = Some(incoming.name.clone());
        }
        renamed.insert(old, p.id.clone());
        pack.personas.push(p);
    }
    for mut pr in incoming.presets {
        pr.seats = pr
            .seats
            .iter()
            .map(|s| renamed.get(s).cloned().unwrap_or_else(|| s.clone()))
            .collect();
        if !pack.presets.iter().any(|x| x.id == pr.id) && lib.preset(&pr.id).is_none() {
            pack.presets.push(pr);
        }
    }
    let n = renamed.len();
    write_pack_at(path, &validate(pack)?)?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib(hide: bool) -> Library {
        Library::from_packs(builtin_packs(), hide)
    }

    fn heads(csv: &str) -> Vec<&'static Head> {
        csv.split(',')
            .map(|h| providers::find(h).unwrap())
            .collect()
    }

    #[test]
    fn builtin_packs_load_and_presets_resolve() {
        let l = lib(false);
        let ids: Vec<_> = l.packs.iter().map(|p| p.id.as_str()).collect();
        if lab_pack_built() {
            assert_eq!(
                ids,
                vec!["frontier-labs", "business-roles", "expert-panels"]
            );
        } else {
            assert_eq!(ids, vec!["business-roles", "expert-panels"]);
            assert!(labs_hidden(&Settings::default()));
        }
        for p in &l.packs {
            for pr in &p.presets {
                assert!((2..=8).contains(&pr.seats.len()), "{}", pr.id);
                for s in &pr.seats {
                    assert!(l.persona(s).is_some(), "{} names {s}", pr.id);
                }
            }
        }
        for id in [
            "security-review",
            "go-to-market",
            "hiring-decision",
            "pricing",
            "architecture-review",
        ] {
            let n = l.preset(id).unwrap().seats.len();
            assert!((3..=5).contains(&n), "{id} has {n}");
        }
        let roles: Vec<_> = l
            .pack("business-roles")
            .unwrap()
            .personas
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(roles.len(), 12);
    }

    #[cfg(feature = "lab-pack")]
    #[test]
    fn lab_seats_name_no_person_and_carry_the_label() {
        let l = lib(false);
        let labs = l.pack("frontier-labs").unwrap();
        assert!(labs.labs);
        for p in &labs.personas {
            let lab = p.lab.as_deref().unwrap();
            assert!(
                p.name.starts_with("The ") && p.name.ends_with(" view"),
                "{}",
                p.name
            );
            let text = format!("{} {} {}", p.name, p.expertise, p.brief).to_lowercase();
            let words: Vec<&str> = text.split(|c: char| !c.is_ascii_alphabetic()).collect();
            for who in [
                "jensen",
                "huang",
                "altman",
                "sam",
                "musk",
                "elon",
                "amodei",
                "dario",
                "hassabis",
                "demis",
                "nadella",
                "satya",
                "lisa",
                "zuckerberg",
                "mark",
                "suleyman",
                "mustafa",
            ] {
                assert!(!words.contains(&who), "{} mentions {who}", p.id);
            }
            assert!(
                !words.contains(&"we") && !words.contains(&"our"),
                "{} speaks as the lab",
                p.id
            );
            let seat = Seat {
                head: providers::find("claude").unwrap(),
                persona: Some(p.clone()),
            };
            assert_eq!(seat_disclaimer(&seat).unwrap(), format!("Simulated perspective drawn from public positions. Not affiliated with or endorsed by {lab}."));
            assert!(seat_prompt(&seat, "Q").contains(&format!("Do not claim to speak for {lab}")));
        }
        for n in [
            "The NVIDIA view",
            "The OpenAI view",
            "The xAI view",
            "The Anthropic view",
        ] {
            assert!(l.persona(n).is_some(), "{n}");
        }
    }

    #[test]
    fn the_switch_hides_the_lab_pack() {
        let l = lib(true);
        assert!(l.pack("frontier-labs").is_none());
        assert!(l.persona("nvidia-view").is_none());
        assert!(resolve("frontier-four", &heads("claude"), &l)
            .unwrap_err()
            .contains("Unknown persona"));
        assert!(resolve("The NVIDIA view", &heads("claude"), &l).is_err());
        let mut s = Settings::default();
        assert!(s.show_lab_pack);
        s.show_lab_pack = false;
        assert!(labs_hidden(&s));
        // A lab seat a person imported is hidden too.
        let mine = Pack {
            id: "x".into(),
            name: "x".into(),
            personas: vec![Persona {
                id: "v".into(),
                name: "V".into(),
                lab: Some("Lab".into()),
                ..Persona::default()
            }],
            ..Pack::default()
        };
        assert!(Library::from_packs(vec![mine], true).persona("v").is_none());
    }

    #[test]
    fn generic_is_todays_table() {
        let s = resolve("generic", &heads("claude,gemini"), &lib(false)).unwrap();
        assert_eq!(s.len(), 2);
        assert!(s.iter().all(|x| x.persona.is_none()));
        assert!(resolve("", &heads("claude"), &lib(false)).unwrap()[0]
            .persona
            .is_none());
        assert_eq!(seat_prompt(&s[0], "Q"), "Q");
        assert_eq!(seat_label(&s[0], "L"), "L");
    }

    #[test]
    fn seats_spread_over_the_ais_and_honour_preferences() {
        let l = lib(false);
        let s = resolve("exec-team", &heads("claude,gemini"), &l).unwrap();
        let got: Vec<_> = s
            .iter()
            .map(|x| (x.persona.as_ref().unwrap().id.as_str(), x.head.id))
            .collect();
        assert_eq!(
            got,
            vec![
                ("ceo", "claude"),
                ("cto", "gemini"),
                ("cfo", "claude"),
                ("coo", "gemini")
            ]
        );
        let s = resolve(
            "CFO, Head of Sales@gemini ,generic",
            &heads("claude,gemini"),
            &l,
        )
        .unwrap();
        assert_eq!(s[0].head.id, "claude");
        assert_eq!(s[1].head.id, "gemini");
        assert_eq!(s[1].persona.as_ref().unwrap().id, "head-of-sales");
        assert!(s[2].persona.is_none());
        // Two personas on one AI.
        let s = resolve("cfo,ceo", &heads("claude"), &l).unwrap();
        assert!(s.iter().all(|x| x.head.id == "claude"));
        let mut lp = l.clone();
        lp.packs
            .iter_mut()
            .find(|p| p.id == "business-roles")
            .unwrap()
            .personas[0]
            .ai = Some("gemini".into());
        let s = resolve("ceo,cto", &heads("claude,gemini"), &lp).unwrap();
        assert_eq!((s[0].head.id, s[1].head.id), ("gemini", "claude"));
        assert!(resolve("nobody", &heads("claude"), &l).is_err());
        assert!(resolve("cfo@nobody", &heads("claude"), &l).is_err());
    }

    #[test]
    fn prompt_and_label() {
        let l = lib(false);
        let s = &resolve("cfo", &heads("claude"), &l).unwrap()[0];
        let p = seat_prompt(s, "## QUESTION\nRaise prices?");
        assert!(p.starts_with("## YOUR SEAT\nYou sit at this roundtable as CFO (Money, risk and return).\nReason in numbers"));
        assert!(p.ends_with("expertise.\n\n## QUESTION\nRaise prices?"));
        assert!(!p.contains("simulated"));
        assert_eq!(
            seat_label(s, "\u{1f535} Claude"),
            "CFO \u{b7} \u{1f535} Claude"
        );
        assert!(seat_disclaimer(s).is_none());
    }

    #[test]
    fn custom_crud_export_import() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("personas/custom.json");
        let l = lib(false);
        let a = save_custom_at(
            &f,
            &l,
            Persona {
                name: "Ops nerd".into(),
                expertise: "Uptime".into(),
                brief: "Count nines.".into(),
                ..Persona::default()
            },
        )
        .unwrap();
        assert_eq!(a.id, "ops-nerd");
        let mut b = a.clone();
        b.brief = "Count nines twice.".into();
        b.ai = Some("gemini".into());
        save_custom_at(&f, &l, b).unwrap();
        let c = duplicate_at(&f, &l, "cfo").unwrap();
        assert_eq!((c.id.as_str(), c.name.as_str()), ("cfo-copy", "CFO (copy)"));
        let pack = read_custom_at(&f);
        assert_eq!(pack.personas.len(), 2);
        assert_eq!(pack.personas[0].brief, "Count nines twice.");
        assert!(save_custom_at(
            &f,
            &l,
            Persona {
                name: " ".into(),
                ..Persona::default()
            }
        )
        .is_err());
        assert!(save_custom_at(
            &f,
            &l,
            Persona {
                name: "X".into(),
                ai: Some("nope".into()),
                ..Persona::default()
            }
        )
        .is_err());
        // A new persona named like a built-in gets its own id.
        assert_eq!(
            save_custom_at(
                &f,
                &l,
                Persona {
                    name: "CFO".into(),
                    ..Persona::default()
                }
            )
            .unwrap()
            .id,
            "cfo-2"
        );
        assert!(
            delete_custom_at(&f, "cfo").is_err(),
            "built-ins are not deleted"
        );
        delete_custom_at(&f, "cfo-2").unwrap();
        let out = d.path().join("share.json");
        assert_eq!(export_at(&f, &out).unwrap(), 2);
        let f2 = d.path().join("other/custom.json");
        assert_eq!(import_at(&f2, &l, &out).unwrap(), 2);
        assert_eq!(import_at(&f2, &l, &out).unwrap(), 2);
        let ids: Vec<_> = read_custom_at(&f2)
            .personas
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(
            ids,
            vec!["ops-nerd", "cfo-copy", "ops-nerd-2", "cfo-copy-2"]
        );
        std::fs::write(d.path().join("bad.json"), "{}").unwrap();
        assert!(import_at(&f2, &l, &d.path().join("bad.json")).is_err());
        delete_custom_at(&f2, "ops-nerd").unwrap();
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Head of Sales"), "head-of-sales");
        assert_eq!(slug("  The NVIDIA view! "), "the-nvidia-view");
    }
}
