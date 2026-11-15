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
        assert_eq!(edid_id_from_path("garbage"), None);
    }
}
