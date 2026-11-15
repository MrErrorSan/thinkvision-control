//! Lists the IOAVServices (DDC endpoints) on Apple Silicon and re-finds one by registry id. Approach from m1ddc (MIT):
//! the display's IODisplayLocation names its framebuffer (AppleCLCD2 / IOMobileFramebufferShim); in the recursive
//! IOService-plane walk the DCPAVServiceProxy with Location = "External" that follows that framebuffer is the
//! display's DDC service. Discovery pairs displays with services through `choose_service`.

use super::ffi::{self, cf_dict_get, cf_to_i64, cf_to_string, Cf, IoObject};
use super::{classify_port, MacPort, ProxyCandidate};

const MAX_DISPLAYS: usize = 16;
const PROXY_NAME: &str = "DCPAVServiceProxy";

/// A located service: an owned IOAVServiceRef plus what the link needs to know about its port.
pub(crate) struct Lookup {
    pub(crate) service: Cf,
    pub(crate) port: MacPort,
    pub(crate) mcdp29xx: bool,
}

/// Re-finds a service [`walk_external_proxies`] listed, by its IORegistry entry id.
pub(crate) fn locate_by_id(io_av: ffi::IoAv, registry_id: u64) -> Result<Lookup, String> {
    let proxies = walk_external_proxies();
    let (entry, info) = proxies
        .iter()
        .find(|(entry, _)| entry.registry_id() == Some(registry_id))
        .ok_or_else(|| format!("No external DCPAVServiceProxy with registry id {registry_id}."))?;
    // SAFETY: a live io_service_t; the result follows the Create rule.
    let service = Cf(unsafe { (io_av.create)(std::ptr::null(), entry.0) });
    if service.0.is_null() {
        return Err("IOAVServiceCreateWithService returned NULL.".into());
    }
    let mcdp29xx = is_mcdp29xx(entry);
    Ok(Lookup { service, port: classify_port(mcdp29xx, info.downstream.as_deref()), mcdp29xx })
}

/// Every online CoreGraphics display id, or the CGError.
pub(super) fn online_displays() -> Result<Vec<u32>, i32> {
    let mut ids = [0u32; MAX_DISPLAYS];
    let mut count = 0u32;
    // SAFETY: `ids` holds MAX_DISPLAYS entries and `count` is a valid out-pointer.
    let error = unsafe { ffi::CGGetOnlineDisplayList(MAX_DISPLAYS as u32, ids.as_mut_ptr(), &mut count) };
    if error != 0 {
        return Err(error);
    }
    Ok(ids[..(count as usize).min(MAX_DISPLAYS)].to_vec())
}

/// Recursive IOService-plane walk from the root: every external DCPAVServiceProxy, with the facts of the framebuffer
/// that precedes it.
pub(super) fn walk_external_proxies() -> Vec<(IoObject, ProxyCandidate)> {
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
pub(super) fn is_mcdp29xx(proxy: &IoObject) -> bool {
    proxy.parent().is_some_and(|parent| {
        cf_to_string(parent.property("EPICProviderClass").0).as_deref() == Some("AppleDCPMCDP29XX")
    })
}
