//! macOS IOAVService backend (Apple Silicon). The rules here are pure and compile on every OS; the link is macOS-only.

#[cfg(target_os = "macos")]
mod ffi;
#[cfg(target_os = "macos")]
mod link;
#[cfg(target_os = "macos")]
mod locator;
#[cfg(target_os = "macos")]
pub(crate) use link::IoAvLink;

/// Attempts per request/reply exchange.
pub const ATTEMPTS: u32 = 5;
/// Gap between attempts, ms.
pub const RETRY_DELAY_MS: u64 = 20;
/// Every request is written this many times (MonitorControl and m1ddc do the same).
pub const WRITE_CYCLES: u32 = 2;
/// Wait before each write, ms.
pub const WRITE_DELAY_MS: u64 = 10;
/// Bytes read for one capabilities fragment.
pub const CAPABILITIES_FRAGMENT_LEN: usize = 38;
/// Upper bound on the capability string.
pub const CAPABILITIES_MAX_BYTES: usize = 1024;

/// How the locator tied the display to its DDC service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchMethod {
    /// m1ddc: the display's IODisplayLocation names the framebuffer that precedes the service.
    DisplayLocation,
    /// The preceding framebuffer's DisplayAttributes.ProductAttributes name the target (ManufacturerID + ProductID).
    ProductAttributes,
    /// Fallback: exactly one external DCPAVServiceProxy exists.
    SingleExternal,
}

/// Best-effort port type, from the service's provider class and the framebuffer's Transport.Downstream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacPort {
    Unknown,
    /// DisplayPort / USB-C / Thunderbolt.
    DisplayPort,
    /// Built-in HDMI (MCDP29xx converter, or Transport.Downstream = "HDMI").
    Hdmi,
}

/// I²C addressing for one DDC path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transport {
    /// 7-bit address passed to IOAVService (0x37; 0xB7 on the MCDP29xx HDMI path).
    pub chip_address: u32,
    /// Offset passed to IOAVServiceReadI2C.
    pub read_offset: u32,
    /// Bytes read for a Get VCP reply.
    pub vcp_reply_len: usize,
    /// Wait between the request and the read, ms.
    pub read_wait_ms: u64,
}

impl Transport {
    /// USB-C / DisplayPort, as MonitorControl: chip 0x37, read offset 0, 11 bytes, 50 ms.
    pub const STANDARD: Transport = Transport { chip_address: 0x37, read_offset: 0, vcp_reply_len: 11, read_wait_ms: 50 };
    /// M1 / base-M2 built-in HDMI via MCDP29xx, as m1ddc PR #60: chip 0xB7, read offset 0x51, 12 bytes, 100 ms.
    pub const MCDP29XX: Transport = Transport { chip_address: 0xB7, read_offset: 0x51, vcp_reply_len: 12, read_wait_ms: 100 };
}

/// Framebuffer facts recorded for one external DCPAVServiceProxy during the registry walk.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProxyCandidate {
    pub framebuffer_id: Option<u64>,
    pub manufacturer_id: Option<String>,
    pub product_id: Option<i64>,
    pub downstream: Option<String>,
}

/// Result of one request/reply exchange over all its attempts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExchangeOutcome {
    Ok,
    /// At least one read returned bytes, but none parsed (bad checksum, wrong opcode or code, or "unsupported"
    /// before the checksum form is known).
    InvalidReply,
    /// Every attempt failed with an IOReturn error.
    IoError,
    /// A valid reply said the monitor doesn't implement the code. Final: no retry, no checksum fallback.
    Unsupported,
}

/// True when CoreGraphics' vendor and model numbers are the target's.
pub fn is_target(vendor: u32, model: u32, want_vendor: u16, want_model: u16) -> bool {
    vendor == u32::from(want_vendor) && model == u32::from(want_model)
}

/// Checksum form for the retry after `tried` gave `outcome`, or None to stop. Only while no form is known yet, only
/// from the spec form (0x51 included) to the legacy one, and only when the monitor answered with something
/// unparseable (an I/O error says nothing about the checksum).
pub fn fallback_checksum_mode(remembered: Option<bool>, tried: bool, outcome: ExchangeOutcome) -> Option<bool> {
    (remembered.is_none() && tried && outcome == ExchangeOutcome::InvalidReply).then_some(false)
}

/// What one reply means. `Some(Ok)` takes it; `Some(Unsupported)` stops: a well-formed "unsupported" reply is final
/// once the checksum form is known. Before that it could be the monitor rejecting our checksum form, so it counts as
/// an invalid reply (`None`, keep trying) and the exchange may fall back to the other form.
pub fn reply_outcome(form_known: bool, accepted: bool, says_unsupported: bool) -> Option<ExchangeOutcome> {
    if accepted {
        Some(ExchangeOutcome::Ok)
    } else if form_known && says_unsupported {
        Some(ExchangeOutcome::Unsupported)
    } else {
        None
    }
}

/// Paths to verify on connect: the MCDP29xx variant first when the service's provider says so.
pub fn transport_order(mcdp29xx_hint: bool) -> [Transport; 2] {
    if mcdp29xx_hint {
        [Transport::MCDP29XX, Transport::STANDARD]
    } else {
        [Transport::STANDARD, Transport::MCDP29XX]
    }
}

/// The port the service is on, from the MCDP29xx hint and the framebuffer's Transport.Downstream.
pub fn classify_port(mcdp29xx: bool, downstream: Option<&str>) -> MacPort {
    match downstream {
        _ if mcdp29xx => MacPort::Hdmi,
        Some(d) if d.eq_ignore_ascii_case("HDMI") => MacPort::Hdmi,
        Some(d) if d.eq_ignore_ascii_case("DP") => MacPort::DisplayPort,
        _ => MacPort::Unknown,
    }
}

/// Report "use USB-C" only with evidence of HDMI; otherwise a silent monitor is just not responding.
pub fn is_unsupported_port(port: MacPort) -> bool {
    port == MacPort::Hdmi
}

/// Picks the external service for the target display. In order: the service after the framebuffer whose registry id
/// matches the display's IODisplayLocation; the one service whose framebuffer attributes name the target (two
/// identical monitors are ambiguous); the only external service. None when nothing applies.
pub fn choose_service(
    candidates: &[ProxyCandidate],
    adapter_id: Option<u64>,
    manufacturer_id: &str,
    product_id: u16,
) -> Option<(usize, MatchMethod)> {
    if let Some(id) = adapter_id {
        if let Some(i) = candidates.iter().position(|c| c.framebuffer_id == Some(id)) {
            return Some((i, MatchMethod::DisplayLocation));
        }
    }

    let mut by_attributes = candidates.iter().enumerate().filter(|(_, c)| {
        c.manufacturer_id.as_deref().is_some_and(|m| m.eq_ignore_ascii_case(manufacturer_id))
            && c.product_id == Some(i64::from(product_id))
    });
    if let (Some((i, _)), None) = (by_attributes.next(), by_attributes.next()) {
        return Some((i, MatchMethod::ProductAttributes));
    }

    (candidates.len() == 1).then_some((0, MatchMethod::SingleExternal))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edid;

    #[test]
    fn lenovo_vendor_and_model_identify_the_target() {
        assert_eq!(edid::pnp_letters(0x30AE).as_deref(), Some("LEN"));
        assert!(is_target(0x30AE, 0x62A2, 0x30AE, 0x62A2));
        assert!(!is_target(0x30AE, 0x62A3, 0x30AE, 0x62A2));
    }

    #[test]
    fn built_in_hdmi_is_the_only_unsupported_port() {
        assert!(is_unsupported_port(classify_port(true, None)));
        assert!(is_unsupported_port(classify_port(false, Some("hdmi"))));
        assert!(!is_unsupported_port(classify_port(false, Some("DP"))));
        assert!(!is_unsupported_port(classify_port(false, None)));
    }

    #[test]
    fn checksum_falls_back_once_and_only_on_invalid_replies() {
        assert_eq!(fallback_checksum_mode(None, true, ExchangeOutcome::InvalidReply), Some(false));
        assert_eq!(fallback_checksum_mode(None, true, ExchangeOutcome::IoError), None);
        assert_eq!(fallback_checksum_mode(None, false, ExchangeOutcome::InvalidReply), None);
        assert_eq!(fallback_checksum_mode(Some(true), true, ExchangeOutcome::InvalidReply), None);
        assert_eq!(fallback_checksum_mode(None, true, ExchangeOutcome::Unsupported), None);
    }

    #[test]
    fn unsupported_reply_is_final_only_once_the_checksum_form_is_known() {
        assert_eq!(reply_outcome(true, false, true), Some(ExchangeOutcome::Unsupported));
        assert_eq!(reply_outcome(false, false, true), None); // could be our checksum form: keep today's fallback
        assert_eq!(reply_outcome(true, false, false), None);
        assert_eq!(reply_outcome(false, true, false), Some(ExchangeOutcome::Ok));
        assert_eq!(reply_outcome(true, true, false), Some(ExchangeOutcome::Ok));
    }

    #[test]
    fn service_choice_prefers_location_then_unique_attributes_then_single() {
        let len = |fb: u64| ProxyCandidate {
            framebuffer_id: Some(fb),
            manufacturer_id: Some("LEN".into()),
            product_id: Some(0x62A2),
            downstream: None,
        };
        let other = ProxyCandidate { framebuffer_id: Some(9), ..Default::default() };

        assert_eq!(choose_service(&[other.clone(), len(2)], Some(2), "LEN", 0x62A2), Some((1, MatchMethod::DisplayLocation)));
        assert_eq!(choose_service(&[other.clone(), len(2)], None, "LEN", 0x62A2), Some((1, MatchMethod::ProductAttributes)));
        assert_eq!(choose_service(&[len(1), len(2)], None, "LEN", 0x62A2), None); // two identical monitors
        assert_eq!(choose_service(std::slice::from_ref(&other), None, "LEN", 0x62A2), Some((0, MatchMethod::SingleExternal)));
        assert_eq!(choose_service(&[], Some(1), "LEN", 0x62A2), None);
    }

    #[test]
    fn mcdp_hint_puts_its_transport_first() {
        assert_eq!(transport_order(true)[0], Transport::MCDP29XX);
        assert_eq!(transport_order(false)[0], Transport::STANDARD);
    }
}
