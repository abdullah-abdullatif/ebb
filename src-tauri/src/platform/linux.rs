//! Linux desktops differ a lot, so every check tries the common ways in turn:
//! GNOME and KDE over D-Bus first, then X11. Anything that fails just reports "no".

use super::Presence;
use std::sync::OnceLock;
use zbus::blocking::Connection;
use zbus::zvariant::OwnedValue;

fn session() -> Option<&'static Connection> {
    static BUS: OnceLock<Option<Connection>> = OnceLock::new();
    BUS.get_or_init(|| Connection::session().ok()).as_ref()
}

fn system() -> Option<&'static Connection> {
    static BUS: OnceLock<Option<Connection>> = OnceLock::new();
    BUS.get_or_init(|| Connection::system().ok()).as_ref()
}

fn call<B, R>(conn: &Connection, dest: &str, path: &str, iface: &str, method: &str, body: &B) -> Option<R>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    let reply = conn.call_method(Some(dest), path, Some(iface), method, body).ok()?;
    let body = reply.body();
    body.deserialize::<R>().ok()
}

fn property(conn: &Connection, dest: &str, path: &str, iface: &str, name: &str) -> Option<OwnedValue> {
    call(conn, dest, path, "org.freedesktop.DBus.Properties", "Get", &(iface, name))
}

pub fn presence() -> Presence {
    Presence { idle_seconds: idle_seconds(), screen_off: screen_off(), media_playing: media_playing() }
}

fn idle_seconds() -> f64 {
    if let Some(bus) = session() {
        // GNOME (Wayland and X11).
        if let Some(ms) = call::<_, u64>(bus, "org.gnome.Mutter.IdleMonitor", "/org/gnome/Mutter/IdleMonitor/Core",
            "org.gnome.Mutter.IdleMonitor", "GetIdletime", &())
        {
            return ms as f64 / 1000.0;
        }
        // KDE Plasma and others implementing the freedesktop screensaver API (milliseconds).
        if let Some(ms) = call::<_, u32>(bus, "org.freedesktop.ScreenSaver", "/org/freedesktop/ScreenSaver",
            "org.freedesktop.ScreenSaver", "GetSessionIdleTime", &())
        {
            return ms as f64 / 1000.0;
        }
    }
    x11_idle_seconds().unwrap_or(0.0)
}

fn x11_idle_seconds() -> Option<f64> {
    use x11rb::connection::Connection as _;
    use x11rb::protocol::screensaver::ConnectionExt as _;
    static X: OnceLock<Option<(x11rb::rust_connection::RustConnection, u32)>> = OnceLock::new();
    let (conn, root) = X
        .get_or_init(|| {
            let (conn, screen) = x11rb::connect(None).ok()?;
            let root = conn.setup().roots.get(screen)?.root;
            Some((conn, root))
        })
        .as_ref()?;
    let info = conn.screensaver_query_info(*root).ok()?.reply().ok()?;
    Some(info.ms_since_user_input as f64 / 1000.0)
}

fn screen_off() -> bool {
    if let Some(bus) = session() {
        for (dest, path) in [("org.gnome.ScreenSaver", "/org/gnome/ScreenSaver"), ("org.freedesktop.ScreenSaver", "/org/freedesktop/ScreenSaver")] {
            let iface = dest;
            if let Some(active) = call::<_, bool>(bus, dest, path, iface, "GetActive", &()) {
                if active {
                    return true;
                }
                break;
            }
        }
    }
    // systemd-logind knows about lock screens from any desktop.
    system()
        .and_then(|bus| property(bus, "org.freedesktop.login1", "/org/freedesktop/login1/session/auto", "org.freedesktop.login1.Session", "LockedHint"))
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false)
}

fn media_playing() -> bool {
    let Some(bus) = session() else { return false };
    // Video players and browsers inhibit the screensaver while playing (GNOME exposes it as flag 8, "idle").
    if call::<_, bool>(bus, "org.gnome.SessionManager", "/org/gnome/SessionManager", "org.gnome.SessionManager", "IsInhibited", &(8u32,))
        .unwrap_or(false)
    {
        return true;
    }
    // Elsewhere: ask video-capable MPRIS players whether they're playing. Music players are left out
    // on purpose, so music in the background doesn't hide that you walked away.
    const VIDEO_PLAYERS: [&str; 14] = [
        "chrom", "firefox", "brave", "vivaldi", "opera", "edge", "vlc", "mpv", "totem", "celluloid", "smplayer", "haruna", "kodi",
        "plasma-browser-integration",
    ];
    let names: Vec<String> =
        call(bus, "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "ListNames", &()).unwrap_or_default();
    names
        .iter()
        .filter(|n| n.starts_with("org.mpris.MediaPlayer2.") && VIDEO_PLAYERS.iter().any(|p| n.to_lowercase().contains(p)))
        .any(|n| {
            property(bus, n, "/org/mpris/MediaPlayer2", "org.mpris.MediaPlayer2.Player", "PlaybackStatus")
                .and_then(|v| String::try_from(v).ok())
                .is_some_and(|s| s == "Playing")
        })
}

/// Some process has a /dev/video* device open. Only our own user's processes are visible,
/// which is exactly the apps that matter.
pub fn camera_in_use() -> bool {
    let Ok(procs) = std::fs::read_dir("/proc") else { return false };
    let me = std::process::id().to_string();
    procs.flatten().filter(|p| p.file_name().to_string_lossy().chars().all(|c| c.is_ascii_digit()) && p.file_name() != me.as_str()).any(|p| {
        std::fs::read_dir(p.path().join("fd")).is_ok_and(|fds| {
            fds.flatten().any(|fd| std::fs::read_link(fd.path()).is_ok_and(|t| t.to_string_lossy().starts_with("/dev/video")))
        })
    })
}

/// PulseAudio and PipeWire both answer `pactl`: any recording stream means a mic is in use.
pub fn microphone_in_use() -> bool {
    std::process::Command::new("pactl")
        .args(["list", "short", "source-outputs"])
        .stderr(std::process::Stdio::null())
        .output()
        .is_ok_and(|o| o.status.success() && !String::from_utf8_lossy(&o.stdout).trim().is_empty())
}
