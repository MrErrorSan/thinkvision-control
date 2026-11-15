//! EDID identification (base block, plus names in CTA-861 extensions). Pure: no I/O.

const HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];

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

/// The EDID id identity uses: manufacturer (bytes 8–9, big-endian, three 5-bit letters, 'A' = 1) + product code
/// (bytes 10–11, little-endian) in hex, e.g. "LEN62A2" from `30 AE A2 62`. None without a header.
pub fn id(edid: &[u8]) -> Option<String> {
    if edid.len() < 12 || edid[..8] != HEADER {
        return None;
    }
    manufacturer(edid).map(|m| format!("{m}{:04X}", u16::from_le_bytes([edid[10], edid[11]])))
}

/// The monitor's own name: the first display product name descriptor (tag 0xFC) in the base block or a CTA-861
/// extension, up to 13 characters ended by a line feed. None when there is none.
pub fn display_name(edid: &[u8]) -> Option<String> {
    descriptor_text(edid, 0xFC)
        .map(|text| {
            text.iter().map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '?' }).collect::<String>()
        })
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
}

/// The monitor's serial: the serial-number text descriptor (tag 0xFF) when there is one, else the 32-bit serial
/// number in bytes 12-15 when it isn't zero. None when the EDID gives neither.
pub fn serial(edid: &[u8]) -> Option<String> {
    let text = descriptor_text(edid, 0xFF)
        .map(|text| text.iter().filter(|b| b.is_ascii_graphic()).map(|&b| b as char).collect::<String>())
        .filter(|s| !s.is_empty());
    text.or_else(|| {
        let n = u32::from_le_bytes(edid.get(12..16)?.try_into().ok()?);
        (n != 0).then(|| n.to_string())
    })
}

/// The text of the first display descriptor with `tag`, up to the line feed that ends it.
fn descriptor_text(edid: &[u8], tag: u8) -> Option<&[u8]> {
    descriptors(edid)
        // A display descriptor starts 00 00 00 <tag> 00; a detailed timing starts with a non-zero pixel clock.
        .find(|d| d[..3] == [0, 0, 0] && d[3] == tag && d[4] == 0)
        .map(|d| {
            let text = &d[5..];
            &text[..text.iter().position(|&b| b == 0x0A).unwrap_or(text.len())]
        })
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

/// EDIDs for other modules' tests.
#[cfg(test)]
pub(crate) mod testing {
    /// A 128-byte base block with vendor/product bytes 8–11, the 32-bit serial in bytes 12–15, and, when given, a name
    /// descriptor (tag 0xFC) in the first descriptor slot. The checksum byte is left at 0.
    pub(crate) fn edid(vendor_product: [u8; 4], serial: u32, name: Option<&str>) -> Vec<u8> {
        let mut edid = vec![0u8; 128];
        edid[..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
        edid[8..12].copy_from_slice(&vendor_product);
        edid[12..16].copy_from_slice(&serial.to_le_bytes());
        if let Some(name) = name {
            edid[54..59].copy_from_slice(&[0, 0, 0, 0xFC, 0]);
            let mut text = [b' '; 13];
            text[..name.len()].copy_from_slice(name.as_bytes());
            if name.len() < 13 {
                text[name.len()] = 0x0A;
            }
            edid[59..72].copy_from_slice(&text);
        }
        edid
    }

    /// LEN62A2 (the P32p-20) with that serial and name.
    pub(crate) fn p32p20(serial: u32, name: Option<&str>) -> Vec<u8> {
        edid([0x30, 0xAE, 0xA2, 0x62], serial, name)
    }
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
    fn serial_prefers_the_text_descriptor_then_the_number() {
        let mut edid = edid_header(0x30, 0xAE, 0xA2, 0x62);
        assert_eq!(serial(&edid), None);
        edid[12..16].copy_from_slice(&1234u32.to_le_bytes());
        assert_eq!(serial(&edid).as_deref(), Some("1234"));
        edid[72..77].copy_from_slice(&[0, 0, 0, 0xFF, 0]);
        edid[77..90].copy_from_slice(b"V90A1234\n    ");
        assert_eq!(serial(&edid).as_deref(), Some("V90A1234"));
    }

    #[test]
    fn edid_id_needs_the_header_and_reads_vendor_and_product() {
        assert_eq!(manufacturer(&edid_header(0x30, 0xAE, 0xA2, 0x62)).as_deref(), Some("LEN"));
        assert_eq!(id(&edid_header(0x30, 0xAE, 0xA3, 0x62)).as_deref(), Some("LEN62A3"));
        assert_eq!(id(&edid_header(0x10, 0xAC, 0xA2, 0x62)).as_deref(), Some("DEL62A2"));
        let mut bad_header = edid_header(0x30, 0xAE, 0xA2, 0x62);
        bad_header[0] = 0x01;
        assert_eq!(id(&bad_header), None);
        assert_eq!(id(&[0u8; 11]), None);
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
    fn pnp_unpacks() {
        assert_eq!(pnp_letters(0x30AE).as_deref(), Some("LEN"));
        assert_eq!(pnp_letters(0), None);
    }
}
