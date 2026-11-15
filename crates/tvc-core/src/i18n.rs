//! Translations for text built in Rust (the window's status lines, results, OSD and setup cards). Text written in
//! `.slint` files goes through Slint's own `@tr()` with the bundled `tvc-app.po` catalogs; this module reads the
//! matching `tvc.po` catalogs from the same `translations/<lang>/LC_MESSAGES/` folders, embedded at build time.
//!
//! The command line, JSON output and log lines stay English on purpose: scripts and support threads rely on them.
//! Placeholders are positional (`{0}`, `{1}`) so a translation can reorder them.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::{OnceLock, RwLock};

/// The languages the app ships, by catalog folder name, with each name in its own language. English is the source.
pub const LANGUAGES: &[(&str, &str)] =
    &[("en", "English"), ("es", "Español"), ("zh_CN", "简体中文"), ("fr", "Français"), ("de", "Deutsch")];

const CATALOGS: &[(&str, &str)] = &[
    ("es", include_str!("../../../translations/es/LC_MESSAGES/tvc.po")),
    ("zh_CN", include_str!("../../../translations/zh_CN/LC_MESSAGES/tvc.po")),
    ("fr", include_str!("../../../translations/fr/LC_MESSAGES/tvc.po")),
    ("de", include_str!("../../../translations/de/LC_MESSAGES/tvc.po")),
];

static CURRENT: RwLock<&str> = RwLock::new("en");
static PARSED: OnceLock<HashMap<&'static str, Catalog>> = OnceLock::new();

/// msgid → its forms (one, or one per plural form).
type Catalog = HashMap<String, Vec<String>>;

/// Selects the language by code; an unknown code selects English.
pub fn set_language(code: &str) {
    let code = LANGUAGES.iter().find(|(c, _)| *c == code).map_or("en", |(c, _)| *c);
    *CURRENT.write().unwrap_or_else(|e| e.into_inner()) = code;
}

pub fn language() -> &'static str {
    *CURRENT.read().unwrap_or_else(|e| e.into_inner())
}

/// The shipped language for the OS's preferred locales, in order ("de-AT" → "de", "zh-Hans-CN" or "zh_CN" →
/// "zh_CN"); English when none matches. Traditional Chinese (zh-TW, zh-HK, zh-Hant) is not Simplified, so it falls
/// through to the next locale.
pub fn best_match(os_locales: &[String]) -> &'static str {
    for locale in os_locales {
        let l = locale.trim().replace('-', "_").to_ascii_lowercase();
        let l = l.split(['.', '@']).next().unwrap_or("");
        let base = l.split('_').next().unwrap_or("");
        if base == "zh" {
            let traditional = ["_tw", "_hk", "_mo", "_hant"].iter().any(|t| l.contains(t));
            if !traditional {
                return "zh_CN";
            }
            continue;
        }
        if let Some((code, _)) = LANGUAGES.iter().find(|(c, _)| *c == base) {
            return code;
        }
    }
    "en"
}

/// `msgid` in the current language; the English text when there is no translation.
pub fn tr(msgid: &str) -> String {
    lookup(msgid, 0).unwrap_or_else(|| msgid.to_string())
}

/// [`tr`] with `{0}`, `{1}`… replaced by `args`.
pub fn trf(msgid: &str, args: &[&dyn Display]) -> String {
    fill(&tr(msgid), args)
}

/// The singular or plural form for `n`, with `{0}`… replaced by `args`.
pub fn trn(msgid: &str, plural: &str, n: u64, args: &[&dyn Display]) -> String {
    let form = plural_form(language(), n);
    let text = lookup(msgid, form).unwrap_or_else(|| if n == 1 { msgid } else { plural }.to_string());
    fill(&text, args)
}

fn plural_form(language: &str, n: u64) -> usize {
    match language {
        "zh_CN" => 0,
        "fr" => usize::from(n > 1),
        _ => usize::from(n != 1),
    }
}

fn lookup(msgid: &str, form: usize) -> Option<String> {
    let language = language();
    if language == "en" {
        return None;
    }
    let catalogs = PARSED.get_or_init(|| CATALOGS.iter().map(|(code, text)| (*code, parse(text))).collect());
    let forms = catalogs.get(language)?.get(msgid)?;
    forms.get(form).or_else(|| forms.first()).filter(|s| !s.is_empty()).cloned()
}

fn fill(text: &str, args: &[&dyn Display]) -> String {
    let mut out = text.to_string();
    for (i, arg) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), &arg.to_string());
    }
    out
}

/// Parses a gettext `.po` file: `msgid`, `msgid_plural`, `msgstr` and `msgstr[n]`, continuation lines, C escapes.
/// The header (empty msgid), fuzzy entries and entries with a `msgctxt` are skipped; untranslated entries are kept
/// empty, so they fall back to English.
pub fn parse(text: &str) -> Catalog {
    #[derive(Default)]
    struct Entry {
        id: String,
        forms: Vec<String>,
        fuzzy: bool,
        context: bool,
    }
    enum Field {
        None,
        Id,
        Plural,
        Str(usize),
    }
    let mut catalog = Catalog::new();
    let mut entry = Entry::default();
    let mut field = Field::None;
    let finish = |entry: &mut Entry, catalog: &mut Catalog| {
        let e = std::mem::take(entry);
        if !e.id.is_empty() && !e.fuzzy && !e.context {
            catalog.insert(e.id, e.forms);
        }
    };
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            continue;
        }
        if let Some(flags) = line.strip_prefix("#,") {
            if !matches!(field, Field::None) {
                finish(&mut entry, &mut catalog);
                field = Field::None;
            }
            entry.fuzzy |= flags.split(',').any(|f| f.trim() == "fuzzy");
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        let (keyword, rest) = match line.find(' ') {
            Some(i) if !line.starts_with('"') => (&line[..i], line[i..].trim()),
            _ => ("", line),
        };
        let value = unquote(rest);
        match keyword {
            "msgctxt" => {
                if matches!(field, Field::Str(_)) {
                    finish(&mut entry, &mut catalog);
                }
                entry.context = true;
                field = Field::None;
            }
            "msgid" => {
                if matches!(field, Field::Str(_)) {
                    finish(&mut entry, &mut catalog);
                }
                entry.id = value;
                field = Field::Id;
            }
            "msgid_plural" => field = Field::Plural,
            k if k == "msgstr" || k.starts_with("msgstr[") => {
                let n = k.strip_prefix("msgstr[").and_then(|r| r.strip_suffix(']')).and_then(|n| n.parse().ok());
                let n = n.unwrap_or(0);
                if entry.forms.len() <= n {
                    entry.forms.resize(n + 1, String::new());
                }
                entry.forms[n] = value;
                field = Field::Str(n);
            }
            "" => match field {
                Field::Id => entry.id.push_str(&value),
                Field::Str(n) => entry.forms[n].push_str(&value),
                Field::Plural | Field::None => {}
            },
            _ => {}
        }
    }
    if matches!(field, Field::Str(_)) {
        finish(&mut entry, &mut catalog);
    }
    catalog
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let inner = s.strip_prefix('"').and_then(|r| r.strip_suffix('"')).unwrap_or("");
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}
