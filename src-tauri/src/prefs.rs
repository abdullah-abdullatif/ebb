use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One kind of gentle reminder (blink, posture, water, stretch).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Nudge {
    pub enabled: bool,
    pub every_minutes: i64,
}

/// Everything the user can tune. Stored as prefs.json in the app data folder.
/// Unknown or missing fields fall back to defaults, so old files keep loading.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    // Rhythm
    pub work_minutes: i64,
    pub break_minutes: i64,
    /// Heads-up before a break starts.
    pub warning_seconds: i64,
    /// Away from the computer this long counts as a break you took yourself.
    pub natural_break_minutes: i64,
    /// With no keyboard/mouse input, keep counting screen time for this long
    /// (reading, watching a build, a video). After that you're treated as gone.
    pub passive_grace_minutes: i64,
    /// A playing video (an app keeping the display awake) or media player counts as screen time.
    pub count_media_as_screen_time: bool,

    // Enforcement
    /// Strict: no emergency skip; on macOS also hides Dock/menu bar and blocks app switching.
    pub strict_mode: bool,
    pub emergency_skip_hold_seconds: f64,

    // Exceptions ("I'm in the middle of something")
    pub exceptions_per_day: i64,
    pub exception_choices_minutes: Vec<i64>,
    pub require_exception_reason: bool,
    /// Never allow more than this much continuous screen work, exceptions included.
    pub hard_cap_minutes: i64,

    // Meetings
    pub detect_camera: bool,
    pub detect_microphone: bool,
    /// macOS only (EventKit).
    pub detect_calendar: bool,
    pub post_meeting_grace_seconds: i64,

    // Eyes
    /// 20-20-20: every N minutes, look 6 m (20 ft) away for 20 seconds.
    pub eye_breaks_enabled: bool,
    pub eye_break_every_minutes: i64,
    pub eye_break_seconds: i64,

    // Nudges
    pub blink: Nudge,
    pub posture: Nudge,
    pub water: Nudge,
    pub stretch: Nudge,

    // Daily limit & night
    pub daily_limit_enabled: bool,
    pub daily_limit_minutes: i64,
    pub wind_down_enabled: bool,
    /// Minutes after midnight, local time. 22:30 → 1350.
    pub wind_down_start: i64,
    pub wind_down_end: i64,
    /// 0 – 0.6: how much to dim and warm the screen during wind-down.
    pub wind_down_dim: f64,

    // Mind
    /// Ask "how are you feeling?" on the break screen.
    pub mood_check_in: bool,

    // System
    pub launch_at_login: bool,
    pub show_timer_in_menu_bar: bool,
    pub auto_update: bool,
    /// ISO week ("2026-W44") whose weekly report was last offered. Not a setting.
    pub weekly_report_shown: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            work_minutes: 50,
            break_minutes: 10,
            warning_seconds: 120,
            natural_break_minutes: 5,
            passive_grace_minutes: 15,
            count_media_as_screen_time: true,
            strict_mode: false,
            emergency_skip_hold_seconds: 3.0,
            exceptions_per_day: 3,
            exception_choices_minutes: vec![5, 10, 15, 25],
            require_exception_reason: true,
            hard_cap_minutes: 120,
            detect_camera: true,
            detect_microphone: true,
            detect_calendar: false,
            post_meeting_grace_seconds: 90,
            eye_breaks_enabled: true,
            eye_break_every_minutes: 20,
            eye_break_seconds: 20,
            blink: Nudge { enabled: true, every_minutes: 15 },
            posture: Nudge { enabled: true, every_minutes: 30 },
            water: Nudge { enabled: true, every_minutes: 45 },
            stretch: Nudge { enabled: false, every_minutes: 60 },
            daily_limit_enabled: false,
            daily_limit_minutes: 480,
            wind_down_enabled: false,
            wind_down_start: 22 * 60 + 30,
            wind_down_end: 6 * 60,
            wind_down_dim: 0.25,
            mood_check_in: true,
            launch_at_login: false,
            show_timer_in_menu_bar: true,
            auto_update: true,
            weekly_report_shown: String::new(),
        }
    }
}

impl Preferences {
    /// Keeps hand-edited or out-of-range values from breaking the clock.
    pub fn sanitized(mut self) -> Self {
        self.work_minutes = self.work_minutes.clamp(1, 240);
        self.break_minutes = self.break_minutes.clamp(1, 60);
        self.warning_seconds = self.warning_seconds.clamp(0, 900);
        self.natural_break_minutes = self.natural_break_minutes.clamp(1, 60);
        self.passive_grace_minutes = self.passive_grace_minutes.clamp(1, 120);
        self.emergency_skip_hold_seconds = self.emergency_skip_hold_seconds.clamp(0.5, 15.0);
        self.exceptions_per_day = self.exceptions_per_day.clamp(0, 20);
        self.exception_choices_minutes.retain(|m| (1..=120).contains(m));
        if self.exception_choices_minutes.is_empty() {
            self.exception_choices_minutes = vec![5, 10, 15, 25];
        }
        self.hard_cap_minutes = self.hard_cap_minutes.clamp(self.work_minutes, 480);
        self.post_meeting_grace_seconds = self.post_meeting_grace_seconds.clamp(0, 900);
        self.eye_break_every_minutes = self.eye_break_every_minutes.clamp(5, 120);
        self.eye_break_seconds = self.eye_break_seconds.clamp(5, 120);
        for n in [&mut self.blink, &mut self.posture, &mut self.water, &mut self.stretch] {
            n.every_minutes = n.every_minutes.clamp(1, 240);
        }
        self.daily_limit_minutes = self.daily_limit_minutes.clamp(30, 24 * 60);
        self.wind_down_start = self.wind_down_start.rem_euclid(24 * 60);
        self.wind_down_end = self.wind_down_end.rem_euclid(24 * 60);
        self.wind_down_dim = self.wind_down_dim.clamp(0.0, 0.6);
        self
    }

    /// Is `minute_of_day` (0–1439) inside the wind-down window? The window may cross midnight.
    pub fn in_wind_down(&self, minute_of_day: i64) -> bool {
        let (s, e) = (self.wind_down_start, self.wind_down_end);
        if s == e {
            return false;
        }
        if s < e {
            (s..e).contains(&minute_of_day)
        } else {
            minute_of_day >= s || minute_of_day < e
        }
    }
}

/// Loads and saves prefs.json.
pub struct PreferencesStore {
    path: PathBuf,
    pub prefs: Preferences,
}

impl PreferencesStore {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("prefs.json");
        let prefs = std::fs::read(&path)
            .ok()
            .and_then(|d| serde_json::from_slice::<Preferences>(&d).ok())
            .unwrap_or_default()
            .sanitized();
        Self { path, prefs }
    }

    pub fn set(&mut self, new: Preferences) {
        self.prefs = new.sanitized();
        self.save();
    }

    fn save(&self) {
        if let Ok(data) = serde_json::to_vec_pretty(&self.prefs) {
            crate::stats::write_atomic(&self.path, &data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let p: Preferences = serde_json::from_str(r#"{"workMinutes": 25}"#).unwrap();
        assert_eq!(p.work_minutes, 25);
        assert_eq!(p.break_minutes, 10);
        assert!(p.eye_breaks_enabled);
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempdir();
        let mut s = PreferencesStore::load(&dir);
        assert_eq!(s.prefs, Preferences::default());
        let mut p = s.prefs.clone();
        p.work_minutes = 30;
        p.water.every_minutes = 20;
        s.set(p.clone());
        assert_eq!(PreferencesStore::load(&dir).prefs, p);
    }

    #[test]
    fn sanitizes_bad_values() {
        let mut p = Preferences::default();
        p.work_minutes = 0;
        p.hard_cap_minutes = 1;
        p.exception_choices_minutes = vec![-3, 0];
        p.wind_down_dim = 3.0;
        let p = p.sanitized();
        assert_eq!(p.work_minutes, 1);
        assert_eq!(p.hard_cap_minutes, 1);
        assert_eq!(p.exception_choices_minutes, vec![5, 10, 15, 25]);
        assert_eq!(p.wind_down_dim, 0.6);
    }

    #[test]
    fn wind_down_window_crosses_midnight() {
        let p = Preferences::default(); // 22:30 → 06:00
        assert!(!p.in_wind_down(22 * 60 + 29));
        assert!(p.in_wind_down(22 * 60 + 30));
        assert!(p.in_wind_down(0));
        assert!(p.in_wind_down(5 * 60 + 59));
        assert!(!p.in_wind_down(6 * 60));

        let mut day = p.clone();
        day.wind_down_start = 13 * 60;
        day.wind_down_end = 14 * 60;
        assert!(day.in_wind_down(13 * 60 + 30));
        assert!(!day.in_wind_down(14 * 60));
    }

    pub fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ebb-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}

#[cfg(test)]
pub use tests::tempdir;
