//! macOS display discovery: every online CoreGraphics display, tied to its IOAVService (DCPAVServiceProxy) by the
//! locator's registry walk and `choose_service`.
//!
//! Identity comes from the display's framebuffer, the AppleCLCD2 node CoreDisplay's IODisplayLocation names: an EDID
//! (the info dictionary's `IODisplayEDID`, else an `EDID` or `IODisplayEDID` property at or below the framebuffer),
//! else its DisplayAttributes.ProductAttributes, else CoreGraphics' vendor, model and serial numbers. CoreGraphics'
//! vendor and model cross-check the first two.

use tvc_core::identity::{Discovery, DisplayInfo, ListError, MonitorIdentity};
use tvc_core::link::{DisplayKind, LinkFailure, MonitorLink};

use super::ffi::{self, cf_dict_get, cf_to_bytes, cf_to_i64, cf_to_string, IoObject};
use super::link::IoAvLink;
use super::locator::{is_mcdp29xx, online_displays, walk_external_proxies};
use super::{
    assign_services, backend, choose_service, classify_port, connector_from_location, display_kind, display_target,
    identify, parse_service_target, service_target, split_edid_id, CgNumbers, ProductAttributes, ProxyCandidate,
};

/// Framebuffer properties that may hold the raw EDID, tried in order.
const EDID_KEYS: [&str; 2] = ["EDID", "IODisplayEDID"];

/// One entry per online display; external ones with an IOAVService get the DDC backend.
pub(crate) struct MacDiscovery;

/// What enumeration learned about one CoreGraphics display.
struct Seen {
    cg_id: u32,
    kind: DisplayKind,
    /// The framebuffer's registry id, which `choose_service` matches first.
    adapter_id: Option<u64>,
    identity: MonitorIdentity,
    name: String,
}

impl Discovery for MacDiscovery {
    fn enumerate(&mut self) -> Result<Vec<DisplayInfo>, ListError> {
        let ids = online_displays().map_err(|e| ListError::Failed(format!("CGGetOnlineDisplayList failed ({e})")))?;
        let proxies = walk_external_proxies();
        let candidates: Vec<ProxyCandidate> = proxies.iter().map(|(_, c)| c.clone()).collect();
        let displays: Vec<Seen> = ids.into_iter().map(read_display).collect();
        let choices: Vec<_> = displays
            .iter()
            .map(|d| {
                let (pnp, product) = split_edid_id(&d.identity.edid_id).unwrap_or(("", 0));
                (d.kind == DisplayKind::External)
                    .then(|| choose_service(&candidates, d.adapter_id, pnp, product))
                    .flatten()
            })
            .collect();

        Ok(displays
            .into_iter()
            .zip(assign_services(&choices))
            .map(|(d, service)| {
                let found = service.and_then(|i| {
                    let (entry, info) = &proxies[i];
                    Some((entry.registry_id()?, classify_port(is_mcdp29xx(entry), info.downstream.as_deref())))
                });
                DisplayInfo {
                    target: found.map_or_else(|| display_target(d.cg_id), |(id, _)| service_target(id)),
                    identity: d.identity,
                    name: d.name,
                    kind: d.kind,
                    backend: backend(d.kind, found.map(|(_, port)| port)),
                }
            })
            .collect())
    }

    fn open(&mut self, target: &str) -> Result<Box<dyn MonitorLink>, LinkFailure> {
        let id = parse_service_target(target).ok_or(LinkFailure::NotFound)?;
        Ok(Box::new(IoAvLink::bound(id)))
    }
}

fn read_display(cg_id: u32) -> Seen {
    // SAFETY: plain CoreGraphics queries on an online display id.
    let (builtin, cg) = unsafe {
        (
            ffi::CGDisplayIsBuiltin(cg_id) != 0,
            CgNumbers {
                vendor: ffi::CGDisplayVendorNumber(cg_id),
                model: ffi::CGDisplayModelNumber(cg_id),
                serial: ffi::CGDisplaySerialNumber(cg_id),
            },
        )
    };
    let info = ffi::display_info_dictionary(cg_id);
    let location = cf_to_string(cf_dict_get(info.0, "IODisplayLocation"));
    let framebuffer = location.as_deref().map(IoObject::from_path).filter(|f| f.0 != 0);
    let edid = cf_to_bytes(cf_dict_get(info.0, "IODisplayEDID")).or_else(|| {
        let framebuffer = framebuffer.as_ref()?;
        EDID_KEYS.iter().find_map(|key| cf_to_bytes(framebuffer.search_property(key).0))
    });
    let attributes = framebuffer.as_ref().map(product_attributes).unwrap_or_default();
    let (mut identity, name) = identify(edid.as_deref(), &attributes, cg);
    identity.connector = location.as_deref().and_then(connector_from_location);
    Seen {
        cg_id,
        kind: display_kind(builtin, location.is_some()),
        adapter_id: framebuffer.as_ref().and_then(IoObject::registry_id),
        identity,
        name,
    }
}

fn product_attributes(framebuffer: &IoObject) -> ProductAttributes {
    let attributes = framebuffer.search_property("DisplayAttributes");
    let product = cf_dict_get(attributes.0, "ProductAttributes");
    ProductAttributes {
        manufacturer_id: cf_to_string(cf_dict_get(product, "ManufacturerID")),
        product_id: cf_to_i64(cf_dict_get(product, "ProductID")),
        alphanumeric_serial: cf_to_string(cf_dict_get(product, "AlphanumericSerialNumber")),
        serial_number: cf_to_i64(cf_dict_get(product, "SerialNumber")),
        name: cf_to_string(cf_dict_get(product, "ProductName")),
    }
}
