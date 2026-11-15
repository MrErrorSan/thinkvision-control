//! The pure half of the Windows join: monitor device paths, the connector string, and which physical monitor of an
//! HMONITOR belongs to a target. No windows-sys.

/// "LEN62A2" from a monitor device path such as `\\?\DISPLAY#LEN62A2#5&2a1b&0&UID4352#{e6f07b5f-...}`.
/// Only three letters and four hex digits count.
pub(super) fn edid_id_from_path(path: &str) -> Option<String> {
    let id = path.split('#').nth(1)?.to_ascii_uppercase();
    let (pnp, product) = id.split_at_checked(3)?;
    let valid = pnp.bytes().all(|b| b.is_ascii_uppercase())
        && product.len() == 4
        && product.bytes().all(|b| b.is_ascii_hexdigit());
    valid.then_some(id)
}

/// "LEN62A2" from DisplayConfig's EDID ids. `edidManufactureId` holds EDID bytes 8–9 as read little-endian, so the
/// big-endian PNP number is byte-swapped; the product code is already little-endian, as in the EDID.
pub(super) fn edid_id_from_ids(manufacturer: u16, product: u16) -> Option<String> {
    crate::edid::pnp_letters(manufacturer.swap_bytes()).map(|pnp| format!("{pnp}{product:04X}"))
}

/// The device's key under `HKLM\SYSTEM\CurrentControlSet\Enum`:
/// `\\?\DISPLAY#LEN62A2#5&2a1b&0&UID4352#{e6f07b5f-...}` gives `DISPLAY\LEN62A2\5&2a1b&0&UID4352`.
pub(super) fn instance_key(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.strip_prefix(r"\\?\")?.split('#').collect();
    let [class, hardware, instance, _interface] = parts[..] else { return None };
    let valid = [class, hardware, instance].iter().all(|p| !p.is_empty() && !p.contains('\\'));
    valid.then(|| format!(r"{class}\{hardware}\{instance}"))
}

/// The port: the PnP instance of the device path ("5&2a1b&0&UID4352"), which Windows builds from the adapter's
/// location and the target's UID. It survives reboots, unlike the adapter LUID, and stays while the cable does.
pub(super) fn connector(path: &str) -> Option<String> {
    instance_key(path)?.rsplit('\\').next().map(str::to_owned)
}

/// Which of an HMONITOR's physical monitors (by `descriptions`) is the target named `own`, where `names` are the
/// friendly names of every target on that HMONITOR (a clone has several). One of each pairs directly; otherwise the
/// one description equal to the name, when no other target has that name. None: ambiguous, don't guess.
pub(super) fn pick_physical(descriptions: &[String], names: &[&str], own: &str) -> Option<usize> {
    if descriptions.len() == 1 && names.len() == 1 {
        return Some(0);
    }
    let own = own.trim();
    if own.is_empty() || names.iter().filter(|n| n.trim().eq_ignore_ascii_case(own)).count() != 1 {
        return None;
    }
    let mut hits = descriptions.iter().enumerate().filter(|(_, d)| d.trim().eq_ignore_ascii_case(own)).map(|(i, _)| i);
    match (hits.next(), hits.next()) {
        (Some(i), None) => Some(i),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATH: &str = r"\\?\DISPLAY#LEN62A2#5&2a1b&0&UID4352#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";

    #[test]
    fn edid_id_is_the_second_path_segment() {
        assert_eq!(edid_id_from_path(PATH).as_deref(), Some("LEN62A2"));
        assert_eq!(edid_id_from_path(r"\\?\display#len62a2#5&1&0&UID1#{x}").as_deref(), Some("LEN62A2"));
        assert_eq!(edid_id_from_path(r"\\?\DISPLAY#Default_Monitor#1&2&0&UID0#{x}"), None);
        assert_eq!(edid_id_from_path(r"\\?\DISPLAY#LEN62G2#1&2&0&UID0#{x}"), None); // G isn't hex
        assert_eq!(edid_id_from_path(r"\\?\DISPLAY#LE162A2#1&2&0&UID0#{x}"), None); // digit in the vendor
        assert_eq!(edid_id_from_path(r"\\?\DISPLAY#LEN62A20#1&2&0&UID0#{x}"), None); // five-digit product
        assert_eq!(edid_id_from_path("garbage"), None);
    }

    #[test]
    fn instance_key_is_the_registry_path_of_the_device() {
        assert_eq!(instance_key(PATH).as_deref(), Some(r"DISPLAY\LEN62A2\5&2a1b&0&UID4352"));
        assert_eq!(instance_key(r"DISPLAY#LEN62A2#5&2a1b&0&UID4352#{x}"), None); // no \\?\ prefix
        assert_eq!(instance_key(r"\\?\DISPLAY#LEN62A2#5&2a1b&0&UID4352"), None); // no interface segment
        assert_eq!(instance_key(r"\\?\DISPLAY#LEN62A2#5&2a1b#0#{x}"), None); // too many segments
        assert_eq!(instance_key(r"\\?\DISPLAY##5&2a1b&0&UID4352#{x}"), None); // empty segment
        assert_eq!(instance_key(r"\\?\DISPLAY#..\..\SAM#5&2a1b&0&UID4352#{x}"), None); // can't climb the key
    }

    #[test]
    fn connector_is_the_instance_segment() {
        assert_eq!(connector(PATH).as_deref(), Some("5&2a1b&0&UID4352"));
        assert_eq!(connector(r"\\?\DISPLAY#DEL4123#5&2a1b&0&UID4353#{x}").as_deref(), Some("5&2a1b&0&UID4353"));
        assert_eq!(connector("garbage"), None);
        assert_eq!(connector(""), None);
    }

    #[test]
    fn edid_ids_swap_the_vendor_bytes_only() {
        // EDID bytes 8–11 of the P32p-20 are 30 AE A2 62; DisplayConfig reads both pairs little-endian.
        assert_eq!(edid_id_from_ids(u16::from_le_bytes([0x30, 0xAE]), u16::from_le_bytes([0xA2, 0x62])).as_deref(), Some("LEN62A2"));
        assert_eq!(edid_id_from_ids(0xAE30, 0x0001).as_deref(), Some("LEN0001"));
        assert_eq!(edid_id_from_ids(0xAC10, 0x4123).as_deref(), Some("DEL4123"));
        assert_eq!(edid_id_from_ids(0, 0x62A2), None); // no letters
    }

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn one_physical_monitor_for_one_target_pairs_directly() {
        assert_eq!(pick_physical(&strings(&["Generic PnP Monitor"]), &["P32p-20"], "P32p-20"), Some(0));
        assert_eq!(pick_physical(&strings(&["Generic PnP Monitor"]), &[""], ""), Some(0));
    }

    #[test]
    fn a_clone_pairs_by_its_unique_description() {
        let descriptions = strings(&["DELL U2720Q", "P32p-20"]);
        assert_eq!(pick_physical(&descriptions, &["DELL U2720Q", "P32p-20"], "P32p-20"), Some(1));
        assert_eq!(pick_physical(&descriptions, &["DELL U2720Q", "P32p-20"], "DELL U2720Q"), Some(0));
        // Case and surrounding spaces don't matter.
        assert_eq!(pick_physical(&strings(&["dell u2720q ", "p32p-20"]), &["DELL U2720Q", " P32p-20"], " P32P-20 "), Some(1));
        // One physical monitor shared by two targets: only the name that is its description pairs.
        assert_eq!(pick_physical(&strings(&["P32p-20"]), &["P32p-20", "DELL U2720Q"], "P32p-20"), Some(0));
        assert_eq!(pick_physical(&strings(&["P32p-20"]), &["P32p-20", "DELL U2720Q"], "DELL U2720Q"), None);
    }

    #[test]
    fn an_ambiguous_clone_gives_none() {
        // Two identical monitors cloned: the name isn't unique among the targets.
        let twins = strings(&["P32p-20", "P32p-20"]);
        assert_eq!(pick_physical(&twins, &["P32p-20", "P32p-20"], "P32p-20"), None);
        // Unique names, but two descriptions match.
        assert_eq!(pick_physical(&twins, &["P32p-20", "DELL U2720Q"], "P32p-20"), None);
        // No description matches.
        let generic = strings(&["Generic PnP Monitor", "Generic PnP Monitor"]);
        assert_eq!(pick_physical(&generic, &["P32p-20", "DELL U2720Q"], "P32p-20"), None);
        // No name to go by.
        assert_eq!(pick_physical(&strings(&["A", "B"]), &["", "B"], ""), None);
        assert_eq!(pick_physical(&[], &["P32p-20"], "P32p-20"), None);
    }
}
