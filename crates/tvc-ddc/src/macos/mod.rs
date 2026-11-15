//! macOS IOAVService backend (Apple Silicon). The rules here are pure and compile on every OS; the link and discovery
//! are macOS-only.

use tvc_core::identity::{BackendKind, MonitorIdentity};
use tvc_core::link::DisplayKind;

use crate::edid;

#[cfg(target_os = "macos")]
mod discovery;
#[cfg(target_os = "macos")]
mod ffi;
#[cfg(target_os = "macos")]
pub(crate) use discovery::MacDiscovery;
#[cfg(target_os = "macos")]
mod link;
#[cfg(target_os = "macos")]
mod locator;

/// Attempts per request/reply exchange.
pub const ATTEMPTS: u32 = 5;
/// Gap between attempts, ms.
pub const RETRY_DELAY_MS: u64 = 20;
/// Every Get VCP and capabilities request is written this many times (MonitorControl and m1ddc do the same). A Set VCP
/// is written once: a second copy could switch the input or reset the monitor twice.
pub const WRITE_CYCLES: u32 = 2;
/// Wait before each write, ms.
pub const WRITE_DELAY_MS: u64 = 10;
/// Bytes read for one capabilities fragment.
pub const CAPABILITIES_FRAGMENT_LEN: usize = 38;
/// Upper bound on the capability string.
pub const CAPABILITIES_MAX_BYTES: usize = 8192;

/// How `choose_service` tied the display to its DDC service, strongest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchMethod {
    /// m1ddc: the display's IODisplayLocation names the framebuffer that precedes the service.
    DisplayLocation,
    /// The preceding framebuffer's DisplayAttributes.ProductAttributes name the display (ManufacturerID + ProductID).
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
    /// The monitor sent the DDC/CI null message `packet::NULL_REPLY_LIMIT` times. For a Get VCP that means
    /// unsupported. No checksum fallback: a busy monitor says nothing about the checksum form.
    AllNull,
    /// A valid reply said the monitor doesn't implement the code. Final: no retry, no checksum fallback.
    Unsupported,
}

/// True when CoreGraphics' vendor and model numbers are the wanted ones.
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

/// Picks the external service for a display. In order: the service after the framebuffer whose registry id matches
/// the display's IODisplayLocation; the one service whose framebuffer attributes name the display's EDID id (two
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

/// Which service each display gets, from each display's `choose_service` result. A service goes to the display that
/// chose it by the strongest method; two displays tied at that method both get none (ambiguous).
pub fn assign_services(choices: &[Option<(usize, MatchMethod)>]) -> Vec<Option<usize>> {
    choices
        .iter()
        .enumerate()
        .map(|(i, choice)| {
            let (service, method) = (*choice)?;
            let contested = choices.iter().enumerate().any(|(j, other)| {
                j != i && other.is_some_and(|(s, m)| s == service && m <= method)
            });
            (!contested).then_some(service)
        })
        .collect()
}

/// DisplayAttributes.ProductAttributes of a display's framebuffer (AppleCLCD2), as far as present.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProductAttributes {
    pub manufacturer_id: Option<String>,
    pub product_id: Option<i64>,
    pub alphanumeric_serial: Option<String>,
    pub serial_number: Option<i64>,
    pub name: Option<String>,
}

/// CoreGraphics' numbers for a display; 0 means unknown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CgNumbers {
    pub vendor: u32,
    pub model: u32,
    pub serial: u32,
}

/// PNP id and product code of an EDID id ("LEN62A2" → ("LEN", 0x62A2)). None unless the last four are hex digits.
pub fn split_edid_id(id: &str) -> Option<(&str, u16)> {
    let (pnp, product) = id.split_at_checked(3)?;
    if product.len() != 4 || !product.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u16::from_str_radix(product, 16).ok().map(|p| (pnp, p))
}

/// True when CoreGraphics names the same monitor as `edid_id`, or doesn't know the vendor.
fn agrees_with_cg(edid_id: &str, cg: CgNumbers) -> bool {
    cg.vendor == 0
        || split_edid_id(edid_id)
            .and_then(|(pnp, product)| Some((edid::pack_pnp(pnp)?, product)))
            .is_some_and(|(vendor, product)| is_target(cg.vendor, cg.model, vendor, product))
}

/// Identity (without connector) and name of a display. In order: the EDID, when CoreGraphics agrees it is this
/// display's; the ProductAttributes, on the same condition; CoreGraphics' vendor, model and serial numbers. The name is
/// the EDID's, else the ProductAttributes', else empty.
pub fn identify(raw_edid: Option<&[u8]>, attributes: &ProductAttributes, cg: CgNumbers) -> (MonitorIdentity, String) {
    let attribute_name = || attributes.name.clone().filter(|n| !n.is_empty()).unwrap_or_default();
    if let Some((bytes, id)) = raw_edid.and_then(|e| Some((e, edid::id(e)?))).filter(|(_, id)| agrees_with_cg(id, cg)) {
        let name = edid::display_name(bytes).unwrap_or_else(attribute_name);
        return (MonitorIdentity { edid_id: id, serial: edid::serial(bytes), connector: None }, name);
    }
    let from_attributes = attributes
        .manufacturer_id
        .as_deref()
        .filter(|m| m.len() == 3)
        .zip(attributes.product_id.and_then(|p| u16::try_from(p).ok()))
        .map(|(m, p)| format!("{}{p:04X}", m.to_ascii_uppercase()))
        .filter(|id| agrees_with_cg(id, cg));
    if let Some(id) = from_attributes {
        let serial = attributes
            .alphanumeric_serial
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| attributes.serial_number.filter(|&n| n != 0).map(|n| n.to_string()));
        return (MonitorIdentity { edid_id: id, serial, connector: None }, attribute_name());
    }
    let id = u16::try_from(cg.vendor)
        .ok()
        .and_then(edid::pnp_letters)
        .zip(u16::try_from(cg.model).ok())
        .map(|(pnp, model)| format!("{pnp}{model:04X}"))
        .unwrap_or_default();
    let serial = (cg.serial != 0).then(|| cg.serial.to_string());
    (MonitorIdentity { edid_id: id, serial, connector: None }, attribute_name())
}

/// The display-pipe node in an IODisplayLocation path ("…/dispext0@…/…" → "dispext0"), stable per port on Apple
/// Silicon. None when the path has no `disp<N>` / `dispext<N>` segment (Intel Macs).
pub fn connector_from_location(path: &str) -> Option<String> {
    path.rsplit('/').map(|segment| segment.split_once('@').map_or(segment, |(name, _)| name)).find_map(|name| {
        let digits = name.strip_prefix("dispext").or_else(|| name.strip_prefix("disp"))?;
        (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then(|| name.to_owned())
    })
}

/// Built-in when CoreGraphics says so; Virtual (AirPlay, Sidecar, DisplayLink) when CoreDisplay names no framebuffer
/// for it; else External.
pub fn display_kind(builtin: bool, has_location: bool) -> DisplayKind {
    match (builtin, has_location) {
        (true, _) => DisplayKind::BuiltIn,
        (false, false) => DisplayKind::Virtual,
        (false, true) => DisplayKind::External,
    }
}

/// DDC for an external display with a service on a port that carries DDC/CI; nothing else is controllable here.
pub fn backend(kind: DisplayKind, port: Option<MacPort>) -> BackendKind {
    match port {
        Some(port) if kind == DisplayKind::External && !is_unsupported_port(port) => BackendKind::Ddc,
        _ => BackendKind::None,
    }
}

/// The `DisplayInfo::target` of a display with a DDC service: the service's IORegistry entry id.
pub fn service_target(registry_id: u64) -> String {
    registry_id.to_string()
}

/// The target of a display without a service: never opens.
pub fn display_target(cg_display: u32) -> String {
    format!("cg:{cg_display}")
}

/// The registry entry id in a [`service_target`]; None for anything else.
pub fn parse_service_target(target: &str) -> Option<u64> {
    target.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edid;
    use crate::edid::testing::p32p20;

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

    use MatchMethod::{DisplayLocation, ProductAttributes as Attributes, SingleExternal};

    #[test]
    fn the_stronger_method_wins_a_service() {
        assert_eq!(assign_services(&[Some((0, DisplayLocation)), Some((0, Attributes))]), [Some(0), None]);
        assert_eq!(assign_services(&[Some((0, SingleExternal)), Some((0, Attributes))]), [None, Some(0)]);
        assert_eq!(
            assign_services(&[Some((0, DisplayLocation)), Some((1, Attributes)), None]),
            [Some(0), Some(1), None]
        );
        assert_eq!(assign_services(&[]), Vec::<Option<usize>>::new());
    }

    #[test]
    fn a_tie_at_the_strongest_method_gives_the_service_to_no_one() {
        assert_eq!(assign_services(&[Some((1, SingleExternal)), Some((1, SingleExternal))]), [None, None]);
        // The weaker third claim loses too.
        assert_eq!(
            assign_services(&[Some((0, Attributes)), Some((0, Attributes)), Some((0, SingleExternal))]),
            [None, None, None]
        );
        // Another service is unaffected.
        assert_eq!(
            assign_services(&[Some((0, Attributes)), Some((0, Attributes)), Some((2, DisplayLocation))]),
            [None, None, Some(2)]
        );
    }

    const LEN: u32 = 0x30AE;
    const DEL: u32 = 0x10AC;

    fn lenovo_attributes() -> ProductAttributes {
        ProductAttributes {
            manufacturer_id: Some("len".into()),
            product_id: Some(0x62A2),
            alphanumeric_serial: Some("V90A1234".into()),
            serial_number: Some(5),
            name: Some("ThinkVision P32p-20".into()),
        }
    }

    #[test]
    fn identity_comes_from_the_edid_when_core_graphics_agrees() {
        let edid = p32p20(1234, Some("P32p-20"));
        let cg = CgNumbers { vendor: LEN, model: 0x62A2, serial: 99 };
        let (identity, name) = identify(Some(&edid), &lenovo_attributes(), cg);
        assert_eq!(identity, MonitorIdentity { edid_id: "LEN62A2".into(), serial: Some("1234".into()), connector: None });
        assert_eq!(name, "P32p-20");
        // No name in the EDID: the attributes' name.
        let (_, name) = identify(Some(&p32p20(1234, None)), &lenovo_attributes(), cg);
        assert_eq!(name, "ThinkVision P32p-20");
    }

    #[test]
    fn a_vendor_of_zero_accepts_the_edid() {
        let edid = p32p20(1234, Some("P32p-20"));
        let (identity, name) = identify(Some(&edid), &ProductAttributes::default(), CgNumbers::default());
        assert_eq!(identity.edid_id, "LEN62A2");
        assert_eq!(identity.serial.as_deref(), Some("1234"));
        assert_eq!(name, "P32p-20");
    }

    #[test]
    fn identity_falls_back_when_core_graphics_contradicts() {
        let edid = p32p20(1234, Some("P32p-20"));
        let dell = CgNumbers { vendor: DEL, model: 0x4123, serial: 99 };
        // Both the EDID and the attributes name a Lenovo: CoreGraphics' numbers win, with the attributes' name.
        let (identity, name) = identify(Some(&edid), &lenovo_attributes(), dell);
        assert_eq!(identity, MonitorIdentity { edid_id: "DEL4123".into(), serial: Some("99".into()), connector: None });
        assert_eq!(name, "ThinkVision P32p-20");
        // The EDID is someone else's but the attributes agree: the attributes.
        let lenovo = CgNumbers { vendor: LEN, model: 0x62A2, serial: 0 };
        let dell_edid = edid::testing::edid([0x10, 0xAC, 0x23, 0x41], 7, Some("DELL U2720Q"));
        let (identity, name) = identify(Some(&dell_edid), &lenovo_attributes(), lenovo);
        assert_eq!(identity.edid_id, "LEN62A2");
        assert_eq!(identity.serial.as_deref(), Some("V90A1234"));
        assert_eq!(name, "ThinkVision P32p-20");
    }

    #[test]
    fn attribute_serial_prefers_the_text_then_a_non_zero_number() {
        let cg = CgNumbers { vendor: LEN, model: 0x62A2, serial: 99 };
        let (identity, _) = identify(None, &lenovo_attributes(), cg);
        assert_eq!(identity, MonitorIdentity { edid_id: "LEN62A2".into(), serial: Some("V90A1234".into()), connector: None });
        let numbered = ProductAttributes { alphanumeric_serial: Some(String::new()), ..lenovo_attributes() };
        assert_eq!(identify(None, &numbered, cg).0.serial.as_deref(), Some("5"));
        let none = ProductAttributes { alphanumeric_serial: None, serial_number: Some(0), ..lenovo_attributes() };
        assert_eq!(identify(None, &none, cg).0.serial, None);
        // Malformed attributes are skipped for CoreGraphics' numbers.
        let bad = ProductAttributes { manufacturer_id: Some("LENOVO".into()), ..lenovo_attributes() };
        assert_eq!(identify(None, &bad, cg).0, MonitorIdentity { edid_id: "LEN62A2".into(), serial: Some("99".into()), connector: None });
    }

    #[test]
    fn identity_from_core_graphics_numbers() {
        let none = ProductAttributes::default();
        let (identity, name) = identify(None, &none, CgNumbers { vendor: LEN, model: 0x62A2, serial: 0 });
        assert_eq!(identity, MonitorIdentity { edid_id: "LEN62A2".into(), serial: None, connector: None });
        assert_eq!(name, "");
        assert_eq!(identify(None, &none, CgNumbers { vendor: LEN, model: 0x62A2, serial: 42 }).0.serial.as_deref(), Some("42"));
        // Unknown or out-of-range numbers give no EDID id.
        assert_eq!(identify(None, &none, CgNumbers::default()).0.edid_id, "");
        assert_eq!(identify(None, &none, CgNumbers { vendor: 0x1_0000, model: 1, serial: 0 }).0.edid_id, "");
        assert_eq!(identify(None, &none, CgNumbers { vendor: LEN, model: 0x1_0000, serial: 0 }).0.edid_id, "");
    }

    #[test]
    fn connector_is_the_display_pipe_node() {
        let m1 = "IOService:/AppleARMPE/arm-io@10F00000/AppleT810xIO/dispext0@F2000000/AppleCLCD2";
        assert_eq!(connector_from_location(m1).as_deref(), Some("dispext0"));
        assert_eq!(connector_from_location("IOService:/AppleARMPE/arm-io/disp0@0/AppleCLCD2").as_deref(), Some("disp0"));
        assert_eq!(connector_from_location("IOService:/x/dispext12").as_deref(), Some("dispext12"));
        let intel = "IOService:/AppleACPIPlatformExpert/PCI0@0/AppleACPIPCI/IGPU@2/AppleIntelFramebuffer@0";
        assert_eq!(connector_from_location(intel), None);
        assert_eq!(connector_from_location("IOService:/x/display0@1"), None);
        assert_eq!(connector_from_location("IOService:/x/dispext@1"), None);
        assert_eq!(connector_from_location(""), None);
    }

    #[test]
    fn display_kind_from_builtin_and_location() {
        assert_eq!(display_kind(true, true), DisplayKind::BuiltIn);
        assert_eq!(display_kind(true, false), DisplayKind::BuiltIn);
        assert_eq!(display_kind(false, true), DisplayKind::External);
        assert_eq!(display_kind(false, false), DisplayKind::Virtual);
    }

    #[test]
    fn only_an_external_display_with_a_ddc_port_gets_the_backend() {
        assert_eq!(backend(DisplayKind::External, Some(MacPort::DisplayPort)), BackendKind::Ddc);
        assert_eq!(backend(DisplayKind::External, Some(MacPort::Unknown)), BackendKind::Ddc);
        assert_eq!(backend(DisplayKind::External, Some(MacPort::Hdmi)), BackendKind::None);
        assert_eq!(backend(DisplayKind::External, None), BackendKind::None);
        assert_eq!(backend(DisplayKind::BuiltIn, Some(MacPort::DisplayPort)), BackendKind::None);
        assert_eq!(backend(DisplayKind::Virtual, Some(MacPort::DisplayPort)), BackendKind::None);
    }

    #[test]
    fn service_targets_round_trip_and_display_targets_never_open() {
        assert_eq!(parse_service_target(&service_target(4_294_968_123)), Some(4_294_968_123));
        assert_eq!(parse_service_target(&service_target(0)), Some(0));
        assert_eq!(display_target(3), "cg:3");
        assert_eq!(parse_service_target(&display_target(3)), None);
        assert_eq!(parse_service_target(""), None);
        assert_eq!(parse_service_target("-1"), None);
    }

    #[test]
    fn edid_ids_split_into_vendor_and_product() {
        assert_eq!(split_edid_id("LEN62A2"), Some(("LEN", 0x62A2)));
        assert_eq!(split_edid_id("del4123"), Some(("del", 0x4123)));
        assert_eq!(split_edid_id("LEN62A"), None);
        assert_eq!(split_edid_id("LEN62A23"), None);
        assert_eq!(split_edid_id("LEN62AZ"), None);
        assert_eq!(split_edid_id("LEN+62A"), None);
        assert_eq!(split_edid_id(""), None);
        assert_eq!(split_edid_id("ÄÄ62A"), None); // 7 bytes, but byte 3 isn't a character boundary
    }
}
