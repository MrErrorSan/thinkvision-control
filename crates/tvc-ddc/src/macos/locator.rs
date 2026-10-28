//! Finds the IOAVService (DDC endpoint) of the target monitor on Apple Silicon. Approach from m1ddc (MIT): the display's
//! IODisplayLocation names its framebuffer (AppleCLCD2 / IOMobileFramebufferShim); in the recursive IOService-plane walk
//! the DCPAVServiceProxy with Location = "External" that follows that framebuffer is the display's DDC service.

use super::ffi::{self, cf_dict_get, cf_to_i64, cf_to_string, Cf, IoObject};
use super::{choose_service, classify_port, is_target, MacPort, ProxyCandidate};

const MAX_DISPLAYS: usize = 16;
const PROXY_NAME: &str = "DCPAVServiceProxy";

/// A located service: an owned IOAVServiceRef plus what the link needs to know about its port.
pub(crate) struct Lookup {
    pub(crate) service: Cf,
    pub(crate) port: MacPort,
    pub(crate) mcdp29xx: bool,
}

/// Finds the target's DDC service. Err says why not (for logs).
pub(crate) fn locate(io_av: ffi::IoAv, vendor: u16, model: u16, pnp: &str) -> Result<Lookup, String> {
    let displays = online_displays();
    let target = displays
        .iter()
        .copied()
        .find(|&id| {
            // SAFETY: plain CoreGraphics queries on an online display id.
            unsafe {
                ffi::CGDisplayIsBuiltin(id) == 0
                    && is_target(ffi::CGDisplayVendorNumber(id), ffi::CGDisplayModelNumber(id), vendor, model)
            }
        })
        .ok_or_else(|| {
            format!("No external display with vendor 0x{vendor:04X} model 0x{model:04X} among {} online.", displays.len())
        })?;

    let adapter_id = io_display_location(target).and_then(|path| IoObject::from_path(&path).registry_id());
    let proxies = walk_external_proxies();
    let candidates: Vec<ProxyCandidate> = proxies.iter().map(|(_, c)| c.clone()).collect();
    let Some((index, _method)) = choose_service(&candidates, adapter_id, pnp, model) else {
        return Err(if proxies.is_empty() {
            "No external DCPAVServiceProxy (DisplayLink, Intel Mac, or a dock that hides DDC?).".into()
        } else {
            format!("{} external DCPAVServiceProxy entries, none tied to display {target}.", proxies.len())
        });
    };

    let (entry, info) = &proxies[index];
    // SAFETY: a live io_service_t; the result follows the Create rule.
    let service = Cf(unsafe { (io_av.create)(std::ptr::null(), entry.0) });
    if service.0.is_null() {
        return Err("IOAVServiceCreateWithService returned NULL.".into());
    }
    let mcdp29xx = is_mcdp29xx(entry);
    Ok(Lookup { service, port: classify_port(mcdp29xx, info.downstream.as_deref()), mcdp29xx })
}

fn online_displays() -> Vec<u32> {
    let mut ids = [0u32; MAX_DISPLAYS];
    let mut count = 0u32;
    // SAFETY: `ids` holds MAX_DISPLAYS entries and `count` is a valid out-pointer.
    if unsafe { ffi::CGGetOnlineDisplayList(MAX_DISPLAYS as u32, ids.as_mut_ptr(), &mut count) } != 0 {
        return Vec::new();
    }
    ids[..(count as usize).min(MAX_DISPLAYS)].to_vec()
}

/// CoreDisplay's IODisplayLocation for the display: the IOService path of its framebuffer.
fn io_display_location(display: u32) -> Option<String> {
    let info = ffi::display_info_dictionary(display);
    cf_to_string(cf_dict_get(info.0, "IODisplayLocation"))
}

/// Recursive IOService-plane walk from the root: every external DCPAVServiceProxy, with the facts of the framebuffer
/// that precedes it.
fn walk_external_proxies() -> Vec<(IoObject, ProxyCandidate)> {
    let mut found = Vec::new();
    let Some(iterator) = IoObject::root().iterate_recursively() else { return found };
    let mut framebuffer = ProxyCandidate::default();
    while let Some(entry) = iterator.next() {
        let name = entry.name();
        if is_framebuffer(&entry, name.as_deref()) {
            framebuffer = read_framebuffer(&entry);
        } else if name.as_deref() == Some(PROXY_NAME)
            && cf_to_string(entry.search_property("Location").0).as_deref() == Some("External")
        {
            found.push((entry, framebuffer.clone()));
        }
    }
    found
}

// m1ddc tests conformance to IOMobileFramebuffer; MonitorControl matches the names. Either marks a framebuffer.
fn is_framebuffer(entry: &IoObject, name: Option<&str>) -> bool {
    matches!(name, Some("AppleCLCD2" | "IOMobileFramebufferShim")) || entry.conforms_to(c"IOMobileFramebuffer")
}

fn read_framebuffer(entry: &IoObject) -> ProxyCandidate {
    let attributes = entry.search_property("DisplayAttributes");
    let product = cf_dict_get(attributes.0, "ProductAttributes");
    let transport = entry.search_property("Transport");
    ProxyCandidate {
        framebuffer_id: entry.registry_id(),
        manufacturer_id: cf_to_string(cf_dict_get(product, "ManufacturerID")),
        product_id: cf_to_i64(cf_dict_get(product, "ProductID")),
        downstream: cf_to_string(cf_dict_get(transport.0, "Downstream")),
    }
}

/// m1ddc: the proxy's parent has EPICProviderClass = "AppleDCPMCDP29XX" on M1-era built-in HDMI.
fn is_mcdp29xx(proxy: &IoObject) -> bool {
    proxy.parent().is_some_and(|parent| {
        cf_to_string(parent.property("EPICProviderClass").0).as_deref() == Some("AppleDCPMCDP29XX")
    })
}
