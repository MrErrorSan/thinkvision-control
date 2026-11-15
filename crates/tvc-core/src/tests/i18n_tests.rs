//! Translation catalogs and language matching.

use crate::i18n::{best_match, parse, LANGUAGES};

fn locales(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn best_match_takes_the_first_shipped_language() {
    assert_eq!(best_match(&locales(&["de-AT", "en-US"])), "de");
    assert_eq!(best_match(&locales(&["fr_CA.UTF-8"])), "fr");
    assert_eq!(best_match(&locales(&["pt-BR", "es-MX"])), "es");
    assert_eq!(best_match(&locales(&["zh-Hans-CN"])), "zh_CN");
    assert_eq!(best_match(&locales(&["zh_CN.UTF-8"])), "zh_CN");
    assert_eq!(best_match(&locales(&["zh-TW", "de"])), "de");
    assert_eq!(best_match(&locales(&["ja", "ko"])), "en");
    assert_eq!(best_match(&[]), "en");
}

#[test]
fn every_shipped_catalog_parses() {
    for (code, _) in LANGUAGES.iter().filter(|(c, _)| *c != "en") {
        let path = format!("{}/../../translations/{code}/LC_MESSAGES/tvc.po", env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let catalog = parse(&text);
        assert!(!catalog.contains_key(""), "{code}: header kept as an entry");
    }
}

#[test]
fn po_entries_plurals_continuations_and_escapes() {
    let po = r#"
msgid ""
msgstr "Language: de\n"

# a comment
msgid "Brightness"
msgstr "Helligkeit"

msgid ""
"Restored {0} of "
"{1} settings."
msgstr "{0} von {1} Einstellungen "
"wiederhergestellt."

msgid "One display"
msgid_plural "{0} displays"
msgstr[0] "Ein Bildschirm"
msgstr[1] "{0} Bildschirme"

#, fuzzy
msgid "Contrast"
msgstr "Kontrast?"

msgctxt "Dialog"
msgid "Open"
msgstr "Öffnen"

msgid "Quote \"this\"\tand\nthat"
msgstr "Zitat \"dies\""

msgid "Untranslated"
msgstr ""
"#;
    let c = parse(po);
    assert_eq!(c["Brightness"], ["Helligkeit"]);
    assert_eq!(c["Restored {0} of {1} settings."], ["{0} von {1} Einstellungen wiederhergestellt."]);
    assert_eq!(c["One display"], ["Ein Bildschirm", "{0} Bildschirme"]);
    assert!(!c.contains_key("Contrast"), "fuzzy entries are skipped");
    assert!(!c.contains_key("Open"), "entries with a context belong to the Slint catalog");
    assert_eq!(c["Quote \"this\"\tand\nthat"], ["Zitat \"dies\""]);
    assert_eq!(c["Untranslated"], [""]);
    assert!(!c.contains_key(""));
}

#[test]
fn english_scope_nests_and_restores() {
    use crate::i18n::{english, tr};
    // The default language is English, so this checks the scope's bookkeeping, not a translation.
    let inner = english(|| english(|| tr("Brightness")));
    assert_eq!(inner, "Brightness");
    assert_eq!(tr("Brightness"), "Brightness");
}

/// (msgctxt, msgid) → msgstr forms, for coverage checks; unlike `parse`, entries with a context are kept.
fn entries(text: &str) -> std::collections::BTreeMap<(String, String), Vec<String>> {
    fn unquote(s: &str) -> String {
        let inner = s.trim().strip_prefix('"').and_then(|r| r.strip_suffix('"')).unwrap_or("");
        inner.replace("\\\"", "\"").replace("\\n", "\n").replace("\\t", "\t").replace("\\\\", "\\")
    }
    let mut out = std::collections::BTreeMap::new();
    let (mut ctx, mut id, mut forms, mut last) = (String::new(), String::new(), Vec::<String>::new(), "");
    let mut flush = |ctx: &mut String, id: &mut String, forms: &mut Vec<String>| {
        if !id.is_empty() {
            out.insert((std::mem::take(ctx), std::mem::take(id)), std::mem::take(forms));
        }
        ctx.clear();
        forms.clear();
    };
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let (key, rest) = if line.starts_with('"') { ("", line) } else { line.split_once(' ').unwrap_or((line, "")) };
        match key {
            "msgctxt" => {
                flush(&mut ctx, &mut id, &mut forms);
                ctx = unquote(rest);
                last = "ctx";
            }
            "msgid" => {
                if last != "ctx" {
                    flush(&mut ctx, &mut id, &mut forms);
                }
                id = unquote(rest);
                last = "id";
            }
            "msgid_plural" => last = "plural",
            k if k.starts_with("msgstr") => {
                forms.push(unquote(rest));
                last = "str";
            }
            "" => match last {
                "id" => id.push_str(&unquote(rest)),
                "ctx" => ctx.push_str(&unquote(rest)),
                "str" => forms.last_mut().unwrap().push_str(&unquote(rest)),
                _ => {}
            },
            _ => {}
        }
    }
    flush(&mut ctx, &mut id, &mut forms);
    out
}

/// `{0}`, `{1}`, `{}` and `{name}` tokens, sorted.
fn placeholders(s: &str) -> Vec<String> {
    let mut found: Vec<String> = s.match_indices('{').filter_map(|(i, _)| {
        let end = s[i..].find('}')?;
        let token = &s[i..=i + end];
        token[1..token.len() - 1].chars().all(|c| c.is_ascii_alphanumeric() || c == '_').then(|| token.to_string())
    }).collect();
    found.sort();
    found
}

#[test]
fn every_catalog_covers_its_template_with_matching_placeholders() {
    let root = format!("{}/../../translations", env!("CARGO_MANIFEST_DIR"));
    for file in ["tvc", "tvc-app"] {
        let template = entries(&std::fs::read_to_string(format!("{root}/{file}.pot")).unwrap());
        for (code, _) in LANGUAGES.iter().filter(|(c, _)| *c != "en") {
            let path = format!("{root}/{code}/LC_MESSAGES/{file}.po");
            let catalog = entries(&std::fs::read_to_string(&path).unwrap());
            let mut problems = Vec::new();
            for key in template.keys() {
                match catalog.get(key) {
                    None => problems.push(format!("missing {key:?}")),
                    Some(forms) if forms.is_empty() || forms.iter().any(String::is_empty) => {
                        problems.push(format!("untranslated {key:?}"))
                    }
                    Some(forms) => {
                        for form in forms {
                            if placeholders(form) != placeholders(&key.1) {
                                problems.push(format!("placeholders differ in {key:?}: {form:?}"));
                            }
                        }
                    }
                }
            }
            assert!(problems.is_empty(), "{path}: {} problem(s), first: {:?}", problems.len(), &problems[..problems.len().min(5)]);
        }
    }
}
