//! Auto-update from GitHub Releases. Each release carries a signed `latest.json`
//! (made by CI); the app checks it at launch and every few hours.

use crate::{ui, Shared};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UpdateStatus {
    Idle,
    Checking,
    UpToDate,
    Available { version: String },
    Downloading { version: String, percent: u8 },
    /// Downloaded; installs on restart (now, or by itself next time you step away).
    Ready { version: String },
    Error { message: String },
}

/// A downloaded update waiting to be installed.
static PENDING: Mutex<Option<(Update, Vec<u8>)>> = Mutex::new(None);

pub const CHECK_EVERY_HOURS: u64 = 6;

fn set_status(app: &AppHandle, status: UpdateStatus) {
    let shared = app.state::<Shared>();
    shared.lock().unwrap().update = status;
}

fn toast(app: &AppHandle, title: String, body: String, actions: Vec<(&'static str, &str)>, seconds: Option<i64>) {
    let fx = app.state::<Shared>().lock().unwrap().show_toast("update", title, body, actions, seconds);
    ui::apply(app, fx);
}

/// `by_user`: from the menu, so also say when there's nothing new and always download.
pub fn check(app: AppHandle, by_user: bool) {
    tauri::async_runtime::spawn(async move {
        let status = app.state::<Shared>().lock().unwrap().update.clone();
        match status {
            UpdateStatus::Checking | UpdateStatus::Downloading { .. } => return,
            UpdateStatus::Ready { version } => {
                if by_user {
                    toast(&app, format!("Ebb {version} is ready"), "Restart Ebb to start using it.".into(),
                        vec![("restartUpdate", "Restart now")], None);
                }
                return;
            }
            _ => {}
        }
        set_status(&app, UpdateStatus::Checking);
        let found = match app.updater() {
            Ok(u) => u.check().await,
            Err(e) => Err(e),
        };
        match found {
            Ok(Some(update)) => {
                let version = update.version.clone();
                let auto = app.state::<Shared>().lock().unwrap().prefs().auto_update;
                if auto || by_user {
                    download(app, update).await;
                } else {
                    set_status(&app, UpdateStatus::Available { version: version.clone() });
                    *PENDING.lock().unwrap() = Some((update, vec![]));
                    toast(&app, format!("Ebb {version} is available"), "See what's new and install it in one click.".into(),
                        vec![("installUpdate", "Update"), ("dismiss", "Later")], None);
                }
            }
            Ok(None) => {
                set_status(&app, UpdateStatus::UpToDate);
                if by_user {
                    let v = app.package_info().version.to_string();
                    toast(&app, "You're up to date".into(), format!("Ebb {v} is the latest version."), vec![], Some(5));
                }
            }
            Err(e) => {
                log::warn(&format!("update check failed: {e}"));
                set_status(&app, UpdateStatus::Error { message: e.to_string() });
                if by_user {
                    toast(&app, "Couldn't check for updates".into(), "Check your internet connection and try again.".into(), vec![], Some(6));
                }
            }
        }
    });
}

async fn download(app: AppHandle, update: Update) {
    let version = update.version.clone();
    set_status(&app, UpdateStatus::Downloading { version: version.clone(), percent: 0 });
    let mut received: u64 = 0;
    let progress_app = app.clone();
    let progress_version = version.clone();
    let result = update
        .download(
            move |chunk, total| {
                received += chunk as u64;
                if let Some(total) = total.filter(|t| *t > 0) {
                    let percent = (received * 100 / total).min(100) as u8;
                    set_status(&progress_app, UpdateStatus::Downloading { version: progress_version.clone(), percent });
                }
            },
            || {},
        )
        .await;
    match result {
        Ok(bytes) => {
            *PENDING.lock().unwrap() = Some((update, bytes));
            set_status(&app, UpdateStatus::Ready { version: version.clone() });
            toast(&app, format!("Ebb {version} is ready"),
                "Restart now, or Ebb will update itself the next time you step away.".into(),
                vec![("restartUpdate", "Restart now"), ("dismiss", "Later")], None);
        }
        Err(e) => {
            log::warn(&format!("update download failed: {e}"));
            set_status(&app, UpdateStatus::Error { message: e.to_string() });
        }
    }
}

/// Install what's pending (downloading it first if needed) and restart into it.
pub fn install_and_restart(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let pending = PENDING.lock().unwrap().take();
        let Some((update, bytes)) = pending else { return };
        if bytes.is_empty() {
            // "Update" pressed with auto-update off: fetch it now; the Ready toast follows.
            download(app, update).await;
            return;
        }
        app.state::<Shared>().lock().unwrap().stats.save_if_needed();
        match update.install(bytes) {
            Ok(()) => app.restart(),
            Err(e) => {
                log::warn(&format!("update install failed: {e}"));
                set_status(&app, UpdateStatus::Error { message: e.to_string() });
                toast(&app, "Update failed".into(), "Download the latest Ebb from GitHub instead.".into(), vec![], Some(10));
            }
        }
    });
}

pub mod log {
    pub fn warn(message: &str) {
        eprintln!("Ebb: {message}");
    }
}
