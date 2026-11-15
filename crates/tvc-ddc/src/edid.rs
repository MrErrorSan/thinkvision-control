//! EDID identification (base block, plus names in CTA-861 extensions). Pure: no I/O.

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

/// The id `MonitorMatch::edid_id` uses: manufacturer + product code in hex, e.g. "LEN62A2". None without a header.
pub fn id(edid: &[u8]) -> Option<String> {
    if edid.len() < 12 || edid[..8] != HEADER {
        return None;
    }
    manufacturer(edid).map(|m| format!("{m}{:04X}", u16::from_le_bytes([edid[10], edid[11]])))
}

/// The monitor's own name: the first display product name descriptor (tag 0xFC) in the base block or a CTA-861
/// extension, up to 13 characters ended by a line feed. None when there is none.
pub fn display_name(edid: &[u8]) -> Option<String> {
    descriptors(edid)
        // A display descriptor starts 00 00 00 <tag> 00; a detailed timing starts with a non-zero pixel clock.
        .find(|d| d[..3] == [0, 0, 0] && d[3] == 0xFC && d[4] == 0)
        .map(|d| {
            let text = &d[5..];
            let end = text.iter().position(|&b| b == 0x0A).unwrap_or(text.len());
            text[..end].iter().map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '?' }).collect::<String>()
        })
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
}

/// True when the 128-byte base block sums to zero, as byte 127 makes it.
pub fn checksum_ok(edid: &[u8]) -> bool {
    edid.len() >= 128 && edid[..128].iter().fold(0u8, |sum, &b| sum.wrapping_add(b)) == 0
}

/// The four 18-byte descriptors of the base block, then the detailed-timing area of each CTA-861 extension block
/// (tag 0x02; byte 2 is where that area starts, and it ends before the checksum byte).
fn descriptors(edid: &[u8]) -> impl Iterator<Item = &[u8]> + '_ {
    let base = (0..4).filter_map(move |i| edid.get(54 + i * 18..72 + i * 18));
    let cta = edid
        .as_chunks::<128>()
        .0
        .iter()
        .skip(1)
        .filter(|block| block[0] == 0x02 && (4..127).contains(&block[2]))
        .flat_map(|block| block[usize::from(block[2])..127].as_chunks::<18>().0.iter().map(|d| d.as_slice()));
    base.chain(cta)
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
    fn id_and_name_come_from_the_base_block() {
        let mut edid = edid_header(0x30, 0xAE, 0xA2, 0x62);
        assert_eq!(id(&edid).as_deref(), Some("LEN62A2"));
        assert_eq!(display_name(&edid), None);
        // Descriptor 1: a detailed timing (non-zero clock) whose bytes would otherwise look like a name.
        edid[54..56].copy_from_slice(&[0x01, 0x1D]);
        edid[57] = 0xFC;
        // Descriptor 3: the name, padded the way monitors pad it.
        edid[90..95].copy_from_slice(&[0, 0, 0, 0xFC, 0]);
        edid[95..108].copy_from_slice(b"P32p-20\n     ");
        assert_eq!(display_name(&edid).as_deref(), Some("P32p-20"));
        assert_eq!(id(&[0u8; 128]), None);
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
