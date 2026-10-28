//! Windows "Efficiency mode": EcoQoS and low memory priority while no window is visible.

/// Call whenever a window shows or the last one hides. Hidden: EcoQoS + memory priority 2 (low);
/// visible: HighQoS + memory priority 5 (normal). Repeated calls with the same value do nothing. No-op off Windows.
pub fn set_window_visible(visible: bool) {
    #[cfg(windows)]
    win::set_efficient(!visible);
    #[cfg(not(windows))]
    let _ = visible;
}

#[cfg(windows)]
mod win {
    use std::sync::Mutex;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, ProcessMemoryPriority, ProcessPowerThrottling, SetProcessInformation,
        MEMORY_PRIORITY_INFORMATION, MEMORY_PRIORITY_LOW, MEMORY_PRIORITY_NORMAL,
        PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        PROCESS_POWER_THROTTLING_STATE,
    };

    static CURRENT: Mutex<Option<bool>> = Mutex::new(None);

    pub fn set_efficient(efficient: bool) {
        let mut current = CURRENT.lock().unwrap_or_else(|e| e.into_inner());
        if *current == Some(efficient) {
            return;
        }
        *current = Some(efficient);
        // ControlMask says we manage EXECUTION_SPEED; StateMask 0 = HighQoS (not system-managed).
        let throttling = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: if efficient { PROCESS_POWER_THROTTLING_EXECUTION_SPEED } else { 0 },
        };
        let memory = MEMORY_PRIORITY_INFORMATION {
            MemoryPriority: if efficient { MEMORY_PRIORITY_LOW } else { MEMORY_PRIORITY_NORMAL },
        };
        // SAFETY: the pseudo-handle is always valid and each struct matches its information class and size.
        // Failures are ignored, as in 0.0.4 (older Windows builds lack power throttling).
        unsafe {
            let process = GetCurrentProcess();
            SetProcessInformation(
                process,
                ProcessPowerThrottling,
                (&throttling as *const PROCESS_POWER_THROTTLING_STATE).cast(),
                size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            );
            SetProcessInformation(
                process,
                ProcessMemoryPriority,
                (&memory as *const MEMORY_PRIORITY_INFORMATION).cast(),
                size_of::<MEMORY_PRIORITY_INFORMATION>() as u32,
            );
        }
    }
}
