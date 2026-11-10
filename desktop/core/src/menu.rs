//! The native menu bar: the app menu, Edit, View, Window and Help on a Mac; File, View and
//! Help on Windows. Help opens the built-in pages (docs/app in the repo, bundled in the UI).
//!
//! Quit and full screen run here. Every other item is sent to the UI as a `menu` event with
//! the item's id, and the UI acts on it (see ui/src/help.ts, `onMenu`).

use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// Help pages in the Help menu, in order: (menu id, label). Each id `help:<page>` has its
/// page at `docs/app/<page>.md` (a test checks it).
pub const HELP_PAGES: &[(&str, &str)] = &[
    ("help:getting-started", "Getting started"),
    ("help:connect-your-ais", "Connect your AIs"),
    ("help:personas-and-rounds", "Personas and rounds"),
    ("help:deep-think", "Deep Think and Quick"),
    ("help:decision-model", "Decision model and Jebadiah"),
    ("help:use-from-your-ai", "Use Roundtable from your AI"),
    ("help:keyboard-shortcuts", "Keyboard shortcuts"),
];

/// The event the UI listens for.
pub const MENU_EVENT: &str = "menu";

/// One line in a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// Our own item: id, label, shortcut.
    Text(&'static str, &'static str, Option<&'static str>),
    Separator,
    /// One of the system's own items (cut, copy, hide...), by name.
    System(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submenu {
    pub title: &'static str,
    pub items: Vec<Item>,
}

/// The menu bar for one OS: "macos" or anything else (Windows).
pub fn plan(os: &str) -> Vec<Submenu> {
    use Item::*;
    let mac = os == "macos";
    let mut help = vec![Text("help:overview", "Roundtable Help", Some(if mac { "CmdOrCtrl+Shift+/" } else { "F1" })), Separator];
    help.extend(HELP_PAGES.iter().map(|(id, label)| Text(id, label, None)));
    help.extend([
        Separator,
        Text("whats-new", "What's new", None),
        Text("online-docs", "Online docs", None),
        Text("report", "Report a problem", None),
    ]);
    if mac {
        let app = Submenu {
            title: "Roundtable",
            items: vec![
                Text("about", "About Roundtable", None),
                Text("check-updates", "Check for updates…", None),
                Separator,
                Text("settings", "Settings…", Some("CmdOrCtrl+,")),
                Separator,
                System("services"),
                Separator,
                System("hide"),
                System("hide_others"),
                System("show_all"),
                Separator,
                Text("quit", "Quit Roundtable", Some("CmdOrCtrl+Q")),
            ],
        };
        // About sits in the app menu on a Mac, as Mac apps do.
        vec![
            app,
            Submenu {
                title: "Edit",
                items: vec![System("undo"), System("redo"), Separator, System("cut"), System("copy"), System("paste"), System("select_all")],
            },
            Submenu { title: "View", items: vec![Text("show-history", "Past roundtables", None), Separator, System("fullscreen")] },
            Submenu { title: "Window", items: vec![System("minimize"), System("maximize"), Separator, System("close_window")] },
            Submenu { title: "Help", items: help },
        ]
    } else {
        // Windows: no Edit menu, so Ctrl+C and the rest stay with the page (WebView2 handles them).
        help.extend([Separator, Text("about", "About Roundtable", None)]);
        vec![
            Submenu {
                title: "&File",
                items: vec![
                    Text("settings", "&Settings", Some("Ctrl+,")),
                    Text("check-updates", "Check for &updates", None),
                    Separator,
                    Text("quit", "E&xit", None),
                ],
            },
            Submenu {
                title: "&View",
                items: vec![Text("show-history", "&Past roundtables", None), Separator, Text("fullscreen", "&Full screen", Some("F11"))],
            },
            Submenu { title: "&Help", items: help },
        ]
    }
}

/// Builds the menu bar for this OS.
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let mut bar = MenuBuilder::new(app);
    for sub in plan(std::env::consts::OS) {
        let mut b = SubmenuBuilder::new(app, sub.title);
        for item in &sub.items {
            b = match item {
                Item::Separator => b.separator(),
                Item::Text(id, label, accel) => {
                    let mut i = MenuItemBuilder::with_id(*id, *label);
                    if let Some(a) = accel {
                        i = i.accelerator(*a);
                    }
                    b.item(&i.build(app)?)
                }
                Item::System(name) => match *name {
                    "services" => b.services(),
                    "hide" => b.hide(),
                    "hide_others" => b.hide_others(),
                    "show_all" => b.show_all(),
                    "undo" => b.undo(),
                    "redo" => b.redo(),
                    "cut" => b.cut(),
                    "copy" => b.copy(),
                    "paste" => b.paste(),
                    "select_all" => b.select_all(),
                    "fullscreen" => b.fullscreen(),
                    "minimize" => b.minimize(),
                    "maximize" => b.maximize(),
                    "close_window" => b.close_window(),
                    other => unreachable!("no system menu item {other}"),
                },
            };
        }
        let built = b.build()?;
        #[cfg(target_os = "macos")]
        {
            if sub.title == "Help" {
                built.set_as_help_menu_for_nsapp()?;
            }
            if sub.title == "Window" {
                built.set_as_windows_menu_for_nsapp()?;
            }
        }
        bar = bar.item(&built);
    }
    bar.build()
}

/// What a menu click does. Quit and full screen run here; the rest goes to the UI.
pub fn handle<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        // app.exit runs RunEvent::Exit, so an update set to install on quit still installs.
        "quit" => app.exit(0),
        "fullscreen" => {
            if let Some(w) = app.get_webview_window("main") {
                let on = w.is_fullscreen().unwrap_or(false);
                let _ = w.set_fullscreen(!on);
            }
        }
        _ => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
            let _ = app.emit(MENU_EVENT, id.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::Path;

    fn ids(subs: &[Submenu]) -> Vec<&'static str> {
        subs.iter()
            .flat_map(|s| s.items.iter())
            .filter_map(|i| match i {
                Item::Text(id, _, _) => Some(*id),
                _ => None,
            })
            .collect()
    }

    fn help(subs: &[Submenu]) -> &Submenu {
        subs.iter().find(|s| s.title.replace('&', "") == "Help").expect("a Help menu")
    }

    #[test]
    fn mac_has_the_five_menus_in_order() {
        let p = plan("macos");
        let titles: Vec<_> = p.iter().map(|s| s.title).collect();
        assert_eq!(titles, ["Roundtable", "Edit", "View", "Window", "Help"]);
        let app_ids = ids(&p[..1]);
        assert_eq!(app_ids, ["about", "check-updates", "settings", "quit"]);
        assert!(p[1].items.contains(&Item::System("copy")) && p[1].items.contains(&Item::System("paste")));
    }

    #[test]
    fn windows_has_file_view_help_and_no_edit_menu() {
        let p = plan("windows");
        let titles: Vec<_> = p.iter().map(|s| s.title.replace('&', "")).collect();
        assert_eq!(titles, ["File", "View", "Help"]);
        // Only items that exist on Windows: no Services, Hide or system full screen.
        assert!(p.iter().flat_map(|s| &s.items).all(|i| !matches!(i, Item::System(_))));
        assert_eq!(ids(&p).last(), Some(&"about"));
    }

    #[test]
    fn help_lists_every_page_then_whats_new_online_docs_and_report() {
        for os in ["macos", "windows"] {
            let h = help(&plan(os)).clone();
            let got = ids(std::slice::from_ref(&h));
            let mut want = vec!["help:overview"];
            want.extend(HELP_PAGES.iter().map(|(id, _)| *id));
            want.extend(["whats-new", "online-docs", "report"]);
            if os == "windows" {
                want.push("about");
            }
            assert_eq!(got, want, "{os}");
        }
    }

    #[test]
    fn every_help_page_has_its_markdown_file() {
        let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/app");
        for (id, label) in HELP_PAGES.iter().chain([("help:overview", "Roundtable Help")].iter()) {
            let page = id.strip_prefix("help:").unwrap();
            let file = docs.join(format!("{page}.md"));
            let text = std::fs::read_to_string(&file).unwrap_or_else(|_| panic!("{} is missing", file.display()));
            assert!(text.contains(&format!("title: {label}")), "{page}.md title should be {label}");
        }
    }

    #[test]
    fn ids_and_shortcuts_are_unique_and_settings_has_its_shortcut() {
        for os in ["macos", "windows"] {
            let p = plan(os);
            let all = ids(&p);
            assert_eq!(all.len(), all.iter().collect::<HashSet<_>>().len(), "{os}: an id twice");
            let accels: Vec<_> = p
                .iter()
                .flat_map(|s| &s.items)
                .filter_map(|i| match i {
                    Item::Text(_, _, Some(a)) => Some(*a),
                    _ => None,
                })
                .collect();
            assert_eq!(accels.len(), accels.iter().collect::<HashSet<_>>().len(), "{os}: a shortcut twice");
            assert!(p.iter().flat_map(|s| &s.items).any(|i| matches!(i, Item::Text("settings", _, Some(a)) if a.ends_with(','))));
        }
    }

    #[test]
    fn labels_have_no_em_dash() {
        for os in ["macos", "windows"] {
            for s in plan(os) {
                for i in s.items {
                    if let Item::Text(_, label, _) = i {
                        assert!(!label.contains('\u{2014}'), "{label}");
                    }
                }
            }
        }
    }

    /// Every shortcut parses the way the menu parses it (muda, the crate under Tauri's menus).
    /// Building the real menu needs the main thread on a Mac, which a test does not have.
    #[test]
    fn every_shortcut_parses() {
        use std::str::FromStr;
        for os in ["macos", "windows"] {
            for s in plan(os) {
                for i in s.items {
                    if let Item::Text(id, _, Some(a)) = i {
                        muda::accelerator::Accelerator::from_str(a).unwrap_or_else(|e| panic!("{os} {id}: {a}: {e}"));
                    }
                }
            }
        }
    }
}
