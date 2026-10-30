//! UI translations.
//!
//! The English text in the code is the key: `tr("Layers")` returns the translation for the
//! current language, or the English text when there is none, so untranslated strings still
//! read correctly. Catalogs are tab-separated files under `assets/i18n/`, one per language,
//! embedded at build time. Translation happens where text is drawn (menus, section titles,
//! tabs, tooltips), so code that matches on labels keeps working with English strings.
//!
//! Adding a language: add a variant to [`Lang`], a catalog file, and its arm in
//! [`Lang::catalog`]. Adding strings: add `English<TAB>translation` lines to the catalogs.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Lang {
    #[default]
    En,
    ZhCn,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::ZhCn];

    /// BCP 47 code stored in the settings.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::ZhCn => "zh-CN",
        }
    }

    /// The language's own name, as shown in the Language menu (never translated).
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::ZhCn => "简体中文",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        let c = code.trim().replace('_', "-").to_ascii_lowercase();
        if c == "en" || c.starts_with("en-") {
            Some(Lang::En)
        } else if c == "zh" || c.starts_with("zh-cn") || c.starts_with("zh-hans") || c.starts_with("zh-sg") {
            Some(Lang::ZhCn)
        } else {
            None
        }
    }

    fn catalog(self) -> Option<&'static HashMap<&'static str, &'static str>> {
        static ZH_CN: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
        match self {
            Lang::En => None,
            Lang::ZhCn => Some(ZH_CN.get_or_init(|| parse(include_str!("../assets/i18n/zh-CN.tsv")))),
        }
    }
}

/// The language from the environment (`LC_ALL`, `LC_MESSAGES`, `LANG`), for a first launch
/// without a saved choice. `C`/`POSIX` and unknown languages give `None`.
pub fn from_env() -> Option<Lang> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty())
        .and_then(|v| Lang::from_code(v.split('.').next().unwrap_or("")))
}

thread_local! {
    // UI text is laid out on the UI thread; a thread-local keeps parallel tests independent.
    static CURRENT: Cell<Lang> = const { Cell::new(Lang::En) };
}

pub fn current() -> Lang {
    CURRENT.with(Cell::get)
}

pub fn set(lang: Lang) {
    CURRENT.with(|c| c.set(lang));
}

/// `text` in the current language (unchanged when there is no translation).
pub fn tr(text: &str) -> &str {
    tr_in(current(), text)
}

/// `text` in `lang`.
pub fn tr_in(lang: Lang, text: &str) -> &str {
    lang.catalog().and_then(|c| c.get(text).copied()).unwrap_or(text)
}

/// `English<TAB>translation` lines; `#` comments and blank lines are skipped.
fn parse(src: &'static str) -> HashMap<&'static str, &'static str> {
    src.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('\t'))
        .map(|(en, tr)| (en, tr.trim()))
        .filter(|(_, tr)| !tr.is_empty())
        .collect()
}

/// Settings key holding the chosen language code.
pub const SETTING: &str = "language";

/// The language to start in: the saved choice, else (on an interactive launch) the system
/// language when we have a catalog for it, else English.
pub fn startup(library: Option<&std::path::Path>, use_env: bool) -> Lang {
    library
        .and_then(|root| crate::comments::settings::load_string(root, SETTING))
        .and_then(|code| Lang::from_code(&code))
        .or_else(|| if use_env { from_env() } else { None })
        .unwrap_or_default()
}

impl crate::app::App {
    /// Preferences -> Language: switches the UI now and remembers the choice.
    pub fn set_language(&mut self, lang: Lang) {
        set(lang);
        if let Some(lib) = &self.lib {
            if let Err(e) = crate::comments::settings::save_string(lib.root(), SETTING, lang.code()) {
                self.show_toast(e, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZH: &str = include_str!("../assets/i18n/zh-CN.tsv");

    #[test]
    fn translates_and_falls_back_to_english() {
        assert_eq!(tr_in(Lang::ZhCn, "Layers"), "图层");
        assert_eq!(tr_in(Lang::ZhCn, "Not in the catalog"), "Not in the catalog");
        assert_eq!(tr_in(Lang::En, "Layers"), "Layers");
        // the current language is per thread and English by default
        assert_eq!(tr("Layers"), "Layers");
        set(Lang::ZhCn);
        assert_eq!(tr("Layers"), "图层");
        set(Lang::En);
    }

    #[test]
    fn language_codes() {
        for l in Lang::ALL {
            assert_eq!(Lang::from_code(l.code()), Some(l));
        }
        assert_eq!(Lang::from_code("zh_CN.UTF-8".split('.').next().unwrap()), Some(Lang::ZhCn));
        assert_eq!(Lang::from_code("zh-Hans"), Some(Lang::ZhCn));
        assert_eq!(Lang::from_code("en_US"), Some(Lang::En));
        assert_eq!(Lang::from_code("C"), None);
        assert_eq!(Lang::from_code("de"), None);
    }

    #[test]
    fn saved_choice_wins_over_the_environment() {
        let dir = std::env::temp_dir().join(format!("desigma-i18n-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(startup(Some(&dir), false), Lang::En, "nothing saved, environment ignored");
        crate::comments::settings::save_string(&dir, SETTING, "zh-CN").unwrap();
        assert_eq!(startup(Some(&dir), false), Lang::ZhCn);
        assert_eq!(startup(None, false), Lang::En);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn catalog_lines_are_well_formed_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for (i, line) in ZH.lines().enumerate() {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let (en, zh) = line.split_once('\t').unwrap_or_else(|| panic!("line {}: no TAB: {line:?}", i + 1));
            assert!(!en.is_empty() && !zh.trim().is_empty() && !zh.contains('\t'), "line {}: {line:?}", i + 1);
            assert_eq!(en, en.trim(), "line {}: stray spaces around the English key", i + 1);
            assert!(seen.insert(en), "line {}: duplicate key {en:?}", i + 1);
        }
    }

    /// The first translated surface: every main-menu, context-menu and tool label has a
    /// zh-CN entry, so new menu items are not left in English unnoticed.
    #[test]
    fn menus_and_tools_are_translated() {
        let menus = include_str!("panels/menus.rs");
        let mut missing = vec![];
        for call in ["leaf(\"", "submenu(\"", "stub_sub(\"", "(\"Select all with same "] {
            for (pos, _) in menus.match_indices(call) {
                let start = pos + call.len() - if call.starts_with("(\"Select") { "Select all with same ".len() } else { 0 };
                let rest = &menus[start..];
                let label = &rest[..rest.find('"').unwrap()];
                if !label.is_empty() && tr_in(Lang::ZhCn, label) == label {
                    missing.push(label.to_string());
                }
            }
        }
        for t in crate::canvas::tools::GROUPS.iter().flat_map(|g| g.iter()) {
            if tr_in(Lang::ZhCn, t.label()) == t.label() {
                missing.push(t.label().to_string());
            }
        }
        for t in crate::canvas::tools::GROUPS.iter().flat_map(|g| g.iter()) {
            let l = crate::panels::menus::tool_menu_label(*t);
            if tr_in(Lang::ZhCn, l) == l {
                missing.push(l.to_string());
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "add these to assets/i18n/zh-CN.tsv: {missing:?}");
    }
}
