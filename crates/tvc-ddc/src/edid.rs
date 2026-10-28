//! EDID base-block identification. Pure: no I/O.

const HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];

/// True when `edid` starts with a valid EDID header and carries PNP manufacturer id `pnp` (bytes 8–9, big-endian,
/// three 5-bit letters, 'A' = 1; compared case-insensitively) and product code `product` (bytes 10–11,
/// little-endian). LEN / 0x62A2 is `30 AE A2 62` in bytes 8–11.
pub fn matches(edid: &[u8], pnp: &str, product: u16) -> bool {
    edid.len() >= 12
        && edid[..8] == HEADER
        && manufacturer(edid).is_some_and(|m| m.eq_ignore_ascii_case(pnp))
        && u16::from_le_bytes([edid[10], edid[11]]) == product
}

/// The three-letter PNP id from bytes 8–9; None if too short or a letter is out of range.
pub fn manufacturer(edid: &[u8]) -> Option<String> {
    (edid.len() >= 10).then(|| u16::from_be_bytes([edid[8], edid[9]])).and_then(pnp_letters)
}

/// Unpacks a PNP vendor number into its letters: 0x30AE → "LEN". None if a letter is out of range.
pub fn pnp_letters(packed: u16) -> Option<String> {
    [10, 5, 0]
        .iter()
        .map(|shift| match (packed >> shift) & 0x1F {
            v @ 1..=26 => Some(char::from(b'A' + v as u8 - 1)),
            _ => None,
        })
        .collect()
}

/// Packs three PNP letters into the vendor number CoreGraphics reports: "LEN" → 0x30AE. None unless three ASCII letters.
pub fn pack_pnp(pnp: &str) -> Option<u16> {
    let bytes = pnp.as_bytes();
    if bytes.len() != 3 || !bytes.iter().all(u8::is_ascii_alphabetic) {
        return None;
    }
    Some(bytes.iter().fold(0u16, |acc, b| acc << 5 | u16::from(b.to_ascii_uppercase() - b'A' + 1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edid_header(b8: u8, b9: u8, b10: u8, b11: u8) -> Vec<u8> {
        let mut edid = vec![0u8; 128];
        edid[..12].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, b8, b9, b10, b11]);
        edid
    }

    #[test]
    fn edid_matches_the_p32p20_and_nothing_else() {
        assert!(matches(&edid_header(0x30, 0xAE, 0xA2, 0x62), "LEN", 0x62A2));
        assert_eq!(manufacturer(&edid_header(0x30, 0xAE, 0xA2, 0x62)).as_deref(), Some("LEN"));
        assert!(!matches(&edid_header(0x30, 0xAE, 0xA3, 0x62), "LEN", 0x62A2)); // other product
        assert!(!matches(&edid_header(0x10, 0xAC, 0xA2, 0x62), "LEN", 0x62A2)); // DEL vendor
        let mut bad_header = edid_header(0x30, 0xAE, 0xA2, 0x62);
        bad_header[0] = 0x01;
        assert!(!matches(&bad_header, "LEN", 0x62A2));
        assert!(!matches(&[0u8; 11], "LEN", 0x62A2));
    }

    #[test]
    fn pnp_packs_and_unpacks() {
        assert_eq!(pnp_letters(0x30AE).as_deref(), Some("LEN"));
        assert_eq!(pack_pnp("LEN"), Some(0x30AE));
        assert_eq!(pack_pnp("len"), Some(0x30AE));
        assert_eq!(pnp_letters(0), None);
        assert_eq!(pack_pnp("LE"), None);
        assert_eq!(pack_pnp("L3N"), None);
    }
}
