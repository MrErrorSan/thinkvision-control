//! DDC/CI over dxva2. Finds the target by EDID id in the display device ID, falling back to the dxva2 description.
//! dxva2 has no permission or driver failure modes worth surfacing: the monitor is either there or not.

use std::mem::size_of;
use std::ptr::{null, null_mut};

use tvc_core::link::{LinkError, LinkFailure, MonitorLink, MonitorMatch, VcpReading};
use windows_sys::core::BOOL;
use windows_sys::Win32::Devices::Display::{
    CapabilitiesRequestAndCapabilitiesReply, DestroyPhysicalMonitors, GetCapabilitiesStringLength,
    GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR, GetVCPFeatureAndVCPFeatureReply,
    SetVCPFeature, PHYSICAL_MONITOR,
};
use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_GRAPHICS_INVALID_PHYSICAL_MONITOR_HANDLE, ERROR_GRAPHICS_MONITOR_NO_LONGER_EXISTS,
    ERROR_INVALID_HANDLE, HANDLE, LPARAM, RECT,
};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, DISPLAY_DEVICEW, HDC, HMONITOR, MONITORINFOEXW,
};

use crate::{ascii, not_connected};

/// `EDD_GET_DEVICE_INTERFACE_NAME`: report the monitor's device interface path (which carries the EDID id) as DeviceID.
const EDD_GET_DEVICE_INTERFACE_NAME: u32 = 1;

/// The dxva2 link: one physical-monitor handle while connected.
pub(crate) struct Dxva2Link {
    target: MonitorMatch,
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
    pub(crate) fn new(target: MonitorMatch) -> Self {
        Dxva2Link { target, held: None }
    }

    fn handle(&self) -> Result<HANDLE, LinkError> {
        self.held.as_ref().map(|h| h.0).ok_or_else(not_connected)
    }
}

impl MonitorLink for Dxva2Link {
    fn connect(&mut self) -> Result<(), LinkFailure> {
        self.held = None;

        let edid_id = self.target.edid_id.to_ascii_uppercase();
        let mut all: Vec<PHYSICAL_MONITOR> = Vec::new();
        let mut target = None;
        for monitor in enumerate_monitors() {
            let physical = physical_monitors(monitor);
            if target.is_none() && !edid_id.is_empty() {
                let ids = device_ids(monitor);
                target = ids
                    .iter()
                    .take(physical.len())
                    .position(|id| id.to_ascii_uppercase().contains(&edid_id))
                    .map(|i| all.len() + i);
            }
            all.extend(physical);
        }

        if target.is_none() {
            if let Some(hint) = self.target.description_hint.as_deref().map(str::to_lowercase) {
                target = all.iter().position(|m| {
                    let description = { m.szPhysicalMonitorDescription }; // copy out of the packed struct
                    wide_to_string(&description).to_lowercase().contains(&hint)
                });
            }
        }

        if let Some(i) = target {
            self.held = Some(Handle(all.remove(i).hPhysicalMonitor));
        }
        destroy(&all);

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

/// The error for a failed dxva2 call, with GetLastError. A dead handle is `Gone`; anything else is worth retrying.
fn failure(call: &str, code: Option<u8>) -> LinkError {
    // SAFETY: no preconditions.
    let error = unsafe { GetLastError() };
    let code = code.map(|c| format!(" 0x{c:02X}")).unwrap_or_default();
    let message = format!("{call}{code} failed (Win32 error 0x{error:08X})");
    let gone = [
        ERROR_INVALID_HANDLE,
        ERROR_GRAPHICS_INVALID_PHYSICAL_MONITOR_HANDLE as u32,
        ERROR_GRAPHICS_MONITOR_NO_LONGER_EXISTS as u32,
    ];
    if gone.contains(&error) {
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

/// The device interface paths of the monitors on this HMONITOR's adapter output, e.g.
/// `\\?\DISPLAY#LEN62A2#5&...#{e6f07b5f-...}`, in the same order as its physical monitors.
fn device_ids(monitor: HMONITOR) -> Vec<String> {
    let mut ids = Vec::new();
    // SAFETY: zeroed POD structs with their size fields set; MONITORINFOEXW starts with MONITORINFO.
    unsafe {
        let mut info: MONITORINFOEXW = std::mem::zeroed();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(monitor, &mut info as *mut MONITORINFOEXW as *mut _) == 0 {
            return ids;
        }
        let mut device: DISPLAY_DEVICEW = std::mem::zeroed();
        device.cb = size_of::<DISPLAY_DEVICEW>() as u32;
        let mut index = 0;
        while EnumDisplayDevicesW(info.szDevice.as_ptr(), index, &mut device, EDD_GET_DEVICE_INTERFACE_NAME) != 0 {
            ids.push(wide_to_string(&device.DeviceID));
            index += 1;
        }
    }
    ids
}

fn wide_to_string(wide: &[u16]) -> String {
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}
