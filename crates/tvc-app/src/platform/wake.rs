//! Wake the monitor before an input switch: a monitor in standby may ignore DDC/CI until the PC
//! signals activity.

/// Windows: a 1 px relative mouse move and back (`SendInput`). macOS: `caffeinate -u -t 1` (declares user
/// activity), not waited for. Linux: nothing.
pub fn nudge() {
    imp::nudge();
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MOVE, MOUSEINPUT,
    };

    fn relative_move(dx: i32) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT { dx, dy: 0, mouseData: 0, dwFlags: MOUSEEVENTF_MOVE, time: 0, dwExtraInfo: 0 },
            },
        }
    }

    pub fn nudge() {
        let inputs = [relative_move(1), relative_move(-1)];
        // SAFETY: a valid array of two INPUTs and the size of one element.
        unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::process::{Command, Stdio};

    pub fn nudge() {
        let child = Command::new("caffeinate")
            .args(["-u", "-t", "1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(mut child) = child {
            // Reap it off the caller's thread so no zombie is left.
            let _ = std::thread::Builder::new().name("tvc-caffeinate".into()).spawn(move || child.wait());
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    pub fn nudge() {}
}
