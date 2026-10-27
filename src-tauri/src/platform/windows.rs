use super::Presence;
use windows_sys::Win32::System::Power::{CallNtPowerInformation, SystemExecutionState};
use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, OpenInputDesktop, DESKTOP_SWITCHDESKTOP};
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETSCREENSAVERRUNNING};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

pub fn presence() -> Presence {
    Presence { idle_seconds: idle_seconds(), screen_off: screen_off(), media_playing: display_required() }
}

fn idle_seconds() -> f64 {
    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    unsafe {
        if GetLastInputInfo(&mut info) == 0 {
            return 0.0;
        }
        // Both are 32-bit millisecond counters that wrap after ~49 days; wrapping_sub handles it.
        GetTickCount().wrapping_sub(info.dwTime) as f64 / 1000.0
    }
}

fn screen_off() -> bool {
    unsafe {
        // While locked, the input desktop is the secure Winlogon desktop and can't be opened.
        let desk = OpenInputDesktop(0, 0, DESKTOP_SWITCHDESKTOP);
        if desk.is_null() {
            return true;
        }
        CloseDesktop(desk);

        let mut running: i32 = 0;
        SystemParametersInfoW(SPI_GETSCREENSAVERRUNNING, 0, (&mut running as *mut i32).cast(), 0);
        running != 0
    }
}

/// Video players and browsers playing video ask Windows to keep the display on
/// (ES_DISPLAY_REQUIRED). The system-wide execution state reflects those requests.
fn display_required() -> bool {
    const ES_DISPLAY_REQUIRED: u32 = 0x0000_0002;
    let mut state: u32 = 0;
    let status = unsafe {
        CallNtPowerInformation(SystemExecutionState, std::ptr::null(), 0, (&mut state as *mut u32).cast(), 4)
    };
    status == 0 && state & ES_DISPLAY_REQUIRED != 0
}

/// Windows records which apps use the camera/mic (the privacy indicator in the taskbar).
/// An entry with a start time and no stop time is in use right now.
fn capability_in_use(capability: &str) -> bool {
    let path = format!(r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\{capability}");
    let Ok(root) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(path) else { return false };
    let in_use = |key: &RegKey| {
        let stop: u64 = key.get_value("LastUsedTimeStop").unwrap_or(1);
        let start: u64 = key.get_value("LastUsedTimeStart").unwrap_or(0);
        start > 0 && stop == 0
    };
    let any_child = |key: &RegKey| key.enum_keys().flatten().any(|name| key.open_subkey(&name).is_ok_and(|k| in_use(&k)));
    // Store apps are direct children; desktop apps (Zoom, Chrome…) live under NonPackaged.
    any_child(&root) || root.open_subkey("NonPackaged").is_ok_and(|np| any_child(&np))
}

pub fn camera_in_use() -> bool {
    capability_in_use("webcam")
}

pub fn microphone_in_use() -> bool {
    capability_in_use("microphone")
}
