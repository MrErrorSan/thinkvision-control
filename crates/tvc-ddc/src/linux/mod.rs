//! Linux i2c-dev backend. The bus-selection rules here are pure and compile on every OS; the link itself is Linux-only.

#[cfg(target_os = "linux")]
mod i2c;
#[cfg(target_os = "linux")]
pub(crate) use i2c::I2cLink;

/// Adapter-name prefixes (`/sys/bus/i2c/devices/i2c-N/name`) that are never a display's DDC lines, as ddcutil matches
/// them. Probing these would write an address byte to unrelated chips (SPD EEPROMs sit at 0x50 on SMBus).
const IGNORABLE_ADAPTER_PREFIXES: [&str; 8] =
    ["SMBus", "i2c-designware", "Synopsys DesignWare", "AMDGPU SMU", "smu", "soc:i2cdsi", "mac-io", "u4"];

/// True for adapter names that are never a display's DDC bus (SMBus controllers and the like).
pub fn is_ignorable_adapter(adapter_name: &str) -> bool {
    IGNORABLE_ADAPTER_PREFIXES.iter().any(|p| adapter_name.starts_with(p))
}

/// Orders the buses to probe: those named by a `/sys/class/drm/*/ddc` link first, then the rest; each group sorted by
/// bus number. `devices` are device nodes (`/dev/i2c-4`, or a bare `i2c-4`) and are returned as given, deduplicated;
/// `linked` are the adapter names the drm `ddc` links point at (`i2c-4`).
pub fn candidate_buses<D: AsRef<str>, L: AsRef<str>>(devices: &[D], linked: &[L]) -> Vec<String> {
    let is_linked = |device: &str| linked.iter().any(|l| bus_name(l.as_ref()) == bus_name(device));
    let mut buses: Vec<String> = devices.iter().map(|d| d.as_ref().to_owned()).collect();
    buses.sort_by(|a, b| {
        let key = |d: &str| (!is_linked(d), bus_number(d).unwrap_or(u32::MAX));
        key(a).cmp(&key(b)).then_with(|| a.cmp(b))
    });
    buses.dedup();
    buses
}

/// `/dev/i2c-4` → `i2c-4`.
pub fn bus_name(device_or_name: &str) -> &str {
    device_or_name.trim_end_matches('/').rsplit('/').next().unwrap_or_default()
}

fn bus_number(device: &str) -> Option<u32> {
    bus_name(device).strip_prefix("i2c-")?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_order_prefers_drm_ddc_links_then_numeric() {
        let buses = candidate_buses(
            &["/dev/i2c-10", "/dev/i2c-2", "/dev/i2c-1", "/dev/i2c-7", "/dev/i2c-3", "/dev/i2c-2"],
            &["i2c-7", "i2c-3", "i2c-99"],
        );
        assert_eq!(buses, ["/dev/i2c-3", "/dev/i2c-7", "/dev/i2c-1", "/dev/i2c-2", "/dev/i2c-10"]);
    }

    #[test]
    fn memory_and_sensor_buses_are_skipped() {
        assert!(is_ignorable_adapter("SMBus I801 adapter at efa0"));
        assert!(!is_ignorable_adapter("i915 gmbus dpb"));
        assert!(!is_ignorable_adapter("AMDGPU DM i2c hw bus 1"));
    }
}
