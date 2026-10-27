use crate::clock::{ClockEvent, Phase, SessionClock, TickInput};
use crate::platform::{self, Presence};
use crate::prefs::{Preferences, PreferencesStore};
use crate::stats::{self, DayStats, StatsStore, Wellbeing};
use crate::updater::UpdateStatus;
use crate::wellness::{NudgeKind, Wellness, WellnessEvent};
use chrono::{DateTime, Datelike, Local, Timelike};
use serde::Serialize;
use std::path::Path;

/// Why we think you're in a meeting. Shown in the menu so the user can trust the decision.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSignals {
    pub camera: bool,
    pub microphone: bool,
    pub calendar_event: Option<String>,
    pub manual: bool,
}

impl MeetingSignals {
    pub fn in_meeting(&self) -> bool {
        self.camera || self.microphone || self.calendar_event.is_some() || self.manual
    }

    pub fn summary(&self) -> Option<String> {
        let mut parts = vec![];
        if self.manual { parts.push("marked by you".to_string()) }
        if self.camera { parts.push("camera on".to_string()) }
        if self.microphone { parts.push("mic on".to_string()) }
        if let Some(e) = &self.calendar_event { parts.push(format!("“{e}”")) }
        (!parts.is_empty()).then(|| parts.join(" · "))
    }
}

/// What Ebb thinks you're doing right now, in words the menu can show.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Activity {
    /// Recent keyboard or mouse input.
    Using,
    /// A video is playing.
    Watching,
    /// No input, but within the grace window: reading, watching a job run.
    Reading,
    /// Screen locked or asleep.
    Locked,
    /// No input for longer than the grace window.
    Gone,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToastAction {
    pub id: &'static str,
    pub label: String,
}

/// A small card in the corner of the screen: heads-ups, nudges, updates.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Toast {
    pub id: u64,
    pub kind: &'static str,
    pub title: String,
    pub body: String,
    pub actions: Vec<ToastAction>,
    /// Hide by itself after this many seconds.
    pub seconds: Option<i64>,
}

/// Window and system work the engine wants done. Runs on the main thread, outside the lock.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    ShowBreak { strict: bool },
    HideBreak,
    /// During a break: rebuild if monitors changed, and in strict mode keep focus.
    TendBreak { strict: bool },
    ShowEyeBreak,
    HideEyeBreak,
    ShowToast,
    HideToast,
    SetDim(Option<f64>),
    HideRequest,
    InstallUpdate,
    SetAutostart(bool),
    RequestCalendar,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitInfo {
    pub enabled: bool,
    pub used_seconds: i64,
    pub limit_seconds: i64,
}

/// Everything a window needs to draw itself. Sent to all windows every second.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub phase: Phase,
    pub seconds_until_break: i64,
    pub worked_seconds: i64,
    pub cycle_progress: f64,
    pub exceptions_left: i64,
    pub max_exception_minutes: i64,
    /// For each of the prefs' exception choices: why it's refused, or None if allowed.
    pub exception_denials: Vec<(i64, Option<String>)>,
    pub last_denial: Option<String>,
    pub meeting: MeetingSignals,
    pub in_meeting: bool,
    pub meeting_summary: Option<String>,
    pub activity: Activity,
    pub prefs: Preferences,
    pub today: DayStats,
    pub today_wellbeing: Wellbeing,
    pub eye_break_remaining: Option<i64>,
    pub seconds_until_eye_break: i64,
    pub wind_down: bool,
    pub limit: LimitInfo,
    pub update: UpdateStatus,
    pub toast: Option<Toast>,
    pub version: String,
    pub os: &'static str,
    pub calendar_supported: bool,
}

/// Wall-clock gap between ticks that means the computer slept or the app was suspended.
const GAP_SECONDS: i64 = 5;
/// While at the screen past the daily limit or during wind-down, remind every this many minutes.
const REMINDER_EVERY_MINUTES: i64 = 30;

pub struct Core {
    pub clock: SessionClock,
    pub wellness: Wellness,
    pub prefs: PreferencesStore,
    pub stats: StatsStore,
    pub meeting: MeetingSignals,
    pub activity: Activity,
    pub update: UpdateStatus,
    pub toast: Option<Toast>,
    pub last_denial: Option<String>,
    pub smoke: bool,
    pub version: String,
    #[cfg(target_os = "macos")]
    calendar: Option<platform::Calendar>,
    calendar_checked_at: i64,
    last_tick: DateTime<Local>,
    tick_count: i64,
    current_day: String,
    toast_seq: u64,
    wind_down_on: bool,
    wind_down_counter: i64,
    limit_stage: u8,
    limit_counter: i64,
}

impl Core {
    pub fn new(data_dir: &Path, legacy_stats: Option<&Path>, smoke: bool, version: String) -> Self {
        let mut prefs = PreferencesStore::load(data_dir);
        if smoke {
            // Seconds-long "minutes": a 2-minute heads-up would cover the whole 50 s cycle.
            let mut p = prefs.prefs.clone();
            p.warning_seconds = 10;
            prefs.set(p);
        }
        let mut stats = StatsStore::load(data_dir);
        if let Some(legacy) = legacy_stats {
            stats.import_legacy(legacy);
        }
        let minute = if smoke { 1 } else { 60 };
        let mut clock = SessionClock::new(prefs.prefs.clone(), minute);
        clock.restore_exceptions_used(stats.today().exceptions.len() as i64);
        Self {
            clock,
            wellness: Wellness::new(minute),
            prefs,
            stats,
            meeting: MeetingSignals::default(),
            activity: Activity::Using,
            update: UpdateStatus::Idle,
            toast: None,
            last_denial: None,
            smoke,
            version,
            #[cfg(target_os = "macos")]
            calendar: None,
            calendar_checked_at: i64::MIN,
            last_tick: Local::now(),
            tick_count: 0,
            current_day: stats::today_key(),
            toast_seq: 0,
            wind_down_on: false,
            wind_down_counter: 0,
            limit_stage: 0,
            limit_counter: 0,
        }
    }

    pub fn prefs(&self) -> &Preferences {
        &self.prefs.prefs
    }

    fn log(&self, what: impl std::fmt::Debug) {
        if self.smoke {
            use std::io::Write;
            println!("EBB_EVENT {what:?}");
            let _ = std::io::stdout().flush();
        }
    }

    pub fn startup_effects(&mut self) -> Vec<Effect> {
        self.log("launched");
        let mut fx = vec![];
        if self.prefs().detect_calendar {
            fx.push(Effect::RequestCalendar);
        }
        fx
    }

    // MARK: Tick

    pub fn tick(&mut self, now: DateTime<Local>) -> Vec<Effect> {
        let mut fx = vec![];
        self.roll_day_if_needed();
        fx.extend(self.catch_up_after_gap(now));
        self.last_tick = now;
        self.tick_count += 1;

        // Hardware polling is cheap, but every 2 s is plenty.
        if self.tick_count % 2 == 0 {
            self.poll_meeting(now);
        }

        let presence = if self.smoke { Presence::default() } else { platform::presence() };
        let p = self.prefs().clone();
        let watching = presence.media_playing && p.count_media_as_screen_time;
        let in_meeting = self.meeting.in_meeting();
        let input = TickInput { idle_seconds: presence.idle_seconds, in_meeting, watching, screen_off: presence.screen_off, now };
        let worked_before = self.clock.worked_seconds();
        let events = self.clock.tick(&input);
        let at_screen = self.clock.worked_seconds() > worked_before;

        self.activity = if presence.screen_off {
            Activity::Locked
        } else if watching {
            Activity::Watching
        } else if presence.idle_seconds < 30.0 || in_meeting {
            Activity::Using
        } else if presence.idle_seconds < (p.passive_grace_minutes * self.clock.minute_length()) as f64 {
            Activity::Reading
        } else {
            Activity::Gone
        };

        if at_screen {
            self.record_second(now, in_meeting, presence.idle_seconds >= 30.0 && !watching && !in_meeting);
        }
        fx.extend(self.handle(events));

        let can_interrupt = matches!(self.clock.phase(), Phase::Working) && !in_meeting;
        let wellness_events = self.wellness.tick(&p, at_screen, can_interrupt);
        fx.extend(self.handle_wellness(wellness_events));

        fx.extend(self.check_wind_down(now, at_screen));
        fx.extend(self.check_daily_limit(at_screen));
        fx.extend(self.check_weekly_report(now, at_screen));

        if let Phase::OnBreak { .. } = self.clock.phase() {
            fx.push(Effect::TendBreak { strict: p.strict_mode });
        }
        if matches!(self.update, UpdateStatus::Ready { .. })
            && p.auto_update
            && matches!(self.clock.phase(), Phase::Away)
            && !self.smoke
        {
            // You stepped away: a good moment to restart into the new version.
            fx.push(Effect::InstallUpdate);
        }
        if self.tick_count % 30 == 0 {
            self.stats.save_if_needed();
        }
        fx
    }

    fn record_second(&mut self, now: DateTime<Local>, in_meeting: bool, passive: bool) {
        let worked = self.clock.worked_seconds();
        let late = self.prefs().in_wind_down((now.hour() * 60 + now.minute()) as i64);
        self.stats.update(|d| {
            d.active_seconds += 1;
            if in_meeting { d.meeting_seconds += 1 }
            if passive { d.passive_seconds += 1 }
            if late { d.late_night_seconds += 1 }
            d.longest_stretch_seconds = d.longest_stretch_seconds.max(worked);
        });
    }

    fn roll_day_if_needed(&mut self) {
        let key = stats::today_key();
        if key == self.current_day {
            return;
        }
        self.stats.save_if_needed();
        self.current_day = key;
        self.clock.start_new_day();
        self.limit_stage = 0;
        self.limit_counter = 0;
    }

    /// Timers don't fire while the computer sleeps; a big jump means you were away.
    fn catch_up_after_gap(&mut self, now: DateTime<Local>) -> Vec<Effect> {
        let gap = (now - self.last_tick).num_seconds();
        if gap <= GAP_SECONDS {
            return vec![];
        }
        self.last_tick = now;
        let events = self.clock.register_gap(gap);
        self.handle(events)
    }

    fn poll_meeting(&mut self, now: DateTime<Local>) {
        let p = self.prefs().clone();
        if self.smoke {
            return;
        }
        self.meeting.camera = p.detect_camera && platform::camera_in_use();
        self.meeting.microphone = p.detect_microphone && platform::microphone_in_use();
        #[cfg(target_os = "macos")]
        {
            if !p.detect_calendar {
                self.meeting.calendar_event = None;
            } else if now.timestamp() - self.calendar_checked_at >= 60 {
                // Calendar data changes slowly; once a minute is plenty.
                self.calendar_checked_at = now.timestamp();
                let cal = self.calendar.get_or_insert_with(platform::Calendar::new);
                self.meeting.calendar_event = cal.current_event();
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (now, self.calendar_checked_at);
    }

    pub fn request_calendar_access(&mut self) {
        #[cfg(target_os = "macos")]
        {
            self.calendar_checked_at = i64::MIN;
            self.calendar.get_or_insert_with(platform::Calendar::new).request_access();
        }
    }

    // MARK: Clock events → side effects

    pub fn handle(&mut self, events: Vec<ClockEvent>) -> Vec<Effect> {
        let mut fx = vec![];
        for event in events {
            self.log(&event);
            match event {
                ClockEvent::Warning { seconds_left } => {
                    let body = "Start wrapping up. In the middle of something? Ask for more time.".to_string();
                    fx.extend(self.show_toast(
                        "warning",
                        format!("Break in {}", format_duration(seconds_left)),
                        body,
                        vec![("breakNow", "Take break now"), ("moreTime", "I'm in flow: more time")],
                        None,
                    ));
                }
                ClockEvent::BreakStarted => {
                    fx.extend(self.hide_toast_kind("warning"));
                    fx.push(Effect::HideEyeBreak);
                    fx.push(Effect::HideRequest);
                    fx.push(Effect::ShowBreak { strict: self.prefs().strict_mode });
                }
                ClockEvent::BreakEnded { completed } => {
                    fx.push(Effect::HideBreak);
                    self.stats.update(|d| if completed { d.breaks_completed += 1 } else { d.breaks_skipped += 1 });
                    if completed {
                        self.wellness.rested();
                        let minutes = self.prefs().work_minutes;
                        fx.extend(self.show_toast("info", "Welcome back".into(), format!("Fresh {minutes}-minute cycle started."), vec![], Some(6)));
                    }
                }
                ClockEvent::NaturalBreak { uncounted_seconds, .. } => {
                    self.wellness.rested();
                    self.stats.update(|d| {
                        d.natural_breaks += 1;
                        // You'd already left: that silent time wasn't screen time after all.
                        d.active_seconds = (d.active_seconds - uncounted_seconds).max(0);
                        d.passive_seconds = (d.passive_seconds - uncounted_seconds).max(0);
                    });
                    fx.extend(self.hide_toast_kind("warning"));
                }
                ClockEvent::DeferredForMeeting => {
                    self.stats.update(|d| d.meeting_deferrals += 1);
                    fx.extend(self.show_toast(
                        "meeting",
                        "Break postponed: you're in a meeting".into(),
                        "Ebb will wait until the call ends.".into(),
                        vec![],
                        Some(8),
                    ));
                }
                ClockEvent::BreakInterruptedByMeeting => {
                    fx.push(Effect::HideBreak);
                    self.stats.update(|d| d.meeting_deferrals += 1);
                }
                ClockEvent::MeetingEnded { grace_seconds } => {
                    fx.extend(self.show_toast(
                        "meeting",
                        "Meeting over".into(),
                        format!("Your break starts in {}. Grab some water.", format_duration(grace_seconds)),
                        vec![("breakNow", "Start it now")],
                        Some(grace_seconds.max(5)),
                    ));
                }
                ClockEvent::ExceptionGranted { .. } => {
                    fx.extend(self.hide_toast_kind("warning"));
                }
                ClockEvent::Resumed => {}
            }
        }
        fx
    }

    fn handle_wellness(&mut self, events: Vec<WellnessEvent>) -> Vec<Effect> {
        let mut fx = vec![];
        for event in events {
            self.log(&event);
            match event {
                WellnessEvent::EyeBreakStarted { .. } => fx.push(Effect::ShowEyeBreak),
                WellnessEvent::EyeBreakEnded { completed } => {
                    fx.push(Effect::HideEyeBreak);
                    self.stats.update(|d| if completed { d.eye_breaks_taken += 1 } else { d.eye_breaks_skipped += 1 });
                }
                WellnessEvent::Nudge { nudge } => {
                    self.stats.update(|d| d.nudges += 1);
                    let (title, body) = nudge.message();
                    let kind = match nudge {
                        NudgeKind::Blink => "blink",
                        NudgeKind::Posture => "posture",
                        NudgeKind::Water => "water",
                        NudgeKind::Stretch => "stretch",
                    };
                    fx.extend(self.show_toast(kind, title.into(), body.into(), vec![], Some(10)));
                }
            }
        }
        fx
    }

    fn check_wind_down(&mut self, now: DateTime<Local>, at_screen: bool) -> Vec<Effect> {
        let p = self.prefs().clone();
        let on = p.wind_down_enabled && p.in_wind_down((now.hour() * 60 + now.minute()) as i64);
        let mut fx = vec![];
        if on != self.wind_down_on {
            self.wind_down_on = on;
            self.wind_down_counter = 0;
            fx.push(Effect::SetDim(on.then_some(p.wind_down_dim).filter(|d| *d > 0.0)));
            if on {
                fx.extend(self.show_toast(
                    "windDown",
                    "Time to wind down".into(),
                    "Screens late at night push back sleep. Wrap up what you're doing and plan tomorrow.".into(),
                    vec![],
                    Some(15),
                ));
            }
        } else if on && at_screen {
            self.wind_down_counter += 1;
            if self.wind_down_counter >= REMINDER_EVERY_MINUTES * self.clock.minute_length() {
                self.wind_down_counter = 0;
                fx.extend(self.show_toast(
                    "windDown",
                    format!("It's {}", now.format("%H:%M")),
                    "Still here? Your eyes and your sleep will thank you for stopping soon.".into(),
                    vec![],
                    Some(12),
                ));
            }
        }
        fx
    }

    pub fn wind_down_dim(&self) -> Option<f64> {
        let d = self.prefs().wind_down_dim;
        (self.wind_down_on && d > 0.0).then_some(d)
    }

    fn check_daily_limit(&mut self, at_screen: bool) -> Vec<Effect> {
        let p = self.prefs().clone();
        if !p.daily_limit_enabled || !at_screen {
            return vec![];
        }
        let used = self.stats.today().active_seconds;
        let limit = p.daily_limit_minutes * self.clock.minute_length();
        if self.limit_stage == 0 && used * 10 >= limit * 9 {
            self.limit_stage = 1;
            let left = (limit - used).max(0);
            return self.show_toast(
                "limit",
                format!("{} of screen time left today", format_duration(left)),
                "You're close to your daily limit. Save the rest for what matters.".into(),
                vec![],
                Some(12),
            );
        }
        if self.limit_stage <= 1 && used >= limit {
            self.limit_stage = 2;
            self.limit_counter = 0;
            return self.show_toast(
                "limit",
                "Daily screen time reached".into(),
                format!("You've spent {} at the screen today. Time to step away for the day.", format_duration(used)),
                vec![("dismiss", "OK")],
                None,
            );
        }
        if self.limit_stage == 2 {
            self.limit_counter += 1;
            if self.limit_counter >= REMINDER_EVERY_MINUTES * self.clock.minute_length() {
                self.limit_counter = 0;
                return self.show_toast(
                    "limit",
                    format!("{} over today's limit", format_duration(used - limit)),
                    "Every minute away from the screen now helps your eyes and sleep.".into(),
                    vec![("dismiss", "OK")],
                    None,
                );
            }
        }
        vec![]
    }

    /// On the first day of a new week, once you're at the screen, offer last week's report.
    fn check_weekly_report(&mut self, now: DateTime<Local>, at_screen: bool) -> Vec<Effect> {
        if !at_screen || self.toast.is_some() || self.tick_count < 120 {
            return vec![];
        }
        let week = now.iso_week();
        let key = format!("{}-W{:02}", week.year(), week.week());
        if self.prefs().weekly_report_shown == key {
            return vec![];
        }
        let last_week = stats::week_report(&self.stats.days, self.prefs(), now.date_naive() - chrono::Duration::days(1));
        let mut p = self.prefs().clone();
        p.weekly_report_shown = key;
        self.prefs.set(p);
        let Some(score) = last_week.average_score else { return vec![] };
        self.show_toast(
            "report",
            format!("Your week: wellness {score}/100"),
            format!("{} at the screen, {} breaks taken.", format_duration(last_week.total_seconds), last_week.breaks_taken),
            vec![("openReport", "See report")],
            Some(20),
        )
    }

    // MARK: Toasts

    pub fn show_toast(
        &mut self,
        kind: &'static str,
        title: String,
        body: String,
        actions: Vec<(&'static str, &str)>,
        seconds: Option<i64>,
    ) -> Vec<Effect> {
        self.toast_seq += 1;
        self.log(format!("toast {kind}: {title}"));
        self.toast = Some(Toast {
            id: self.toast_seq,
            kind,
            title,
            body,
            actions: actions.into_iter().map(|(id, label)| ToastAction { id, label: label.to_string() }).collect(),
            seconds,
        });
        vec![Effect::ShowToast]
    }

    pub fn dismiss_toast(&mut self, id: u64) -> Vec<Effect> {
        if self.toast.as_ref().is_some_and(|t| t.id == id) {
            self.toast = None;
            return vec![Effect::HideToast];
        }
        vec![]
    }

    fn hide_toast_kind(&mut self, kind: &str) -> Vec<Effect> {
        if self.toast.as_ref().is_some_and(|t| t.kind == kind) {
            self.toast = None;
            return vec![Effect::HideToast];
        }
        vec![]
    }

    // MARK: Prefs

    pub fn set_prefs(&mut self, new: Preferences) -> Vec<Effect> {
        let old = self.prefs().clone();
        self.prefs.set(new);
        let new = self.prefs().clone();
        self.clock.update_prefs(new.clone());
        let mut fx = vec![];
        if new.detect_calendar && !old.detect_calendar {
            fx.push(Effect::RequestCalendar);
        }
        if new.launch_at_login != old.launch_at_login {
            fx.push(Effect::SetAutostart(new.launch_at_login));
        }
        if self.wind_down_on && new.wind_down_dim != old.wind_down_dim {
            fx.push(Effect::SetDim(self.wind_down_dim()));
        }
        // Re-evaluated on the next tick, so turning wind-down on or off applies at once.
        if new.wind_down_enabled != old.wind_down_enabled || new.wind_down_start != old.wind_down_start || new.wind_down_end != old.wind_down_end {
            self.wind_down_on = false;
            fx.push(Effect::SetDim(None));
        }
        if new.daily_limit_minutes != old.daily_limit_minutes || new.daily_limit_enabled != old.daily_limit_enabled {
            self.limit_stage = 0;
        }
        fx
    }

    // MARK: Snapshot

    pub fn snapshot(&self) -> Snapshot {
        let p = self.prefs().clone();
        let today = self.stats.today();
        Snapshot {
            phase: self.clock.phase().clone(),
            seconds_until_break: self.clock.seconds_until_break(),
            worked_seconds: self.clock.worked_seconds(),
            cycle_progress: self.clock.cycle_progress(),
            exceptions_left: self.clock.exceptions_left_today(),
            max_exception_minutes: self.clock.max_exception_minutes(),
            exception_denials: p
                .exception_choices_minutes
                .iter()
                .map(|m| (*m, self.clock.check_exception(*m).map(|d| d.to_string())))
                .collect(),
            last_denial: self.last_denial.clone(),
            in_meeting: self.meeting.in_meeting(),
            meeting_summary: self.meeting.summary(),
            meeting: self.meeting.clone(),
            activity: self.activity,
            today_wellbeing: today.wellness(&p),
            eye_break_remaining: self.wellness.eye_break_remaining(),
            seconds_until_eye_break: self.wellness.seconds_until_eye_break(&p),
            wind_down: self.wind_down_on,
            limit: LimitInfo {
                enabled: p.daily_limit_enabled,
                used_seconds: today.active_seconds,
                limit_seconds: p.daily_limit_minutes * 60,
            },
            today,
            update: self.update.clone(),
            toast: self.toast.clone(),
            version: self.version.clone(),
            os: platform::OS,
            calendar_supported: cfg!(target_os = "macos"),
            prefs: p,
        }
    }
}

pub fn format_duration(seconds: i64) -> String {
    let s = seconds.max(0);
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::tempdir;

    fn core() -> Core {
        Core::new(&tempdir(), None, true, "test".into())
    }

    fn run(c: &mut Core, seconds: i64) -> Vec<Effect> {
        let start = c.last_tick;
        (1..=seconds).flat_map(|i| c.tick(start + chrono::Duration::seconds(i))).collect()
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(65), "1:05");
        assert_eq!(format_duration(3725), "1:02:05");
        assert_eq!(format_duration(-4), "0:00");
    }

    #[test]
    fn meeting_summary_explains_itself() {
        let m = MeetingSignals { camera: true, manual: true, calendar_event: Some("Standup".into()), ..Default::default() };
        assert!(m.in_meeting());
        assert_eq!(m.summary().unwrap(), "marked by you · camera on · “Standup”");
        assert_eq!(MeetingSignals::default().summary(), None);
    }

    #[test]
    fn full_cycle_shows_and_hides_break_and_records_stats() {
        // Smoke mode: one "minute" is one second. Work 50 s, break 10 s.
        let mut c = core();
        let fx = run(&mut c, 50);
        assert!(fx.contains(&Effect::ShowBreak { strict: false }));
        assert_eq!(c.stats.today().active_seconds, 50);
        let fx = run(&mut c, 10);
        assert!(fx.contains(&Effect::HideBreak));
        assert_eq!(c.stats.today().breaks_completed, 1);
        assert_eq!(c.toast.as_ref().unwrap().title, "Welcome back");
    }

    #[test]
    fn eye_breaks_and_nudges_happen_during_work() {
        let mut c = core();
        let mut p = c.prefs().clone();
        p.warning_seconds = 5; // a heads-up in progress holds eye breaks back
        c.set_prefs(p);
        let fx = run(&mut c, 21);
        assert!(fx.contains(&Effect::ShowEyeBreak), "20-20-20 after 20 'minutes'");
        let fx = run(&mut c, 20);
        assert!(fx.contains(&Effect::HideEyeBreak));
        assert_eq!(c.stats.today().eye_breaks_taken, 1);
    }

    #[test]
    fn sleep_gap_is_a_natural_break() {
        let mut c = core();
        run(&mut c, 10);
        let later = c.last_tick + chrono::Duration::seconds(600);
        c.tick(later);
        assert_eq!(c.stats.today().natural_breaks, 1);
        assert_eq!(c.clock.worked_seconds(), 1, "a fresh cycle started on return");
    }

    #[test]
    fn exception_toast_and_dismissal() {
        let mut c = core();
        let fx = c.show_toast("info", "Hi".into(), "there".into(), vec![], Some(3));
        assert_eq!(fx, vec![Effect::ShowToast]);
        let id = c.toast.as_ref().unwrap().id;
        assert_eq!(c.dismiss_toast(id + 1), vec![]);
        assert_eq!(c.dismiss_toast(id), vec![Effect::HideToast]);
        assert!(c.toast.is_none());
    }

    #[test]
    fn daily_limit_warns_then_tells_you_to_stop() {
        let mut c = core();
        let mut p = c.prefs().clone();
        p.daily_limit_enabled = true;
        p.daily_limit_minutes = 30; // 30 s in smoke mode
        p.work_minutes = 100;
        p.hard_cap_minutes = 200;
        p.eye_breaks_enabled = false;
        p.blink.enabled = false;
        p.posture.enabled = false;
        p.water.enabled = false;
        c.set_prefs(p);
        run(&mut c, 27);
        assert!(c.toast.as_ref().unwrap().title.contains("left today"));
        run(&mut c, 3);
        assert_eq!(c.toast.as_ref().unwrap().title, "Daily screen time reached");
    }

    #[test]
    fn prefs_changes_reach_the_clock_and_ask_for_side_effects() {
        let mut c = core();
        let mut p = c.prefs().clone();
        p.work_minutes = 25;
        p.launch_at_login = true;
        p.detect_calendar = true;
        let fx = c.set_prefs(p);
        assert_eq!(c.clock.seconds_until_break(), 25);
        assert!(fx.contains(&Effect::SetAutostart(true)));
        assert!(fx.contains(&Effect::RequestCalendar));
    }
}
