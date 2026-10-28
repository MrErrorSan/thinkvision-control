//! MCCS capability strings: `(prot(monitor)type(LCD)model(P32p-20)...vcp(02 04(4C 42) 10...))`.

use std::collections::BTreeMap;
use std::fmt;

/// Parsed capability string. A `None` value list means the code is continuous (0..max).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MonitorCapabilities {
    pub model: String,
    pub vcp: BTreeMap<u8, Option<Vec<u8>>>,
}

impl MonitorCapabilities {
    pub fn supports(&self, code: u8) -> bool {
        self.vcp.contains_key(&code)
    }

    pub fn allows_value(&self, code: u8, value: u16) -> bool {
        match self.vcp.get(&code) {
            None => false,
            Some(None) => true,
            Some(Some(values)) => u8::try_from(value).is_ok_and(|v| values.contains(&v)),
        }
    }

    /// The value list of 0x04 read as ASCII ("LB1.2" on the P32p-20), when it has ≥2 bytes, all printable.
    pub fn firmware(&self) -> Option<String> {
        let bytes = self.vcp.get(&0x04)?.as_ref()?;
        (bytes.len() >= 2 && bytes.iter().all(|b| (0x20..0x7F).contains(b)))
            .then(|| bytes.iter().map(|&b| b as char).collect())
    }
}

/// The capability string has no parsable `vcp(...)` section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

pub fn parse(capabilities: &str) -> Result<MonitorCapabilities, ParseError> {
    let s = capabilities.as_bytes();
    let vcp = find_section(s, b"vcp")?.ok_or_else(|| ParseError("Capability string has no vcp(...) section.".into()))?;
    let model = find_section(s, b"model")?
        .map(|m| String::from_utf8_lossy(m).trim().to_string())
        .unwrap_or_default();
    Ok(MonitorCapabilities { model, vcp: parse_vcp(vcp)? })
}

/// Contents of the first `name(...)` at depth 0 or inside the outer wrapper.
fn find_section<'a>(s: &'a [u8], name: &[u8]) -> Result<Option<&'a [u8]>, ParseError> {
    let mut depth = 0i32;
    for i in 0..s.len() {
        if depth <= 1
            && s[i..].starts_with(name)
            && s.get(i + name.len()) == Some(&b'(')
            && (i == 0 || !s[i - 1].is_ascii_alphanumeric())
        {
            let open = i + name.len();
            return Ok(Some(&s[open + 1..matching_close(s, open)?]));
        }
        match s[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
    }
    Ok(None)
}

fn matching_close(s: &[u8], open: usize) -> Result<usize, ParseError> {
    let mut depth = 0;
    for (i, &c) in s.iter().enumerate().skip(open) {
        if c == b'(' {
            depth += 1;
        } else if c == b')' {
            depth -= 1;
            if depth == 0 {
                return Ok(i);
            }
        }
    }
    Err(ParseError(format!("Unbalanced parentheses at position {open}.")))
}

fn parse_vcp(vcp: &[u8]) -> Result<BTreeMap<u8, Option<Vec<u8>>>, ParseError> {
    let mut result = BTreeMap::new();
    let mut i = 0;
    loop {
        skip_spaces(vcp, &mut i);
        if i >= vcp.len() {
            return Ok(result);
        }
        let code = read_hex_byte(vcp, &mut i)?;
        skip_spaces(vcp, &mut i);
        if vcp.get(i) == Some(&b'(') {
            let close = matching_close(vcp, i)?;
            result.insert(code, Some(parse_values(&vcp[i + 1..close])?));
            i = close + 1;
        } else {
            result.insert(code, None);
        }
    }
}

fn parse_values(s: &[u8]) -> Result<Vec<u8>, ParseError> {
    let mut values = Vec::new();
    let mut i = 0;
    loop {
        skip_spaces(s, &mut i);
        if i >= s.len() {
            return Ok(values);
        }
        values.push(read_hex_byte(s, &mut i)?);
    }
}

fn skip_spaces(s: &[u8], i: &mut usize) {
    while *i < s.len() && s[*i].is_ascii_whitespace() {
        *i += 1;
    }
}

fn read_hex_byte(s: &[u8], i: &mut usize) -> Result<u8, ParseError> {
    let pair = s.get(*i..*i + 2).filter(|p| p.iter().all(u8::is_ascii_hexdigit));
    let pair = pair.ok_or_else(|| ParseError(format!("Expected a two-digit hex value at position {i} in the vcp section.")))?;
    *i += 2;
    Ok(u8::from_str_radix(std::str::from_utf8(pair).unwrap_or_default(), 16).unwrap_or_default())
}
