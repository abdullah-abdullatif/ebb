use super::Presence;
use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use std::ffi::c_void;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
    fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayIsAsleep(display: u32) -> i32;
}

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPMCopyAssertionsStatus(out: *mut CFDictionaryRef) -> i32;
}

const COMBINED_SESSION_STATE: i32 = 0;
const ANY_INPUT_EVENT: u32 = !0;

pub fn presence() -> Presence {
    Presence { idle_seconds: idle_seconds(), screen_off: screen_off(), media_playing: display_kept_awake() }
}

/// HID idle counter: needs no Accessibility or Input Monitoring permission.
fn idle_seconds() -> f64 {
    unsafe { CGEventSourceSecondsSinceLastEventType(COMBINED_SESSION_STATE, ANY_INPUT_EVENT) }
}

fn screen_off() -> bool {
    if unsafe { CGDisplayIsAsleep(CGMainDisplayID()) } != 0 {
        return true;
    }
    let raw = unsafe { CGSessionCopyCurrentDictionary() };
    if raw.is_null() {
        return false; // no GUI session (e.g. over SSH)
    }
    let dict: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_create_rule(raw) };
    dict.find(&CFString::from_static_string("CGSSessionScreenIsLocked"))
        .and_then(|v| v.downcast::<CFBoolean>())
        .map(bool::from)
        .unwrap_or(false)
}

/// Video players (browsers on YouTube/Netflix, QuickTime, VLC, IINA…) hold a
/// "prevent display sleep" power assertion while playing. Music players don't.
fn display_kept_awake() -> bool {
    let mut raw: CFDictionaryRef = std::ptr::null();
    if unsafe { IOPMCopyAssertionsStatus(&mut raw) } != 0 || raw.is_null() {
        return false;
    }
    let dict: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_create_rule(raw) };
    dict.find(&CFString::from_static_string("PreventUserIdleDisplaySleep"))
        .and_then(|v| v.downcast::<CFNumber>())
        .and_then(|n| n.to_i64())
        .is_some_and(|n| n > 0)
}

// MARK: Camera (CoreMediaIO) and microphone (CoreAudio)
// Both only ask whether *some* process is running the device; Ebb never opens it.

#[repr(C)]
struct PropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

const fn fourcc(s: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*s)
}

const SYSTEM_OBJECT: u32 = 1;
const PROPERTY_DEVICES: u32 = fourcc(b"dev#");
const SCOPE_GLOBAL: u32 = fourcc(b"glob");
const SCOPE_INPUT: u32 = fourcc(b"inpt");
const ELEMENT_MAIN: u32 = 0;
const RUNNING_SOMEWHERE: u32 = fourcc(b"gone");
const STREAMS: u32 = fourcc(b"stm#");

#[link(name = "CoreMediaIO", kind = "framework")]
extern "C" {
    fn CMIOObjectGetPropertyDataSize(id: u32, addr: *const PropertyAddress, qs: u32, q: *const c_void, size: *mut u32) -> i32;
    fn CMIOObjectGetPropertyData(id: u32, addr: *const PropertyAddress, qs: u32, q: *const c_void, size: u32, used: *mut u32, data: *mut c_void) -> i32;
}

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyDataSize(id: u32, addr: *const PropertyAddress, qs: u32, q: *const c_void, size: *mut u32) -> i32;
    fn AudioObjectGetPropertyData(id: u32, addr: *const PropertyAddress, qs: u32, q: *const c_void, size: *mut u32, data: *mut c_void) -> i32;
}

pub fn camera_in_use() -> bool {
    let addr = PropertyAddress { selector: PROPERTY_DEVICES, scope: SCOPE_GLOBAL, element: ELEMENT_MAIN };
    let mut size = 0u32;
    unsafe {
        if CMIOObjectGetPropertyDataSize(SYSTEM_OBJECT, &addr, 0, std::ptr::null(), &mut size) != 0 || size == 0 {
            return false;
        }
        let mut devices = vec![0u32; size as usize / 4];
        let mut used = 0u32;
        if CMIOObjectGetPropertyData(SYSTEM_OBJECT, &addr, 0, std::ptr::null(), size, &mut used, devices.as_mut_ptr().cast()) != 0 {
            return false;
        }
        let run = PropertyAddress { selector: RUNNING_SOMEWHERE, scope: SCOPE_GLOBAL, element: ELEMENT_MAIN };
        devices.into_iter().any(|d| {
            let mut running = 0u32;
            let mut out = 0u32;
            CMIOObjectGetPropertyData(d, &run, 0, std::ptr::null(), 4, &mut out, (&mut running as *mut u32).cast()) == 0 && running != 0
        })
    }
}

pub fn microphone_in_use() -> bool {
    let addr = PropertyAddress { selector: PROPERTY_DEVICES, scope: SCOPE_GLOBAL, element: ELEMENT_MAIN };
    let mut size = 0u32;
    unsafe {
        if AudioObjectGetPropertyDataSize(SYSTEM_OBJECT, &addr, 0, std::ptr::null(), &mut size) != 0 || size == 0 {
            return false;
        }
        let mut devices = vec![0u32; size as usize / 4];
        if AudioObjectGetPropertyData(SYSTEM_OBJECT, &addr, 0, std::ptr::null(), &mut size, devices.as_mut_ptr().cast()) != 0 {
            return false;
        }
        // Only input devices count: speakers run whenever music plays.
        let input_streams = PropertyAddress { selector: STREAMS, scope: SCOPE_INPUT, element: ELEMENT_MAIN };
        let run = PropertyAddress { selector: RUNNING_SOMEWHERE, scope: SCOPE_GLOBAL, element: ELEMENT_MAIN };
        devices.into_iter().any(|d| {
            let mut streams = 0u32;
            let has_input = AudioObjectGetPropertyDataSize(d, &input_streams, 0, std::ptr::null(), &mut streams) == 0 && streams > 0;
            let mut running = 0u32;
            let mut rs = 4u32;
            has_input
                && AudioObjectGetPropertyData(d, &run, 0, std::ptr::null(), &mut rs, (&mut running as *mut u32).cast()) == 0
                && running != 0
        })
    }
}

// MARK: Break screen presentation

use objc2::msg_send;
use objc2::runtime::AnyObject;

/// Puts a break window above everything, on every Space and over full-screen apps.
/// Must run on the main thread. `ns_window` is the NSWindow pointer from Tauri.
pub fn raise_overlay(ns_window: *mut c_void) {
    const SCREEN_SAVER_LEVEL: isize = 1000;
    // canJoinAllSpaces | stationary | ignoresCycle | fullScreenAuxiliary
    const BEHAVIOR: usize = (1 << 0) | (1 << 4) | (1 << 6) | (1 << 8);
    let w = ns_window as *mut AnyObject;
    if w.is_null() {
        return;
    }
    unsafe {
        let _: () = msg_send![w, setLevel: SCREEN_SAVER_LEVEL];
        let _: () = msg_send![w, setCollectionBehavior: BEHAVIOR];
    }
}

/// Strict mode: hide Dock and menu bar, block ⌘-Tab, Force Quit and log-out while a break runs.
/// Must run on the main thread.
pub fn set_strict_presentation(on: bool) {
    // hideDock | hideMenuBar | disableProcessSwitching | disableSessionTermination | disableHideApplication
    let options: usize = if on { (1 << 1) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 8) } else { 0 };
    unsafe {
        let Some(cls) = objc2::runtime::AnyClass::get(c"NSApplication") else { return };
        let app: *mut AnyObject = msg_send![cls, sharedApplication];
        if !app.is_null() {
            let _: () = msg_send![app, setPresentationOptions: options];
        }
    }
}

// MARK: Calendar (EventKit)

use objc2::rc::Retained;
use objc2_event_kit::{EKAuthorizationStatus, EKEntityType, EKEventAvailability, EKEventStatus, EKEventStore, EKParticipantStatus};
use objc2_foundation::NSDate;

/// Reads the calendars already set up on this Mac (iCloud, Google, Exchange…).
pub struct Calendar {
    store: Retained<EKEventStore>,
}

// EKEventStore is documented as safe to use from any thread.
unsafe impl Send for Calendar {}

impl Calendar {
    pub fn new() -> Self {
        Self { store: unsafe { EKEventStore::new() } }
    }

    pub fn authorized() -> bool {
        unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) == EKAuthorizationStatus::FullAccess }
    }

    /// Shows the system permission prompt the first time.
    pub fn request_access(&self) {
        let block = block2::RcBlock::new(|_granted: objc2::runtime::Bool, _err: *mut objc2_foundation::NSError| {});
        unsafe { self.store.requestFullAccessToEventsWithCompletion(&*block as *const _ as *mut _) };
    }

    /// Title of a busy, non-declined event happening right now.
    pub fn current_event(&self) -> Option<String> {
        if !Self::authorized() {
            return None;
        }
        unsafe {
            let start = NSDate::dateWithTimeIntervalSinceNow(-60.0);
            let end = NSDate::dateWithTimeIntervalSinceNow(60.0);
            let now = NSDate::now();
            let predicate = self.store.predicateForEventsWithStartDate_endDate_calendars(&start, &end, None);
            let events = self.store.eventsMatchingPredicate(&predicate);
            for e in events.iter() {
                let declined = e.attendees().is_some_and(|people| {
                    people.iter().any(|p| p.isCurrentUser() && p.participantStatus() == EKParticipantStatus::Declined)
                });
                let happening = e.startDate().timeIntervalSinceDate(&now) <= 0.0 && e.endDate().timeIntervalSinceDate(&now) > 0.0;
                if !e.isAllDay()
                    && e.availability() != EKEventAvailability::Free
                    && e.status() != EKEventStatus::Canceled
                    && happening
                    && !declined
                {
                    return Some(e.title().to_string());
                }
            }
        }
        None
    }
}
