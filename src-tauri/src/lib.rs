mod clock;
mod engine;
mod platform;
mod prefs;
mod stats;
mod ui;
mod updater;
mod wellness;

use chrono::Local;
use engine::{Core, Effect, Snapshot};
use prefs::Preferences;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, State};

pub type Shared = Arc<Mutex<Core>>;

/// `EBB_SMOKE_TEST=1`: used by CI to run the real app unattended. A "minute" lasts one
/// second, input is treated as constant, and every event is printed as `EBB_EVENT …`.
fn is_smoke_test() -> bool {
    std::env::var("EBB_SMOKE_TEST").as_deref() == Ok("1")
}

const TRAY: &str = "main";

// MARK: Commands (called from the windows)

#[tauri::command]
fn get_state(state: State<Shared>) -> Snapshot {
    state.lock().unwrap().snapshot()
}

/// Lock the core, change something, then run the resulting effects outside the lock.
fn act(app: &AppHandle, f: impl FnOnce(&mut Core) -> Vec<Effect>) {
    let fx = {
        let shared = app.state::<Shared>();
        let mut core = shared.lock().unwrap();
        let events = f(&mut core);
        events
    };
    ui::apply(app, fx);
}

#[tauri::command]
fn start_break_now(app: AppHandle) {
    act(&app, |c| {
        let events = c.clock.start_break_now();
        c.handle(events)
    });
}

#[tauri::command]
fn emergency_skip(app: AppHandle) {
    act(&app, |c| {
        if c.prefs().strict_mode {
            return vec![];
        }
        let events = c.clock.skip_break();
        c.handle(events)
    });
}

#[tauri::command]
fn pause(app: AppHandle, minutes: Option<i64>) {
    act(&app, |c| {
        c.clock.pause(minutes.map(|m| Local::now() + chrono::Duration::minutes(m)));
        vec![Effect::HideBreak, Effect::HideEyeBreak]
    });
}

#[tauri::command]
fn resume(app: AppHandle) {
    act(&app, |c| {
        let events = c.clock.resume();
        c.handle(events)
    });
}

#[tauri::command]
fn set_manual_meeting(app: AppHandle, on: bool) {
    act(&app, |c| {
        c.meeting.manual = on;
        vec![]
    });
}

#[tauri::command]
fn open_request(app: AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || ui::open_request(&handle));
}

#[tauri::command]
fn close_request(app: AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || ui::close_request(&handle));
}

#[tauri::command]
fn request_exception(app: AppHandle, minutes: i64, category: String, reason: String) -> Result<(), String> {
    let shared = app.state::<Shared>();
    let mut c = shared.lock().unwrap();
    if c.prefs().require_exception_reason && reason.trim().chars().count() < 3 {
        return Err("Say in a few words what you're finishing.".into());
    }
    match c.clock.grant_exception(minutes) {
        Ok(events) => {
            c.last_denial = None;
            c.stats.update(|d| {
                d.exceptions.push(stats::ExceptionRecord {
                    id: uuid::Uuid::new_v4(),
                    date: chrono::Utc::now(),
                    minutes,
                    reason: reason.trim().to_string(),
                    category,
                })
            });
            c.stats.save_if_needed();
            let mut fx = c.handle(events);
            fx.push(Effect::HideRequest);
            drop(c);
            ui::apply(&app, fx);
            Ok(())
        }
        Err(denial) => {
            c.last_denial = Some(denial.to_string());
            Err(denial.to_string())
        }
    }
}

#[tauri::command]
fn set_prefs(app: AppHandle, prefs: Preferences) {
    act(&app, |c| c.set_prefs(prefs));
}

#[tauri::command]
fn reset_prefs(app: AppHandle) {
    act(&app, |c| {
        let mut defaults = Preferences::default();
        defaults.weekly_report_shown = c.prefs().weekly_report_shown.clone();
        c.set_prefs(defaults)
    });
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryDay {
    stats: stats::DayStats,
    wellbeing: stats::Wellbeing,
}

#[tauri::command]
fn get_history(state: State<Shared>, limit: usize) -> Vec<HistoryDay> {
    let c = state.lock().unwrap();
    c.stats
        .history(limit)
        .into_iter()
        .map(|d| HistoryDay { wellbeing: d.wellness(c.prefs()), stats: d })
        .collect()
}

/// `weeks_ago`: 0 = the 7 days ending today, 1 = the week before, …
#[tauri::command]
fn get_week_report(state: State<Shared>, weeks_ago: i64) -> stats::WeekReport {
    let c = state.lock().unwrap();
    let end = Local::now().date_naive() - chrono::Duration::days(7 * weeks_ago.max(0));
    c.stats.week_report(c.prefs(), end)
}

#[tauri::command]
fn record_mood(app: AppHandle, mood: u8) {
    act(&app, |c| {
        c.stats.update(|d| d.moods.push(stats::MoodRecord { date: chrono::Utc::now(), mood: mood.clamp(1, 5) }));
        c.stats.save_if_needed();
        vec![]
    });
}

#[tauri::command]
fn skip_eye_break(app: AppHandle) {
    act(&app, |c| {
        let events = c.wellness.skip_eye_break();
        c.stats.update(|d| d.eye_breaks_skipped += events.len() as i64);
        vec![Effect::HideEyeBreak]
    });
}

#[tauri::command]
fn toast_action(app: AppHandle, id: u64, action: String) {
    let fx = app.state::<Shared>().lock().unwrap().dismiss_toast(id);
    ui::apply(&app, fx);
    match action.as_str() {
        "breakNow" => start_break_now(app),
        "moreTime" => open_request(app),
        "openReport" => open_settings(app, Some("report".into())),
        "restartUpdate" | "installUpdate" => updater::install_and_restart(app),
        _ => {}
    }
}

#[tauri::command]
fn dismiss_toast(app: AppHandle, id: u64) {
    act(&app, |c| c.dismiss_toast(id));
}

#[tauri::command]
fn open_settings(app: AppHandle, tab: Option<String>) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || ui::open_settings(&handle, tab.as_deref()));
}

#[tauri::command]
fn check_updates(app: AppHandle) {
    updater::check(app, true);
}

#[tauri::command]
fn install_update(app: AppHandle) {
    updater::install_and_restart(app);
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.state::<Shared>().lock().unwrap().stats.save_if_needed();
    app.exit(0);
}

// MARK: Tray

/// Three waves, drawn as a monochrome "template" image so macOS tints it for light/dark menu bars.
fn waves_icon(size: u32) -> Image<'static> {
    let s = size as f64;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let thickness = s * 0.085;
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
            let mut alpha: f64 = 0.0;
            for row in 0..3 {
                let base = s * (0.3 + 0.2 * row as f64);
                let wave = base + s * 0.06 * (fx / s * std::f64::consts::TAU * 1.5).sin();
                let inset = s * 0.1;
                if fx < inset || fx > s - inset {
                    continue;
                }
                alpha = alpha.max((thickness / 2.0 + 0.5 - (fy - wave).abs()).clamp(0.0, 1.0));
            }
            let i = ((y * size + x) * 4) as usize;
            rgba[i + 3] = (alpha * 255.0) as u8;
        }
    }
    Image::new_owned(rgba, size, size)
}

fn tray_title(s: &Snapshot) -> Option<String> {
    use clock::Phase::*;
    if !s.prefs.show_timer_in_menu_bar {
        return None;
    }
    let f = engine::format_duration;
    Some(match &s.phase {
        Working | Warning | Idle => f(s.seconds_until_break),
        OnBreak { remaining } => format!("break {}", f(*remaining)),
        PostMeetingGrace { remaining } => f(*remaining),
        MeetingHold => "meeting".into(),
        Away => String::new(),
        Paused { .. } => "paused".into(),
    })
}

pub fn update_tray(app: &AppHandle, s: &Snapshot) {
    let Some(tray) = app.tray_by_id(TRAY) else { return };
    let title = tray_title(s);
    #[cfg(target_os = "macos")]
    let _ = tray.set_title(title.as_deref().map(|t| format!(" {t}")));
    #[cfg(not(target_os = "macos"))]
    let _ = tray.set_title(title.as_deref());
    let tooltip = match &s.phase {
        clock::Phase::Paused { .. } => "Ebb is paused".to_string(),
        clock::Phase::OnBreak { .. } => "Ebb: on a break".to_string(),
        _ => format!("Ebb: break in {}", engine::format_duration(s.seconds_until_break)),
    };
    let _ = tray.set_tooltip(Some(tooltip));
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);
    let pause_menu = Submenu::with_items(
        app,
        "Pause Ebb",
        true,
        &[&item("pause-30", "30 minutes")?, &item("pause-60", "1 hour")?, &item("pause-120", "2 hours")?, &item("pause-forever", "Until I resume")?],
    )?;
    let meeting = CheckMenuItem::with_id(app, "meeting", "I'm in a meeting", true, false, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &item("open", "Open Ebb")?,
            &PredefinedMenuItem::separator(app)?,
            &item("break", "Take a break now")?,
            &item("more", "Ask for more time…")?,
            &meeting,
            &pause_menu,
            &item("resume", "Resume")?,
            &PredefinedMenuItem::separator(app)?,
            &item("report", "Weekly report…")?,
            &item("settings", "Settings…")?,
            &item("updates", "Check for updates…")?,
            &PredefinedMenuItem::separator(app)?,
            &item("quit", "Quit Ebb")?,
        ],
    )?;

    let icon = if cfg!(target_os = "macos") {
        waves_icon(36)
    } else {
        app.default_window_icon().cloned().unwrap_or_else(|| waves_icon(32))
    };
    TrayIconBuilder::with_id(TRAY)
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Ebb")
        .on_menu_event(move |app, event| {
            let app = app.clone();
            match event.id().as_ref() {
                "open" => {
                    let h = app.clone();
                    let _ = app.run_on_main_thread(move || ui::toggle_panel(&h, None));
                }
                "break" => start_break_now(app),
                "more" => open_request(app),
                "meeting" => {
                    let on = !app.state::<Shared>().lock().unwrap().meeting.manual;
                    set_manual_meeting(app, on);
                }
                "pause-30" => pause(app, Some(30)),
                "pause-60" => pause(app, Some(60)),
                "pause-120" => pause(app, Some(120)),
                "pause-forever" => pause(app, None),
                "resume" => resume(app),
                "report" => open_settings(app, Some("report".into())),
                "settings" => open_settings(app, None),
                "updates" => check_updates(app),
                "quit" => quit(app),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = event {
                let app = tray.app_handle().clone();
                let scale = app.primary_monitor().ok().flatten().map(|m| m.scale_factor()).unwrap_or(1.0);
                let anchor = (rect.position.to_physical::<f64>(scale), rect.size.to_physical::<f64>(scale));
                let h = app.clone();
                let _ = app.run_on_main_thread(move || ui::toggle_panel(&h, Some(anchor)));
            }
        })
        .build(app)?;

    // Keep the menu's meeting checkbox in step with the panel's.
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        let manual = handle.state::<Shared>().lock().unwrap().meeting.manual;
        if meeting.is_checked().ok() != Some(manual) {
            let _ = meeting.set_checked(manual);
        }
    });
    Ok(())
}

// MARK: Run

pub fn run() {
    let smoke = is_smoke_test();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Opening Ebb again shows the panel instead of starting a second copy.
            let h = app.clone();
            let _ = app.run_on_main_thread(move || ui::toggle_panel(&h, None));
        }))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_state,
            start_break_now,
            emergency_skip,
            pause,
            resume,
            set_manual_meeting,
            open_request,
            close_request,
            request_exception,
            set_prefs,
            reset_prefs,
            get_history,
            get_week_report,
            record_mood,
            skip_eye_break,
            toast_action,
            dismiss_toast,
            open_settings,
            check_updates,
            install_update,
            quit,
        ])
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let data_dir: std::path::PathBuf = match std::env::var_os("EBB_DATA_DIR") {
                Some(dir) => dir.into(),
                None => app.path().app_data_dir()?,
            };
            std::fs::create_dir_all(&data_dir)?;
            // The original macOS app kept history in ~/Library/Application Support/Ebb.
            let legacy = app.path().data_dir().ok().map(|d| d.join("Ebb").join("stats.json")).filter(|_| cfg!(target_os = "macos") && !smoke);
            let version = app.package_info().version.to_string();
            let core = Core::new(&data_dir, legacy.as_deref(), smoke, version);
            let shared: Shared = Arc::new(Mutex::new(core));
            app.manage(shared.clone());

            build_tray(app.handle())?;
            let startup = shared.lock().unwrap().startup_effects();
            ui::apply(app.handle(), startup);
            if smoke {
                open_settings(app.handle().clone(), None);
            }

            // The heartbeat: one tick per second.
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_millis(1000));
                let fx = shared.lock().unwrap().tick(Local::now());
                ui::apply(&handle, fx);
            });

            // Updates: shortly after launch, then every few hours.
            if !smoke && !cfg!(debug_assertions) {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(30));
                    loop {
                        updater::check(handle.clone(), false);
                        std::thread::sleep(Duration::from_secs(updater::CHECK_EVERY_HOURS * 3600));
                    }
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Ebb");

    app.run(|app, event| match event {
        // Ebb lives in the tray: closing its last window must not quit it.
        RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
        RunEvent::Exit => {
            if let Some(shared) = app.try_state::<Shared>() {
                shared.lock().unwrap().stats.save_if_needed();
            }
        }
        _ => {}
    });
}
