//! Ebb's windows. Every function here must run on the main thread; `apply` takes care of that.

use crate::engine::Effect;
use crate::Shared;
use tauri::{AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const TOAST: &str = "toast";
const EYE: &str = "eye";
const PANEL: &str = "panel";
const SETTINGS: &str = "settings";
const REQUEST: &str = "request";

static PANEL_CLOSED_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Run the engine's effects on the main thread, then push fresh state to every window.
pub fn apply(app: &AppHandle, effects: Vec<Effect>) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        for e in effects {
            run(&handle, e);
        }
        broadcast(&handle);
    });
}

pub fn broadcast(app: &AppHandle) {
    let snapshot = app.state::<Shared>().lock().unwrap().snapshot();
    crate::update_tray(app, &snapshot);
    let _ = app.emit("ebb://state", snapshot);
}

fn run(app: &AppHandle, effect: Effect) {
    match effect {
        Effect::ShowBreak { strict } => show_break(app, strict),
        Effect::HideBreak => hide_break(app),
        Effect::TendBreak { strict } => tend_break(app, strict),
        Effect::ShowEyeBreak => show_card(app, EYE, "eye.html", 440.0, 250.0, Corner::TopCenter),
        Effect::HideEyeBreak => close(app, EYE),
        Effect::ShowToast => show_card(app, TOAST, "toast.html", 380.0, 150.0, Corner::TopRight),
        Effect::HideToast => close(app, TOAST),
        Effect::SetDim(level) => set_dim(app, level),
        Effect::HideRequest => close(app, REQUEST),
        Effect::InstallUpdate => crate::updater::install_and_restart(app.clone()),
        Effect::SetAutostart(on) => {
            use tauri_plugin_autostart::ManagerExt;
            let launcher = app.autolaunch();
            let result = if on { launcher.enable() } else { launcher.disable() };
            if let Err(e) = result {
                crate::updater::log::warn(&format!("launch-at-login change failed: {e}"));
            }
        }
        Effect::RequestCalendar => app.state::<Shared>().lock().unwrap().request_calendar_access(),
    }
}

fn close(app: &AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.destroy();
    }
}

fn monitors(app: &AppHandle) -> (Vec<Monitor>, usize) {
    let all = app.available_monitors().unwrap_or_default();
    let primary = app.primary_monitor().ok().flatten();
    let index = primary
        .and_then(|p| all.iter().position(|m| m.position() == p.position() && m.size() == p.size()))
        .unwrap_or(0);
    (all, index)
}

fn primary_monitor(app: &AppHandle) -> Option<Monitor> {
    let (all, i) = monitors(app);
    all.into_iter().nth(i)
}

/// Make `w` cover monitor `m` exactly.
fn cover(w: &WebviewWindow, m: &Monitor) {
    let _ = w.set_position(PhysicalPosition::new(m.position().x, m.position().y));
    let _ = w.set_size(PhysicalSize::new(m.size().width, m.size().height));
}

/// Above everything, on every desktop/Space, including over full-screen apps.
fn raise(w: &WebviewWindow) {
    let _ = w.set_always_on_top(true);
    let _ = w.set_visible_on_all_workspaces(true);
    raise_above_menu_bar(w);
}

/// macOS: screen-saver level, above the Dock and menu bar. Tauri applies its own window
/// settings on the next turn of the event loop, so this is queued behind them.
fn raise_above_menu_bar(w: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    {
        let w = w.clone();
        let _ = w.clone().run_on_main_thread(move || {
            if let Ok(ns) = w.ns_window() {
                crate::platform::raise_overlay(ns);
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = w;
}

// MARK: Break screen

fn break_labels(app: &AppHandle) -> Vec<String> {
    app.webview_windows().into_keys().filter(|l| l.starts_with("break-")).collect()
}

fn show_break(app: &AppHandle, strict: bool) {
    hide_break(app);
    close(app, PANEL);
    let (all, primary) = monitors(app);
    for (i, m) in all.iter().enumerate() {
        let is_primary = i == primary;
        let url = format!("break.html?primary={}", is_primary as u8);
        let Ok(w) = WebviewWindowBuilder::new(app, format!("break-{i}"), WebviewUrl::App(url.into()))
            .title("Ebb break")
            .decorations(false)
            .resizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .visible(false)
            .build()
        else {
            continue;
        };
        cover(&w, m);
        raise(&w);
        // Linux window managers only let a full-screen window cover panels and docks.
        #[cfg(target_os = "linux")]
        let _ = w.set_fullscreen(true);
        let _ = w.show();
        if is_primary {
            let _ = w.set_focus();
        }
    }
    #[cfg(target_os = "macos")]
    if strict {
        crate::platform::set_strict_presentation(true);
    }
    let _ = strict;
}

fn hide_break(app: &AppHandle) {
    for label in break_labels(app) {
        close(app, &label);
    }
    #[cfg(target_os = "macos")]
    crate::platform::set_strict_presentation(false);
}

fn tend_break(app: &AppHandle, strict: bool) {
    let labels = break_labels(app);
    let (all, _) = monitors(app);
    if labels.len() != all.len() {
        // A display was plugged in or out.
        show_break(app, strict);
        return;
    }
    for l in &labels {
        if let Some(w) = app.get_webview_window(l) {
            raise_above_menu_bar(&w);
        }
    }
    if strict {
        // Strict mode: you can't click or Alt-Tab your way out of the break.
        let focused = labels.iter().any(|l| app.get_webview_window(l).is_some_and(|w| w.is_focused().unwrap_or(false)));
        if !focused {
            let primary = labels.iter().filter_map(|l| app.get_webview_window(l)).find(|w| w.url().is_ok_and(|u| u.query() == Some("primary=1")));
            if let Some(w) = primary {
                let _ = w.set_focus();
            }
        }
    }
}

// MARK: Cards (toast, eye break)

enum Corner {
    TopRight,
    TopCenter,
}

fn show_card(app: &AppHandle, label: &str, page: &str, width: f64, height: f64, corner: Corner) {
    if app.get_webview_window(label).is_some() {
        // Already up: the page re-reads state from the broadcast.
        return;
    }
    let Ok(w) = WebviewWindowBuilder::new(app, label, WebviewUrl::App(page.into()))
        .title("Ebb")
        .inner_size(width, height)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focused(false)
        .visible(false)
        .build()
    else {
        return;
    };
    if let Some(m) = primary_monitor(app) {
        let scale = m.scale_factor();
        let (mx, my) = (m.position().x as f64, m.position().y as f64);
        let mw = m.size().width as f64;
        let (w_px, margin) = (width * scale, 16.0 * scale);
        // Leave room for the macOS menu bar.
        let top = my + if cfg!(target_os = "macos") { 40.0 * scale } else { margin };
        let (x, y) = match corner {
            Corner::TopRight => (mx + mw - w_px - margin, top),
            // Below where toasts appear, so a nudge and an eye break never overlap.
            Corner::TopCenter => (mx + (mw - w_px) / 2.0, top + 170.0 * scale),
        };
        let _ = w.set_position(PhysicalPosition::new(x as i32, y as i32));
    }
    raise(&w);
    let _ = w.show();
}

// MARK: Night dimming

fn set_dim(app: &AppHandle, level: Option<f64>) {
    let existing: Vec<String> = app.webview_windows().into_keys().filter(|l| l.starts_with("dim-")).collect();
    let Some(level) = level else {
        for l in existing {
            close(app, &l);
        }
        return;
    };
    if !existing.is_empty() {
        let _ = app.emit("ebb://dim", level);
        return;
    }
    let (all, _) = monitors(app);
    for (i, m) in all.iter().enumerate() {
        let Ok(w) = WebviewWindowBuilder::new(app, format!("dim-{i}"), WebviewUrl::App(format!("dim.html?level={level}").into()))
            .title("Ebb wind-down")
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .focused(false)
            .visible(false)
            .build()
        else {
            continue;
        };
        cover(&w, m);
        let _ = w.set_ignore_cursor_events(true);
        let _ = w.set_visible_on_all_workspaces(true);
        let _ = w.show();
    }
}

// MARK: Panel, settings, request

/// The popover from the menu bar / system tray. `anchor` is the tray icon's rect, if known.
pub fn toggle_panel(app: &AppHandle, anchor: Option<(PhysicalPosition<f64>, PhysicalSize<f64>)>) {
    if let Some(w) = app.get_webview_window(PANEL) {
        let _ = w.destroy();
        return;
    }
    // Clicking the tray icon while the panel is open first closes it (focus moves away);
    // don't let the same click open it again.
    if PANEL_CLOSED_AT.lock().unwrap().is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(300)) {
        return;
    }
    let (width, height) = (340.0, 560.0);
    let Ok(w) = WebviewWindowBuilder::new(app, PANEL, WebviewUrl::App("panel.html".into()))
        .title("Ebb")
        .inner_size(width, height)
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .build()
    else {
        return;
    };
    match (anchor, primary_monitor(app)) {
        (Some((pos, size)), Some(m)) => {
            let scale = m.scale_factor();
            let (pw, ph) = (width * scale, height * scale);
            let x = pos.x + size.width / 2.0 - pw / 2.0;
            let screen_bottom = (m.position().y as f64) + m.size().height as f64;
            // Tray at the top (macOS, some Linux): open below it. At the bottom (Windows): above it.
            let y = if pos.y < screen_bottom / 2.0 { pos.y + size.height + 4.0 } else { pos.y - ph - 4.0 };
            let max_x = m.position().x as f64 + m.size().width as f64 - pw - 8.0;
            let _ = w.set_position(PhysicalPosition::new(x.min(max_x).max(m.position().x as f64) as i32, y as i32));
        }
        _ => {
            let _ = w.center();
        }
    }
    let _ = w.show();
    let _ = w.set_focus();
    let handle = app.clone();
    w.on_window_event(move |e| {
        if let tauri::WindowEvent::Focused(false) = e {
            // Clicking anywhere else closes it, like a menu.
            if let Some(w) = handle.get_webview_window(PANEL) {
                *PANEL_CLOSED_AT.lock().unwrap() = Some(std::time::Instant::now());
                let _ = w.destroy();
            }
        }
    });
}

pub fn open_settings(app: &AppHandle, tab: Option<&str>) {
    close(app, PANEL);
    if let Some(w) = app.get_webview_window(SETTINGS) {
        if let Some(tab) = tab {
            let _ = w.emit("ebb://tab", tab);
        }
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let url = format!("settings.html#{}", tab.unwrap_or("today"));
    if let Ok(w) = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App(url.into()))
        .title("Ebb Settings")
        .inner_size(820.0, 640.0)
        .min_inner_size(680.0, 520.0)
        .center()
        .build()
    {
        let _ = w.set_focus();
    }
}

pub fn open_request(app: &AppHandle) {
    close(app, PANEL);
    if !break_labels(app).is_empty() {
        return; // the break screen has its own form
    }
    if let Some(w) = app.get_webview_window(REQUEST) {
        let _ = w.set_focus();
        return;
    }
    if let Ok(w) = WebviewWindowBuilder::new(app, REQUEST, WebviewUrl::App("request.html".into()))
        .title("Ask for more time")
        .inner_size(460.0, 540.0)
        .resizable(false)
        .always_on_top(true)
        .center()
        .build()
    {
        let _ = w.set_focus();
    }
}

pub fn close_request(app: &AppHandle) {
    close(app, REQUEST);
}
