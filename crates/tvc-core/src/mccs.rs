//! MCCS capability strings: `(prot(monitor)type(LCD)model(P32p-20)...vcp(02 04(4C 42) 10...))`.

use std::collections::BTreeMap;
use std::fmt;

const INPUT: u8 = 0x60;

/// Parsed capability string. A `None` value list means the code is advertised without one (continuous: 0..max).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MonitorCapabilities {
    pub model: String,
    /// `vcp(...)`: every advertised code with its top-level values, for display. A code advertised more than once
    /// holds the union of its lists. 0x60 never lists FF.
    pub vcp: BTreeMap<u8, Option<Vec<u8>>>,
    /// Values a code advertised more than once may be written with: those present in every occurrence.
    pub writable: BTreeMap<u8, Vec<u8>>,
    /// `vcpname(...)`: names the monitor gives its own codes.
    pub names: BTreeMap<u8, String>,
    /// `mccs_ver(...)`, e.g. "2.2"; empty when absent.
    pub mccs_ver: String,
    /// What the parser skipped, one line each; empty when the string parsed cleanly.
    pub warnings: Vec<String>,
}

impl MonitorCapabilities {
    pub fn supports(&self, code: u8) -> bool {
        self.vcp.contains_key(&code)
    }

    /// Whether the capability string permits writing `value` to `code`. A code advertised without a value list
    /// permits any value; whether that is safe for an Enum code is the safety
    /// layer's decision.
    pub fn allows_value(&self, code: u8, value: u16) -> bool {
        let Ok(v) = u8::try_from(value) else { return self.vcp.get(&code) == Some(&None) };
        match (self.writable.get(&code), self.vcp.get(&code)) {
            (_, None) => false,
            (Some(values), _) => values.contains(&v),
            (None, Some(None)) => true,
            (None, Some(Some(values))) => values.contains(&v),
        }
    }

    /// The value list of 0x04 read as ASCII ("LB1.2" on the P32p-20), when it has ≥2 bytes, all printable.
    pub fn firmware(&self) -> Option<String> {
        let bytes = self.vcp.get(&0x04)?.as_ref()?;
        (bytes.len() >= 2 && bytes.iter().all(|b| (0x20..0x7F).contains(b)))
            .then(|| bytes.iter().map(|&b| b as char).collect())
    }
}

/// The MCCS 2.2a name of a 0x60 input-source value, for monitors with no profile.
pub fn input_name(value: u8) -> Option<&'static str> {
    Some(match value {
        0x01 => "VGA 1",
        0x02 => "VGA 2",
        0x03 => "DVI 1",
        0x04 => "DVI 2",
        0x05 => "Composite 1",
        0x06 => "Composite 2",
        0x07 => "S-Video 1",
        0x08 => "S-Video 2",
        0x09 => "Tuner 1",
        0x0A => "Tuner 2",
        0x0B => "Tuner 3",
        0x0C => "Component 1",
        0x0D => "Component 2",
        0x0E => "Component 3",
        0x0F => "DisplayPort 1",
        0x10 => "DisplayPort 2",
        0x11 => "HDMI 1",
        0x12 => "HDMI 2",
        _ => return None,
    })
}

/// The capability string has no usable `vcp(...)` section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

/// Parses a capability string. Broken sections and broken `vcp` entries are skipped and listed in `warnings`;
/// only a string with no usable `vcp` entry is an error. Positions in messages count from the start of the string.
pub fn parse(capabilities: &str) -> Result<MonitorCapabilities, ParseError> {
    let s = capabilities.as_bytes();
    let mut caps = MonitorCapabilities::default();
    let mut entries = Vec::new();
    for (open, close) in find_sections(s, b"vcp") {
        let end = close.unwrap_or_else(|| {
            caps.warnings.push(format!("vcp(...) at position {open} is not closed; read to the end of the string."));
            s.len()
        });
        parse_vcp(s, open + 1, end, &mut entries, &mut caps.warnings);
    }
    if entries.is_empty() {
        return Err(ParseError("Capability string has no vcp(...) section with a usable entry.".into()));
    }
    merge(&mut caps, entries);
    caps.model = text_section(s, b"model", &mut caps.warnings);
    caps.mccs_ver = text_section(s, b"mccs_ver", &mut caps.warnings);
    if let Some(contents) = section(s, b"vcpname", &mut caps.warnings) {
        caps.names = parse_names(s, contents, &mut caps.warnings);
    }
    Ok(caps)
}

/// One `vcp` entry as written: its code and top-level values (a value's sub-list, as in the P40w-20's
/// `F7(... 14(00 02 03) ...)`, is dropped).
struct Entry {
    code: u8,
    values: Option<Vec<u8>>,
}

/// A value in a list; `nested` once a sub-list followed it (a second one is an error).
struct Value {
    value: u8,
    nested: bool,
}

/// Folds the entries into `vcp` and `writable`.
fn merge(caps: &mut MonitorCapabilities, entries: Vec<Entry>) {
    let mut by_code: BTreeMap<u8, Vec<Entry>> = BTreeMap::new();
    for e in entries {
        by_code.entry(e.code).or_default().push(e);
    }
    for (code, mut list) in by_code {
        let strip_ff = |v: &mut Vec<u8>| if code == INPUT { v.retain(|&b| b != 0xFF) };
        if list.len() == 1 {
            let mut values = list.pop().and_then(|e| e.values);
            if let Some(v) = values.as_mut() {
                strip_ff(v);
            }
            caps.vcp.insert(code, values);
            continue;
        }
        // An occurrence without a list neither widens nor narrows the others.
        let lists: Vec<&Vec<u8>> = list.iter().filter_map(|e| e.values.as_ref()).collect();
        if lists.is_empty() {
            caps.vcp.insert(code, None);
            continue;
        }
        let mut union: Vec<u8> = Vec::new();
        for &v in lists.iter().copied().flatten() {
            if !union.contains(&v) {
                union.push(v);
            }
        }
        let mut common: Vec<u8> = union.iter().copied().filter(|v| lists.iter().all(|l| l.contains(v))).collect();
        strip_ff(&mut union);
        strip_ff(&mut common);
        caps.vcp.insert(code, Some(union));
        caps.writable.insert(code, common);
    }
}

/// Byte range of a section's contents, between its parentheses.
struct Contents {
    start: usize,
    end: usize,
}

/// Every `name(...)` (or `name (...)`) at depth 0 or inside the outer wrapper, as (index of `(`, index of its `)`).
/// When there is none and the parentheses don't balance, an earlier section may have swallowed it, so the first one
/// at any depth counts.
fn find_sections(s: &[u8], name: &[u8]) -> Vec<(usize, Option<usize>)> {
    let found = |any_depth: bool| {
        let mut depth = 0i32;
        let mut found = Vec::new();
        for i in 0..s.len() {
            if let Some(open) = name_at(s, i, name).filter(|_| any_depth || depth <= 1) {
                found.push((open, matching_close(s, open)));
                if any_depth {
                    break;
                }
            }
            match s[i] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
        }
        found
    };
    let at_top = found(false);
    let balanced = s.iter().filter(|&&c| c == b'(').count() == s.iter().filter(|&&c| c == b')').count();
    if at_top.is_empty() && !balanced { found(true) } else { at_top }
}

/// The index of the `(` when `name(` or `name (` starts at `i` and isn't the tail of a longer word.
fn name_at(s: &[u8], i: usize, name: &[u8]) -> Option<usize> {
    if !s[i..].starts_with(name) || (i > 0 && (s[i - 1].is_ascii_alphanumeric() || s[i - 1] == b'_')) {
        return None;
    }
    let mut open = i + name.len();
    while s.get(open).is_some_and(|c| c.is_ascii_whitespace()) {
        open += 1;
    }
    (s.get(open) == Some(&b'(')).then_some(open)
}

/// The first `name(...)` section's contents; an unclosed one is skipped with a warning.
fn section(s: &[u8], name: &[u8], warnings: &mut Vec<String>) -> Option<Contents> {
    let (open, close) = *find_sections(s, name).first()?;
    let Some(close) = close else {
        warnings.push(format!("{}(...) at position {open} is not closed; ignored.", String::from_utf8_lossy(name)));
        return None;
    };
    Some(Contents { start: open + 1, end: close })
}

fn text_section(s: &[u8], name: &[u8], warnings: &mut Vec<String>) -> String {
    section(s, name, warnings).map(|c| String::from_utf8_lossy(&s[c.start..c.end]).trim().to_string()).unwrap_or_default()
}

fn matching_close(s: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, &c) in s.iter().enumerate().skip(open) {
        if c == b'(' {
            depth += 1;
        } else if c == b')' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// The entries of `s[start..end]`, a `vcp` section. A broken entry is skipped with a warning.
fn parse_vcp(s: &[u8], start: usize, end: usize, entries: &mut Vec<Entry>, warnings: &mut Vec<String>) {
    let mut i = start;
    loop {
        skip_separators(s, &mut i, end);
        if i >= end {
            return;
        }
        let at = i;
        if s[i] == b')' {
            warnings.push(format!("Skipped a stray ')' at position {at} in vcp(...)."));
            i += 1;
            continue;
        }
        let codes = if s[i] == b'(' { Err(format!("a value list without a code at position {at}")) } else { read_token(s, &mut i, end) };
        skip_separators(s, &mut i, end);
        let mut values = None;
        if i < end && s[i] == b'(' {
            let close = matching_close(s, i).filter(|&c| c < end);
            let mut j = i + 1;
            let list_end = close.unwrap_or(end);
            values = Some(parse_list(s, &mut j, list_end).and_then(|list| match close {
                Some(_) => Ok(list),
                None => Err(format!("the value list at position {i} is not closed")),
            }));
            i = close.map_or(end, |c| c + 1);
        }
        let codes = match codes {
            Ok(codes) => codes,
            Err(e) => {
                warnings.push(format!("Skipped a vcp entry: {e}."));
                continue;
            }
        };
        let (&code, bare) = codes.split_last().expect("a token has at least one byte");
        entries.extend(bare.iter().map(|&code| Entry { code, values: None }));
        let values = match values.transpose() {
            Ok(values) => values.map(|list| list.into_iter().map(|v| v.value).collect()),
            Err(e) => {
                warnings.push(format!("Skipped vcp entry {code:02X}: {e}."));
                continue;
            }
        };
        entries.push(Entry { code, values });
    }
}

/// The values of `s[*i..end]`, a value list. A value may carry its own parenthesised sub-list.
fn parse_list(s: &[u8], i: &mut usize, end: usize) -> Result<Vec<Value>, String> {
    let mut values: Vec<Value> = Vec::new();
    loop {
        skip_separators(s, i, end);
        if *i >= end {
            return Ok(values);
        }
        match s[*i] {
            b')' => return Err(format!("unexpected ')' at position {i}")),
            b'(' => {
                let open = *i;
                let last = values.last_mut().filter(|v| !v.nested).ok_or_else(|| format!("unexpected '(' at position {open}"))?;
                let close = matching_close(s, open).filter(|&c| c < end).ok_or_else(|| format!("the list at position {open} is not closed"))?;
                *i = open + 1;
                parse_list(s, i, close)?;
                last.nested = true;
                *i = close + 1;
            }
            _ => values.extend(read_token(s, i, end)?.into_iter().map(|value| Value { value, nested: false })),
        }
    }
}

/// `vcpname(14(Colour Preset) E0(Over Drive))`. A broken entry ends the section with a warning.
fn parse_names(s: &[u8], contents: Contents, warnings: &mut Vec<String>) -> BTreeMap<u8, String> {
    let mut names = BTreeMap::new();
    let mut i = contents.start;
    loop {
        skip_separators(s, &mut i, contents.end);
        if i >= contents.end {
            return names;
        }
        let code = read_token(s, &mut i, contents.end).map(|codes| codes[codes.len() - 1]);
        skip_separators(s, &mut i, contents.end);
        let close = (s.get(i) == Some(&b'(')).then(|| matching_close(s, i)).flatten().filter(|&c| c < contents.end);
        let (Ok(code), Some(close)) = (code, close) else {
            warnings.push(format!("Skipped the rest of vcpname(...) from position {i}."));
            return names;
        };
        let text = &s[i + 1..close];
        let text = text.iter().position(|&c| c == b'(').map_or(text, |p| &text[..p]);
        names.insert(code, String::from_utf8_lossy(text).trim().to_string());
        i = close + 1;
    }
}

/// Whitespace, NUL and other control bytes all separate tokens.
fn skip_separators(s: &[u8], i: &mut usize, end: usize) {
    while *i < end && is_separator(s[*i]) {
        *i += 1;
    }
}

fn is_separator(c: u8) -> bool {
    c <= b' ' || c == 0x7F
}

/// A run of hex digits: one digit is one byte, an even count is a byte per pair (`0F11` is 0F 11). Any other run
/// is an error naming its position.
fn read_token(s: &[u8], i: &mut usize, end: usize) -> Result<Vec<u8>, String> {
    let at = *i;
    while *i < end && !is_separator(s[*i]) && s[*i] != b'(' && s[*i] != b')' {
        *i += 1;
    }
    let token = &s[at..*i];
    let digit = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let digits: Option<Vec<u8>> = token.iter().map(|&c| digit(c)).collect();
    match digits {
        Some(d) if d.len() == 1 => Ok(d),
        Some(d) if !d.is_empty() && d.len() % 2 == 0 => Ok(d.chunks(2).map(|p| p[0] << 4 | p[1]).collect()),
        _ => Err(format!("'{}' at position {at} is not a hex value", String::from_utf8_lossy(token))),
    }
}
