//! The `\\.\LCD` device's brightness IOCTLs (ntddvdeo.h), chosen over WMI `WmiMonitorBrightness` because
//! windows-sys has no COM interface wrappers. Assumption, to verify on a laptop: the panel answers these IOCTLs
//! for an unelevated user.
//!
//! The codes are `CTL_CODE(FILE_DEVICE_VIDEO = 0x23, function, METHOD_BUFFERED = 0, FILE_ANY_ACCESS = 0)`, i.e.
//! `0x23 << 16 | function << 2`; windows-sys carries them precomputed:
//! - `IOCTL_VIDEO_QUERY_SUPPORTED_BRIGHTNESS`, function 0x125 = 0x230494: the supported levels, one percent per byte;
//! - `IOCTL_VIDEO_QUERY_DISPLAY_BRIGHTNESS`, function 0x126 = 0x230498: a `DISPLAY_BRIGHTNESS`;
//! - `IOCTL_VIDEO_SET_DISPLAY_BRIGHTNESS`, function 0x127 = 0x23049C: a `DISPLAY_BRIGHTNESS`.

use std::cmp::Ordering;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{null, null_mut};

use tvc_core::identity::DisplayInfo;
use tvc_core::link::{LinkError, LinkFailure, MonitorLink};
use windows_sys::Win32::Devices::Display::{
    DISPLAYPOLICY_AC, DISPLAYPOLICY_DC, DISPLAY_BRIGHTNESS, IOCTL_VIDEO_QUERY_DISPLAY_BRIGHTNESS,
    IOCTL_VIDEO_QUERY_SUPPORTED_BRIGHTNESS, IOCTL_VIDEO_SET_DISPLAY_BRIGHTNESS,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ACCESS_DENIED, ERROR_INVALID_HANDLE, ERROR_NOT_SUPPORTED, GENERIC_READ,
    GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
use windows_sys::Win32::System::IO::DeviceIoControl;

use super::link::{BacklightLink, Panel};
use super::{display_info, nearest_level};

/// The only key: there is one `\\.\LCD` device.
const KEY: &str = "lcd";

pub(super) fn displays() -> Vec<DisplayInfo> {
    match Lcd::open() {
        Ok(_) => vec![display_info(KEY, &[], "internal")],
        Err(_) => Vec::new(),
    }
}

pub(super) fn open(key: &str) -> Result<Box<dyn MonitorLink>, LinkFailure> {
    if key != KEY {
        return Err(LinkFailure::NotFound);
    }
    Ok(Box::new(BacklightLink::new(LcdPanel { held: None })))
}

struct LcdPanel {
    held: Option<Lcd>,
}

impl LcdPanel {
    fn lcd(&self) -> Result<&Lcd, LinkError> {
        self.held.as_ref().ok_or_else(crate::not_connected)
    }
}

impl Panel for LcdPanel {
    fn open(&mut self) -> Result<(), LinkFailure> {
        self.held = Some(Lcd::open()?);
        Ok(())
    }

    fn close(&mut self) {
        self.held = None;
    }

    fn get(&mut self) -> Result<u16, LinkError> {
        let mut b = DISPLAY_BRIGHTNESS::default();
        self.lcd()?.ioctl(IOCTL_VIDEO_QUERY_DISPLAY_BRIGHTNESS, null(), 0, (&mut b as *mut DISPLAY_BRIGHTNESS).cast(),
            size_of::<DISPLAY_BRIGHTNESS>() as u32)?;
        // Assumption: on a query, the policy names the power source in effect.
        let level = if u32::from(b.ucDisplayPolicy) == DISPLAYPOLICY_DC { b.ucDCBrightness } else { b.ucACBrightness };
        Ok(level.into())
    }

    fn set(&mut self, percent: u16, toward: Ordering) -> Result<u16, LinkError> {
        let lcd = self.lcd()?;
        let level = nearest_level(&lcd.levels, percent, toward).unwrap_or_default();
        let b = DISPLAY_BRIGHTNESS {
            ucDisplayPolicy: (DISPLAYPOLICY_AC | DISPLAYPOLICY_DC) as u8,
            ucACBrightness: level,
            ucDCBrightness: level,
        };
        lcd.ioctl(IOCTL_VIDEO_SET_DISPLAY_BRIGHTNESS, (&b as *const DISPLAY_BRIGHTNESS).cast(),
            size_of::<DISPLAY_BRIGHTNESS>() as u32, null_mut(), 0)?;
        Ok(level.into())
    }
}

/// An open `\\.\LCD` handle with its supported levels (never empty), closed on drop.
struct Lcd {
    handle: HANDLE,
    levels: Vec<u8>,
}

// SAFETY: a file handle may be used from any thread; the engine uses it from one.
unsafe impl Send for Lcd {}

impl Drop for Lcd {
    fn drop(&mut self) {
        // SAFETY: a handle we own and no longer use.
        unsafe { CloseHandle(self.handle) };
    }
}

impl Lcd {
    /// Opens the device; present only when it lists at least one supported level.
    fn open() -> Result<Self, LinkFailure> {
        let path: Vec<u16> = r"\\.\LCD".encode_utf16().chain([0]).collect();
        // SAFETY: a NUL-terminated path; no security attributes or template.
        let handle = unsafe {
            CreateFileW(path.as_ptr(), GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, null(),
                OPEN_EXISTING, 0, null_mut())
        };
        if handle == INVALID_HANDLE_VALUE {
            // SAFETY: no preconditions.
            let error = unsafe { GetLastError() };
            return Err(if error == ERROR_ACCESS_DENIED { LinkFailure::PermissionDenied } else { LinkFailure::NotFound });
        }
        let mut lcd = Lcd { handle, levels: Vec::new() };
        let mut buffer = [0u8; 256];
        let returned = lcd
            .ioctl(IOCTL_VIDEO_QUERY_SUPPORTED_BRIGHTNESS, null(), 0, buffer.as_mut_ptr().cast(), buffer.len() as u32)
            .map_err(|_| LinkFailure::NotFound)?;
        lcd.levels = buffer[..returned.min(buffer.len())].to_vec();
        if lcd.levels.is_empty() {
            return Err(LinkFailure::NotFound);
        }
        Ok(lcd)
    }

    /// One buffered IOCTL; the number of bytes returned.
    fn ioctl(&self, code: u32, input: *const c_void, input_len: u32, output: *mut c_void, output_len: u32)
        -> Result<usize, LinkError> {
        let mut returned = 0u32;
        // SAFETY: a live handle; the buffers are valid for the lengths given, and the call is synchronous.
        if unsafe { DeviceIoControl(self.handle, code, input, input_len, output, output_len, &mut returned, null_mut()) }
            == 0
        {
            // SAFETY: no preconditions.
            let error = unsafe { GetLastError() };
            let message = format!("LCD IOCTL 0x{code:06X} failed (error {error})");
            return Err(match error {
                ERROR_INVALID_HANDLE => LinkError::Gone(message),
                ERROR_NOT_SUPPORTED => LinkError::Unsupported(message),
                _ => LinkError::Transient(message),
            });
        }
        Ok(returned as usize)
    }
}
