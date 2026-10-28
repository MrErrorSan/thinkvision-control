//! Local time for backup names, log lines and the Day/Night schedule (tvc-core makes no OS calls). Read fresh on
//! every call so a daylight-saving change is picked up.

/// Seconds since the Unix epoch.
pub fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// Seconds east of UTC.
#[cfg(windows)]
pub fn utc_offset() -> i32 {
    #[repr(C)]
    struct TimeZoneInformation {
        bias: i32,
        standard_name: [u16; 32],
        standard_date: [u16; 8],
        standard_bias: i32,
        daylight_name: [u16; 32],
        daylight_date: [u16; 8],
        daylight_bias: i32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetTimeZoneInformation(tzi: *mut TimeZoneInformation) -> u32;
    }
    // SAFETY: GetTimeZoneInformation fills the struct it is given; the layout matches TIME_ZONE_INFORMATION.
    let (mode, tzi) = unsafe {
        let mut tzi: TimeZoneInformation = std::mem::zeroed();
        (GetTimeZoneInformation(&mut tzi), tzi)
    };
    let extra = match mode {
        1 => tzi.standard_bias,
        2 => tzi.daylight_bias,
        _ => 0,
    };
    -(tzi.bias + extra) * 60
}

#[cfg(unix)]
pub fn utc_offset() -> i32 {
    // SAFETY: localtime_r writes one `tm` for the given time.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return 0;
        }
        tm.tm_gmtoff as i32
    }
}

#[cfg(not(any(windows, unix)))]
pub fn utc_offset() -> i32 {
    0
}
