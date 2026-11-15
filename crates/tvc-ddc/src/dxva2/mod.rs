//! DDC/CI over dxva2. One enumeration finds every display: each active QueryDisplayConfig target, its GDI source
//! (`\\.\DISPLAY1`), the HMONITOR with that device name, and that HMONITOR's physical monitor. [`Dxva2Discovery`]
//! binds each link to one monitor device path.
//! dxva2 has no permission or driver failure modes worth surfacing: the monitor is either there or not.

mod join;

use std::mem::size_of;
use std::ptr::{null, null_mut};

use tvc_core::identity::{BackendKind, Discovery, DisplayInfo, ListError, MonitorIdentity};
use tvc_core::link::{is_virtual_vendor, DisplayKind, LinkError, LinkFailure, MonitorLink, VcpReading};
use windows_sys::core::BOOL;
use windows_sys::Win32::Devices::Display::{
    CapabilitiesRequestAndCapabilitiesReply, DestroyPhysicalMonitors, DisplayConfigGetDeviceInfo,
    GetCapabilitiesStringLength, GetDisplayConfigBufferSizes, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, GetVCPFeatureAndVCPFeatureReply, QueryDisplayConfig, SetVCPFeature,
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_MODE_INFO,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INDIRECT_VIRTUAL,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INDIRECT_WIRED, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_MIRACAST,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_OTHER, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY,
    PHYSICAL_MONITOR, QDC_ONLY_ACTIVE_PATHS,
};
use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_GRAPHICS_DDCCI_VCP_NOT_SUPPORTED, ERROR_GRAPHICS_INVALID_PHYSICAL_MONITOR_HANDLE,
    ERROR_GRAPHICS_MONITOR_NO_LONGER_EXISTS, ERROR_INSUFFICIENT_BUFFER, ERROR_INVALID_HANDLE, ERROR_SUCCESS, HANDLE,
    LPARAM, RECT,
};
use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW};
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_BINARY};

use crate::{ascii, edid, not_connected};
use join::{connector, edid_id_from_ids, edid_id_from_path, instance_key, pick_physical};

/// `edidIdsValid` in `DISPLAYCONFIG_TARGET_DEVICE_NAME_FLAGS` (after `friendlyNameFromEdid` and `friendlyNameForced`).
const EDID_IDS_VALID: u32 = 1 << 2;

/// The dxva2 link: one physical-monitor handle while connected.
pub(crate) struct Dxva2Link {
    /// The monitor device path `connect` reopens.
    path: String,
    held: Option<Handle>,
}

/// An owned physical-monitor handle, destroyed on drop.
struct Handle(HANDLE);

// SAFETY: a physical-monitor handle is an opaque token dxva2 accepts from any thread; the engine uses it from one.
unsafe impl Send for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        destroy(&[physical(self.0)]);
    }
}

impl Dxva2Link {
    fn bound(path: String) -> Self {
        Dxva2Link { path, held: None }
    }

    fn handle(&self) -> Result<HANDLE, LinkError> {
        self.held.as_ref().map(|h| h.0).ok_or_else(not_connected)
    }

    fn is_target(&self, display: &Joined) -> bool {
        !self.path.is_empty() && display.path.eq_ignore_ascii_case(&self.path)
    }
}

impl MonitorLink for Dxva2Link {
    fn connect(&mut self) -> Result<(), LinkFailure> {
        self.held = None;
        let all = joined().unwrap_or_default();
        self.held = all.iter().filter(|d| self.is_target(d)).find_map(|d| open_physical(d, &all));
        if self.held.is_some() {
            Ok(())
        } else {
            Err(LinkFailure::NotFound)
        }
    }

    fn disconnect(&mut self) {
        self.held = None;
    }

    fn capabilities(&mut self) -> Result<String, LinkError> {
        let handle = self.handle()?;
        let mut length = 0u32;
        // SAFETY: a live physical-monitor handle and a valid out-pointer.
        if unsafe { GetCapabilitiesStringLength(handle, &mut length) } == 0 {
            return Err(failure("GetCapabilitiesStringLength", None));
        }
        let mut buffer = vec![0u8; length as usize];
        // SAFETY: the buffer holds exactly `length` bytes.
        if unsafe { CapabilitiesRequestAndCapabilitiesReply(handle, buffer.as_mut_ptr(), length) } == 0 {
            return Err(failure("CapabilitiesRequestAndCapabilitiesReply", None));
        }
        Ok(ascii(&buffer).trim_end_matches('\0').to_owned())
    }

    fn read(&mut self, code: u8) -> Result<VcpReading, LinkError> {
        let handle = self.handle()?;
        let (mut current, mut max) = (0u32, 0u32);
        // SAFETY: a live handle and valid out-pointers; the code-type pointer is optional.
        if unsafe { GetVCPFeatureAndVCPFeatureReply(handle, code, null_mut(), &mut current, &mut max) } == 0 {
            return Err(failure("GetVCPFeatureAndVCPFeatureReply", Some(code)));
        }
        let clamp = |v: u32| u16::try_from(v).unwrap_or(u16::MAX);
        Ok(VcpReading { current: clamp(current), max: clamp(max) })
    }

    fn write(&mut self, code: u8, value: u16) -> Result<(), LinkError> {
        let handle = self.handle()?;
        // SAFETY: a live physical-monitor handle.
        if unsafe { SetVCPFeature(handle, code, value.into()) } == 0 {
            return Err(failure("SetVCPFeature", Some(code)));
        }
        Ok(())
    }
}

/// The error for a failed dxva2 call, with GetLastError.
fn failure(call: &str, code: Option<u8>) -> LinkError {
    // SAFETY: no preconditions.
    let error = unsafe { GetLastError() };
    let code = code.map(|c| format!(" 0x{c:02X}")).unwrap_or_default();
    classify(error, format!("{call}{code} failed (Win32 error 0x{error:08X})"))
}

/// Sorts a dxva2 error (after PowerToys' `DdcErrorClassifier`, MIT):
/// - `Unsupported`: the monitor answered that it doesn't implement the code (`ERROR_GRAPHICS_DDCCI_VCP_NOT_SUPPORTED`).
/// - `Gone`: the physical-monitor handle is dead (invalid handle, monitor no longer exists); reconnect.
/// - `Transient`: everything else. That covers the I²C transmit/receive errors, the DDC/CI framing errors (invalid
///   data, message command, length, checksum), current > maximum (also what a corrupted reply produces),
///   `ERROR_TIMEOUT`, MCA internal/capabilities-string errors, and the connection-level codes
///   `I2C_NOT_SUPPORTED`, `I2C_DEVICE_DOES_NOT_EXIST` and `ONLY_CONSOLE_SESSION_SUPPORTED`. PowerToys stops on the
///   two I²C ones, but they describe the whole bus rather than one code (so not `Unsupported`), a reconnect finds the
///   same handle (so not `Gone`), and a sleeping monitor or a dock can raise them briefly; the engine's retry budget
///   and reconnect schedule bound the cost.
fn classify(error: u32, message: String) -> LinkError {
    const GONE: [u32; 3] = [
        ERROR_INVALID_HANDLE,
        ERROR_GRAPHICS_INVALID_PHYSICAL_MONITOR_HANDLE as u32,
        ERROR_GRAPHICS_MONITOR_NO_LONGER_EXISTS as u32,
    ];
    if error == ERROR_GRAPHICS_DDCCI_VCP_NOT_SUPPORTED as u32 {
        LinkError::Unsupported(message)
    } else if GONE.contains(&error) {
        LinkError::Gone(message)
    } else {
        LinkError::Transient(message)
    }
}

fn physical(handle: HANDLE) -> PHYSICAL_MONITOR {
    PHYSICAL_MONITOR { hPhysicalMonitor: handle, szPhysicalMonitorDescription: [0; 128] }
}

fn destroy(monitors: &[PHYSICAL_MONITOR]) {
    if !monitors.is_empty() {
        // SAFETY: every entry holds a handle we own and no longer use.
        unsafe { DestroyPhysicalMonitors(monitors.len() as u32, monitors.as_ptr()) };
    }
}

fn enumerate_monitors() -> Vec<HMONITOR> {
    unsafe extern "system" fn collect(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the `&mut Vec` passed below, alive for the whole enumeration.
        unsafe { (*(data as *mut Vec<HMONITOR>)).push(monitor) };
        1
    }
    let mut monitors: Vec<HMONITOR> = Vec::new();
    // SAFETY: the callback only pushes into `monitors`, which outlives the synchronous call.
    unsafe { EnumDisplayMonitors(null_mut(), null(), Some(collect), &mut monitors as *mut _ as LPARAM) };
    monitors
}

fn physical_monitors(monitor: HMONITOR) -> Vec<PHYSICAL_MONITOR> {
    let mut count = 0u32;
    // SAFETY: a monitor handle from EnumDisplayMonitors and a valid out-pointer.
    if unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(monitor, &mut count) } == 0 || count == 0 {
        return Vec::new();
    }
    let mut monitors = vec![physical(null_mut()); count as usize];
    // SAFETY: the array holds `count` entries.
    if unsafe { GetPhysicalMonitorsFromHMONITOR(monitor, count, monitors.as_mut_ptr()) } == 0 {
        return Vec::new();
    }
    monitors
}

fn description(monitor: &PHYSICAL_MONITOR) -> String {
    let description = { monitor.szPhysicalMonitorDescription }; // copy out of the packed struct
    wide_to_string(&description)
}

/// One active DisplayConfig target, joined to its HMONITOR through its source's GDI device name.
struct Joined {
    /// The monitor device path, e.g. `\\?\DISPLAY#LEN62A2#5&2a1b&0&UID4352#{e6f07b5f-...}`.
    path: String,
    friendly: String,
    /// From the device path, else from the target's EDID ids; empty when neither gives one.
    edid_id: String,
    technology: DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY,
    /// None when no HMONITOR carries the source's GDI device name.
    monitor: Option<HMONITOR>,
}

/// Every active target, joined. None if QueryDisplayConfig fails.
fn joined() -> Option<Vec<Joined>> {
    let paths = active_paths()?;
    let monitors = gdi_monitors();
    let joined = paths
        .iter()
        .filter_map(|p| {
            let name = target_name(p)?;
            let gdi = source_name(p);
            let monitor = gdi.and_then(|gdi| monitors.iter().find(|(d, _)| d.eq_ignore_ascii_case(&gdi)).map(|m| m.1));
            let path = wide_to_string(&name.monitorDevicePath);
            // SAFETY: both union views are a u32 bitfield.
            let ids_valid = unsafe { name.flags.Anonymous.value } & EDID_IDS_VALID != 0;
            // The path's PnP id is what Windows derives from the same EDID; the ids fill in when the path has none.
            let edid_id = edid_id_from_path(&path)
                .or_else(|| edid_id_from_ids(name.edidManufactureId, name.edidProductCodeId).filter(|_| ids_valid))
                .unwrap_or_default();
            Some(Joined {
                path,
                friendly: wide_to_string(&name.monitorFriendlyDeviceName),
                edid_id,
                technology: name.outputTechnology,
                monitor,
            })
        })
        .collect();
    Some(joined)
}

/// The active display paths. None if the query fails.
fn active_paths() -> Option<Vec<DISPLAYCONFIG_PATH_INFO>> {
    // The path count can grow between the size query and the query itself (a monitor plugged in): retry then.
    for _ in 0..3 {
        let (mut path_count, mut mode_count) = (0u32, 0u32);
        // SAFETY: valid out-pointers.
        if unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count) } != ERROR_SUCCESS {
            return None;
        }
        // SAFETY: both are plain C structs (integers, LUIDs, unions of integers); all-zero is a valid value.
        let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> = vec![unsafe { std::mem::zeroed() }; path_count as usize];
        let mut modes: Vec<DISPLAYCONFIG_MODE_INFO> = vec![unsafe { std::mem::zeroed() }; mode_count as usize];
        // SAFETY: the arrays hold the counts passed; the topology pointer may be null without QDC_DATABASE_CURRENT.
        let status = unsafe {
            QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &mut path_count, paths.as_mut_ptr(), &mut mode_count,
                modes.as_mut_ptr(), null_mut())
        };
        if status == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if status != ERROR_SUCCESS {
            return None;
        }
        paths.truncate(path_count as usize);
        return Some(paths);
    }
    None
}

/// The path's target name: device path, friendly name, EDID ids, connector type.
fn target_name(path: &DISPLAYCONFIG_PATH_INFO) -> Option<DISPLAYCONFIG_TARGET_DEVICE_NAME> {
    // SAFETY: zeroed POD; the header names the request, the size of the struct it starts, and the target.
    let mut name: DISPLAYCONFIG_TARGET_DEVICE_NAME = unsafe { std::mem::zeroed() };
    name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
    name.header.size = size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
    name.header.adapterId = path.targetInfo.adapterId;
    name.header.id = path.targetInfo.id;
    // SAFETY: the header starts a DISPLAYCONFIG_TARGET_DEVICE_NAME of the size it states.
    (unsafe { DisplayConfigGetDeviceInfo(&mut name.header) } == ERROR_SUCCESS as i32).then_some(name)
}

/// The GDI device name of the path's source, e.g. `\\.\DISPLAY1`.
fn source_name(path: &DISPLAYCONFIG_PATH_INFO) -> Option<String> {
    // SAFETY: zeroed POD; the header names the request, the size of the struct it starts, and the source.
    let mut name: DISPLAYCONFIG_SOURCE_DEVICE_NAME = unsafe { std::mem::zeroed() };
    name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
    name.header.size = size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
    name.header.adapterId = path.sourceInfo.adapterId;
    name.header.id = path.sourceInfo.id;
    // SAFETY: the header starts a DISPLAYCONFIG_SOURCE_DEVICE_NAME of the size it states.
    if unsafe { DisplayConfigGetDeviceInfo(&mut name.header) } != ERROR_SUCCESS as i32 {
        return None;
    }
    Some(wide_to_string(&name.viewGdiDeviceName)).filter(|n| !n.is_empty())
}

/// Every HMONITOR with its GDI device name (`MONITORINFOEXW::szDevice`).
fn gdi_monitors() -> Vec<(String, HMONITOR)> {
    enumerate_monitors()
        .into_iter()
        .filter_map(|monitor| {
            // SAFETY: zeroed POD with its size field set; MONITORINFOEXW starts with MONITORINFO.
            unsafe {
                let mut info: MONITORINFOEXW = std::mem::zeroed();
                info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
                (GetMonitorInfoW(monitor, &mut info as *mut MONITORINFOEXW as *mut _) != 0)
                    .then(|| (wide_to_string(&info.szDevice), monitor))
            }
        })
        .collect()
}

/// The physical monitor of `display`, from its HMONITOR alone; the others that HMONITOR returned are destroyed.
/// None when it has no HMONITOR or [`pick_physical`] can't tell which one it is.
fn open_physical(display: &Joined, all: &[Joined]) -> Option<Handle> {
    let monitor = display.monitor?;
    let mut physical = physical_monitors(monitor);
    let descriptions: Vec<String> = physical.iter().map(description).collect();
    let names: Vec<&str> = all.iter().filter(|d| d.monitor == Some(monitor)).map(|d| d.friendly.as_str()).collect();
    let picked = pick_physical(&descriptions, &names, &display.friendly);
    let held = picked.map(|i| Handle(physical.remove(i).hPhysicalMonitor));
    destroy(&physical);
    held
}

/// The EDID Windows stored for the device path, when its base block checks out.
fn registry_edid(path: &str) -> Option<Vec<u8>> {
    let key = wide(&format!(r"SYSTEM\CurrentControlSet\Enum\{}\Device Parameters", instance_key(path)?));
    let value = wide("EDID");
    let mut length = 0u32;
    // SAFETY: NUL-terminated key and value names; a null buffer asks for the size only.
    let status = unsafe {
        RegGetValueW(HKEY_LOCAL_MACHINE, key.as_ptr(), value.as_ptr(), RRF_RT_REG_BINARY, null_mut(), null_mut(),
            &mut length)
    };
    if status != ERROR_SUCCESS || length == 0 {
        return None;
    }
    let mut data = vec![0u8; length as usize];
    // SAFETY: the buffer holds `length` bytes, which RegGetValueW is told.
    let status = unsafe {
        RegGetValueW(HKEY_LOCAL_MACHINE, key.as_ptr(), value.as_ptr(), RRF_RT_REG_BINARY, null_mut(),
            data.as_mut_ptr().cast(), &mut length)
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    data.truncate(length as usize);
    edid::checksum_ok(&data).then_some(data)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Built in for laptop panels (internal, embedded DisplayPort/UDI, LVDS); virtual for indirect targets (virtual,
/// DisplayLink USB, Miracast: none carries DDC/CI), unclassified ones without an EDID id, and hypervisor/remote-desktop
/// vendors; otherwise external.
fn display_kind(technology: DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY, edid_id: &str) -> DisplayKind {
    match technology {
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL
        | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED
        | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED
        | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS => DisplayKind::BuiltIn,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INDIRECT_VIRTUAL
        | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INDIRECT_WIRED
        | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_MIRACAST => DisplayKind::Virtual,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_OTHER if edid_id.is_empty() => DisplayKind::Virtual,
        _ if is_virtual_vendor(edid_id) => DisplayKind::Virtual,
        _ => DisplayKind::External,
    }
}

fn wide_to_string(wide: &[u16]) -> String {
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}

/// Windows displays: every active QueryDisplayConfig target, its `target` the monitor device path.
pub(crate) struct Dxva2Discovery;

impl Dxva2Discovery {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl Discovery for Dxva2Discovery {
    fn enumerate(&mut self) -> Result<Vec<DisplayInfo>, ListError> {
        let all = joined().ok_or_else(|| ListError::Failed("QueryDisplayConfig failed".into()))?;
        Ok(all.iter().map(|d| display_info(d, &all)).collect())
    }

    fn open(&mut self, target: &str) -> Result<Box<dyn MonitorLink>, LinkFailure> {
        Ok(Box::new(Dxva2Link::bound(target.to_owned())))
    }
}

/// Identity from the target and the registry EDID; DDC only for an external monitor whose physical monitor is known.
/// A clone (several targets on one HMONITOR) whose physical monitor can't be told apart is listed without a backend.
fn display_info(display: &Joined, all: &[Joined]) -> DisplayInfo {
    let edid = registry_edid(&display.path);
    let kind = display_kind(display.technology, &display.edid_id);
    let cloned = display.monitor.is_some() && all.iter().filter(|d| d.monitor == display.monitor).count() > 1;
    let ambiguous = kind == DisplayKind::External && cloned && open_physical(display, all).is_none();
    if ambiguous {
        let path = &display.path;
        tvc_core::log::write(&format!("dxva2: {path} is cloned and its physical monitor can't be told apart; no DDC"));
    }
    DisplayInfo {
        target: display.path.clone(),
        identity: MonitorIdentity {
            edid_id: display.edid_id.clone(),
            serial: edid.as_deref().and_then(edid::serial),
            connector: connector(&display.path),
        },
        name: edid.as_deref().and_then(edid::display_name).unwrap_or_else(|| display.friendly.clone()),
        kind,
        backend: if kind == DisplayKind::External && !ambiguous { BackendKind::Ddc } else { BackendKind::None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::{
        ERROR_GRAPHICS_DDCCI_CURRENT_CURRENT_VALUE_GREATER_THAN_MAXIMUM_VALUE, ERROR_GRAPHICS_DDCCI_INVALID_DATA,
        ERROR_GRAPHICS_DDCCI_INVALID_MESSAGE_CHECKSUM, ERROR_GRAPHICS_DDCCI_INVALID_MESSAGE_COMMAND,
        ERROR_GRAPHICS_DDCCI_INVALID_MESSAGE_LENGTH, ERROR_GRAPHICS_I2C_DEVICE_DOES_NOT_EXIST,
        ERROR_GRAPHICS_I2C_ERROR_RECEIVING_DATA, ERROR_GRAPHICS_I2C_ERROR_TRANSMITTING_DATA,
        ERROR_GRAPHICS_I2C_NOT_SUPPORTED, ERROR_GRAPHICS_MCA_INTERNAL_ERROR,
        ERROR_GRAPHICS_ONLY_CONSOLE_SESSION_SUPPORTED, ERROR_TIMEOUT,
    };

    fn kind(error: u32) -> &'static str {
        match classify(error, String::new()) {
            LinkError::Transient(_) => "transient",
            LinkError::Unsupported(_) => "unsupported",
            LinkError::Gone(_) => "gone",
        }
    }

    #[test]
    fn display_kinds_follow_the_connector_and_vendor() {
        use windows_sys::Win32::Devices::Display::{
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EXTERNAL, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HDMI,
        };
        assert_eq!(display_kind(DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL, "BOE0A1B"), DisplayKind::BuiltIn);
        assert_eq!(display_kind(DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HDMI, "LEN62A2"), DisplayKind::External);
        assert_eq!(display_kind(DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EXTERNAL, "DEL4123"), DisplayKind::External);
        assert_eq!(display_kind(DISPLAYCONFIG_OUTPUT_TECHNOLOGY_OTHER, ""), DisplayKind::Virtual);
        assert_eq!(display_kind(DISPLAYCONFIG_OUTPUT_TECHNOLOGY_OTHER, "MSH062E"), DisplayKind::Virtual);
        assert_eq!(display_kind(DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INDIRECT_VIRTUAL, "LEN62A2"), DisplayKind::Virtual);
    }

    #[test]
    fn vcp_not_supported_is_final() {
        assert_eq!(ERROR_GRAPHICS_DDCCI_VCP_NOT_SUPPORTED as u32, 0xC026_2584);
        assert_eq!(kind(ERROR_GRAPHICS_DDCCI_VCP_NOT_SUPPORTED as u32), "unsupported");
    }

    #[test]
    fn dead_handles_are_gone() {
        assert_eq!(kind(ERROR_INVALID_HANDLE), "gone");
        assert_eq!(kind(ERROR_GRAPHICS_INVALID_PHYSICAL_MONITOR_HANDLE as u32), "gone");
        assert_eq!(kind(ERROR_GRAPHICS_MONITOR_NO_LONGER_EXISTS as u32), "gone");
    }

    #[test]
    fn bus_and_framing_errors_are_transient() {
        let transient = [
            ERROR_GRAPHICS_I2C_ERROR_TRANSMITTING_DATA,
            ERROR_GRAPHICS_I2C_ERROR_RECEIVING_DATA,
            ERROR_GRAPHICS_I2C_NOT_SUPPORTED,
            ERROR_GRAPHICS_I2C_DEVICE_DOES_NOT_EXIST,
            ERROR_GRAPHICS_DDCCI_INVALID_DATA,
            ERROR_GRAPHICS_DDCCI_INVALID_MESSAGE_COMMAND,
            ERROR_GRAPHICS_DDCCI_INVALID_MESSAGE_LENGTH,
            ERROR_GRAPHICS_DDCCI_INVALID_MESSAGE_CHECKSUM,
            ERROR_GRAPHICS_DDCCI_CURRENT_CURRENT_VALUE_GREATER_THAN_MAXIMUM_VALUE,
            ERROR_GRAPHICS_MCA_INTERNAL_ERROR,
            ERROR_GRAPHICS_ONLY_CONSOLE_SESSION_SUPPORTED,
        ];
        for error in transient {
            assert_eq!(kind(error as u32), "transient", "0x{:08X}", error as u32);
        }
        assert_eq!(kind(ERROR_TIMEOUT), "transient");
        assert_eq!(kind(0), "transient");
    }
}
