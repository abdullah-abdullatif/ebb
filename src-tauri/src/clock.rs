use crate::prefs::Preferences;
use chrono::{DateTime, Local};
use serde::Serialize;

/// Where we are in the work → break cycle.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Phase {
    /// At the screen, the work clock is running.
    Working,
    /// Break is close; a heads-up was sent.
    Warning,
    /// No input for a while (or the screen is locked). Clock is frozen, not reset.
    Idle,
    /// Away long enough to count as a real break. Next activity starts a fresh cycle.
    Away,
    /// A break is due but you're in a meeting, so it waits.
    MeetingHold,
    /// Meeting just ended; break starts when this runs out.
    PostMeetingGrace { remaining: i64 },
    /// Screen is blocked.
    OnBreak { remaining: i64 },
    /// User paused Ebb (e.g. "pause for 1 hour"). None = until resumed.
    Paused { until: Option<DateTime<Local>> },
}

impl Phase {
    pub fn is_on_break(&self) -> bool {
        matches!(self, Phase::OnBreak { .. })
    }
    pub fn is_paused(&self) -> bool {
        matches!(self, Phase::Paused { .. })
    }
}

/// One second of the outside world, as seen by the clock.
#[derive(Clone, Debug)]
pub struct TickInput {
    /// Seconds since the last keyboard/mouse/touch input.
    pub idle_seconds: f64,
    pub in_meeting: bool,
    /// Something is clearly being watched: a video keeps the display awake, or media is playing.
    pub watching: bool,
    /// Screen locked, screensaver running, or display asleep: nobody is looking.
    pub screen_off: bool,
    pub now: DateTime<Local>,
}

impl TickInput {
    pub fn active() -> Self {
        Self { idle_seconds: 0.0, in_meeting: false, watching: false, screen_off: false, now: Local::now() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ClockEvent {
    Warning { seconds_left: i64 },
    BreakStarted,
    BreakEnded { completed: bool },
    /// You stepped away long enough. `uncounted_seconds` were counted as passive screen
    /// time (no input, but maybe watching) before Ebb could tell you had actually left;
    /// they should come off today's screen time.
    NaturalBreak { worked_seconds: i64, uncounted_seconds: i64 },
    DeferredForMeeting,
    BreakInterruptedByMeeting,
    MeetingEnded { grace_seconds: i64 },
    ExceptionGranted { minutes: i64 },
    Resumed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExceptionDenial {
    NoneLeftToday,
    /// Would push continuous work past the hard cap. `max_minutes` is what's still allowed (may be 0).
    HardCap { max_minutes: i64 },
    NotNow,
}

impl std::fmt::Display for ExceptionDenial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoneLeftToday => write!(f, "No exceptions left today."),
            Self::HardCap { max_minutes } => write!(f, "Only {max_minutes} more minutes allowed before the hard limit."),
            Self::NotNow => write!(f, "Nothing to extend right now."),
        }
    }
}

/// Input this recent means a person is definitely at the computer.
const RECENT_INPUT_SECONDS: f64 = 30.0;

/// The pure state machine behind Ebb. No timers, no UI, no system calls:
/// feed it one `tick` per second and it tells you what happened.
#[derive(Clone, Debug)]
pub struct SessionClock {
    prefs: Preferences,
    phase: Phase,
    /// Screen seconds since the last real break.
    worked_seconds: i64,
    /// `worked_seconds` value at which the break is due. Grows with exceptions.
    break_due_at: i64,
    exceptions_used_today: i64,
    warned: bool,
    /// Seconds counted in the current no-input stretch. Taken back if it turns out you'd left.
    passive_seconds: i64,
    /// Seconds in one "minute" of the preferences. 60 in real life; 1 in smoke tests.
    minute_length: i64,
}

impl SessionClock {
    pub fn new(prefs: Preferences, minute_length: i64) -> Self {
        let break_due_at = prefs.work_minutes * minute_length;
        Self {
            prefs,
            phase: Phase::Working,
            worked_seconds: 0,
            break_due_at,
            exceptions_used_today: 0,
            warned: false,
            passive_seconds: 0,
            minute_length,
        }
    }

    fn work_seconds(&self) -> i64 { self.prefs.work_minutes * self.minute_length }
    fn break_seconds(&self) -> i64 { self.prefs.break_minutes * self.minute_length }
    fn natural_break_seconds(&self) -> i64 { self.prefs.natural_break_minutes * self.minute_length }
    fn passive_grace_seconds(&self) -> i64 { self.prefs.passive_grace_minutes * self.minute_length }
    fn hard_cap_seconds(&self) -> i64 { self.prefs.hard_cap_minutes * self.minute_length }

    pub fn phase(&self) -> &Phase { &self.phase }
    pub fn prefs(&self) -> &Preferences { &self.prefs }
    pub fn worked_seconds(&self) -> i64 { self.worked_seconds }
    pub fn minute_length(&self) -> i64 { self.minute_length }
    pub fn seconds_until_break(&self) -> i64 { (self.break_due_at - self.worked_seconds).max(0) }
    pub fn exceptions_left_today(&self) -> i64 { (self.prefs.exceptions_per_day - self.exceptions_used_today).max(0) }
    pub fn cycle_progress(&self) -> f64 {
        if self.break_due_at <= 0 {
            return 0.0;
        }
        (self.worked_seconds as f64 / self.break_due_at as f64).min(1.0)
    }

    // MARK: Tick

    pub fn tick(&mut self, input: &TickInput) -> Vec<ClockEvent> {
        let mut events = vec![];

        match self.phase.clone() {
            Phase::Paused { until } => {
                if until.is_some_and(|u| input.now >= u) {
                    self.reset_cycle();
                    self.phase = Phase::Working;
                    events.push(ClockEvent::Resumed);
                }
                return events;
            }
            Phase::OnBreak { remaining } => {
                if input.in_meeting {
                    // A call came in mid-break: never block a meeting. Break waits.
                    self.phase = Phase::MeetingHold;
                    events.push(ClockEvent::BreakInterruptedByMeeting);
                } else if remaining <= 1 {
                    events.extend(self.finish_break(true));
                } else {
                    self.phase = Phase::OnBreak { remaining: remaining - 1 };
                }
                return events;
            }
            Phase::MeetingHold => {
                if input.in_meeting {
                    self.worked_seconds += 1;
                } else {
                    let grace = self.prefs.post_meeting_grace_seconds;
                    self.phase = Phase::PostMeetingGrace { remaining: grace };
                    events.push(ClockEvent::MeetingEnded { grace_seconds: grace });
                }
                return events;
            }
            Phase::PostMeetingGrace { remaining } => {
                if input.in_meeting {
                    self.phase = Phase::MeetingHold;
                } else if input.idle_seconds >= self.natural_break_seconds() as f64 {
                    events.push(ClockEvent::NaturalBreak { worked_seconds: self.worked_seconds, uncounted_seconds: 0 });
                    self.reset_cycle();
                    self.phase = Phase::Away;
                } else if remaining <= 1 {
                    events.extend(self.start_break());
                } else {
                    self.phase = Phase::PostMeetingGrace { remaining: remaining - 1 };
                }
                return events;
            }
            Phase::Working | Phase::Warning | Phase::Idle | Phase::Away => {}
        }

        // Sitting silently in a call, or watching a video, is still screen time.
        let engaged = input.in_meeting || (input.watching && !input.screen_off);
        let idle = if engaged { 0.0 } else { input.idle_seconds };

        // With the screen on, no input may just mean reading or watching something, so keep
        // counting for a while. With the screen locked or asleep, nobody is looking.
        let grace = if input.screen_off { 0 } else { self.passive_grace_seconds() };
        let away_after = if input.screen_off {
            self.natural_break_seconds()
        } else {
            self.natural_break_seconds().max(grace)
        };

        if idle >= away_after as f64 {
            if self.phase != Phase::Away {
                // The passive time since your last input was really time away.
                let uncounted = self.passive_seconds.min(self.worked_seconds);
                if self.worked_seconds > 0 {
                    events.push(ClockEvent::NaturalBreak { worked_seconds: self.worked_seconds - uncounted, uncounted_seconds: uncounted });
                }
                self.reset_cycle();
                self.phase = Phase::Away;
            }
            return events;
        }
        if idle >= grace as f64 {
            if self.phase != Phase::Away {
                self.phase = Phase::Idle;
            }
            return events;
        }

        // At the screen.
        if idle >= RECENT_INPUT_SECONDS {
            self.passive_seconds += 1;
        } else {
            self.passive_seconds = 0;
        }
        self.worked_seconds += 1;
        self.phase = Phase::Working;
        let left = self.break_due_at - self.worked_seconds;

        if left <= 0 {
            if input.in_meeting {
                self.phase = Phase::MeetingHold;
                events.push(ClockEvent::DeferredForMeeting);
            } else {
                events.extend(self.start_break());
            }
        } else if left <= self.prefs.warning_seconds {
            self.phase = Phase::Warning;
            if !self.warned {
                self.warned = true;
                events.push(ClockEvent::Warning { seconds_left: left });
            }
        }
        events
    }

    /// The computer slept or the app was suspended for `seconds`. Long gaps count as a break.
    pub fn register_gap(&mut self, seconds: i64) -> Vec<ClockEvent> {
        if seconds < self.natural_break_seconds() || self.phase.is_paused() {
            return vec![];
        }
        let mut events = vec![];
        if self.phase.is_on_break() {
            events.extend(self.finish_break(true));
        } else if self.worked_seconds > 0 {
            events.push(ClockEvent::NaturalBreak { worked_seconds: self.worked_seconds, uncounted_seconds: 0 });
        }
        self.reset_cycle();
        self.phase = Phase::Away;
        events
    }

    // MARK: User actions

    pub fn start_break_now(&mut self) -> Vec<ClockEvent> {
        if self.phase.is_on_break() {
            return vec![];
        }
        self.start_break()
    }

    /// Emergency skip. Ends the break without granting extra time; a new full cycle starts.
    pub fn skip_break(&mut self) -> Vec<ClockEvent> {
        if !self.phase.is_on_break() {
            return vec![];
        }
        self.finish_break(false)
    }

    pub fn pause(&mut self, until: Option<DateTime<Local>>) {
        self.phase = Phase::Paused { until };
    }

    pub fn resume(&mut self) -> Vec<ClockEvent> {
        if !self.phase.is_paused() {
            return vec![];
        }
        self.reset_cycle();
        self.phase = Phase::Working;
        vec![ClockEvent::Resumed]
    }

    /// Longest exception that would still respect the hard cap, in whole minutes.
    pub fn max_exception_minutes(&self) -> i64 {
        let base = self.break_due_at.max(self.worked_seconds);
        ((self.hard_cap_seconds() - base) / self.minute_length).max(0)
    }

    pub fn check_exception(&self, minutes: i64) -> Option<ExceptionDenial> {
        if matches!(self.phase, Phase::Paused { .. } | Phase::Away) {
            return Some(ExceptionDenial::NotNow);
        }
        if self.exceptions_left_today() == 0 {
            return Some(ExceptionDenial::NoneLeftToday);
        }
        if minutes > self.max_exception_minutes() {
            return Some(ExceptionDenial::HardCap { max_minutes: self.max_exception_minutes() });
        }
        None
    }

    /// "I'm in the middle of something": push the break back.
    pub fn grant_exception(&mut self, minutes: i64) -> Result<Vec<ClockEvent>, ExceptionDenial> {
        if let Some(denial) = self.check_exception(minutes) {
            return Err(denial);
        }
        let mut events = vec![];
        let was_on_break = self.phase.is_on_break();
        self.break_due_at = self.break_due_at.max(self.worked_seconds) + minutes * self.minute_length;
        self.exceptions_used_today += 1;
        self.warned = false;
        self.phase = Phase::Working;
        if was_on_break {
            events.push(ClockEvent::BreakEnded { completed: false });
        }
        events.push(ClockEvent::ExceptionGranted { minutes });
        Ok(events)
    }

    // MARK: Housekeeping

    pub fn update_prefs(&mut self, new: Preferences) {
        // Only move the due point if no exception has stretched it.
        if self.break_due_at == self.work_seconds() {
            self.break_due_at = new.work_minutes * self.minute_length;
        }
        let new_break = new.break_minutes * self.minute_length;
        if let Phase::OnBreak { remaining } = self.phase {
            if new_break < remaining {
                self.phase = Phase::OnBreak { remaining: new_break };
            }
        }
        self.prefs = new;
    }

    pub fn start_new_day(&mut self) {
        self.exceptions_used_today = 0;
    }

    /// After a relaunch, today's exceptions still count against the budget.
    pub fn restore_exceptions_used(&mut self, count: i64) {
        self.exceptions_used_today = count;
    }

    // MARK: Private

    fn start_break(&mut self) -> Vec<ClockEvent> {
        self.phase = Phase::OnBreak { remaining: self.break_seconds() };
        vec![ClockEvent::BreakStarted]
    }

    fn finish_break(&mut self, completed: bool) -> Vec<ClockEvent> {
        self.reset_cycle();
        self.phase = Phase::Working;
        vec![ClockEvent::BreakEnded { completed }]
    }

    fn reset_cycle(&mut self) {
        self.worked_seconds = 0;
        self.break_due_at = self.work_seconds();
        self.warned = false;
        self.passive_seconds = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use ClockEvent::*;

    fn prefs() -> Preferences {
        Preferences {
            work_minutes: 10,
            break_minutes: 2,
            warning_seconds: 60,
            natural_break_minutes: 3,
            passive_grace_minutes: 1,
            exceptions_per_day: 2,
            hard_cap_minutes: 30,
            post_meeting_grace_seconds: 10,
            ..Preferences::default()
        }
    }

    fn input(idle: f64) -> TickInput {
        TickInput { idle_seconds: idle, ..TickInput::active() }
    }
    fn active() -> TickInput { input(0.0) }
    fn in_call() -> TickInput { TickInput { in_meeting: true, ..TickInput::active() } }

    fn run(c: &mut SessionClock, i: &TickInput, times: usize) -> Vec<ClockEvent> {
        (0..times).flat_map(|_| c.tick(i)).collect()
    }

    fn clock() -> SessionClock { SessionClock::new(prefs(), 60) }

    #[test]
    fn warns_then_starts_break_then_finishes() {
        let mut c = clock();
        assert!(run(&mut c, &active(), 539).is_empty());
        assert_eq!(c.tick(&active()), vec![Warning { seconds_left: 60 }]);
        assert_eq!(*c.phase(), Phase::Warning);
        assert_eq!(run(&mut c, &active(), 60), vec![BreakStarted]);
        assert_eq!(*c.phase(), Phase::OnBreak { remaining: 120 });
        assert_eq!(run(&mut c, &active(), 120), vec![BreakEnded { completed: true }]);
        assert_eq!(c.worked_seconds(), 0);
    }

    // MARK: No input, but still at the screen

    #[test]
    fn watching_without_touching_keeps_counting_within_grace() {
        let mut c = clock();
        run(&mut c, &active(), 100);
        // 45 s without input used to freeze the clock. Now it's passive screen time.
        c.tick(&input(45.0));
        assert_eq!(*c.phase(), Phase::Working);
        assert_eq!(c.worked_seconds(), 101);
    }

    #[test]
    fn playing_video_counts_however_long_without_input() {
        let mut c = clock();
        let watching = TickInput { idle_seconds: 3000.0, watching: true, ..TickInput::active() };
        run(&mut c, &watching, 300);
        assert_eq!(c.worked_seconds(), 300);
        assert_eq!(*c.phase(), Phase::Working);
    }

    #[test]
    fn past_grace_clock_freezes_then_away_takes_back_passive_time() {
        let mut c = clock(); // grace 1 min, natural break 3 min
        run(&mut c, &active(), 100);
        for s in 1..=59 {
            c.tick(&input(s as f64));
        }
        assert_eq!(c.worked_seconds(), 159);
        assert_eq!(*c.phase(), Phase::Working);

        c.tick(&input(60.0));
        assert_eq!(*c.phase(), Phase::Idle);
        assert_eq!(c.worked_seconds(), 159);

        // Seconds 30–59 of the silence were counted, but you were gone: give them back.
        let events = c.tick(&input(180.0));
        assert_eq!(events, vec![NaturalBreak { worked_seconds: 129, uncounted_seconds: 30 }]);
        assert_eq!(*c.phase(), Phase::Away);
        assert_eq!(c.worked_seconds(), 0);
    }

    #[test]
    fn long_grace_means_away_only_after_grace() {
        let mut p = prefs();
        p.passive_grace_minutes = 20;
        let mut c = SessionClock::new(p, 60);
        run(&mut c, &active(), 10);
        c.tick(&input(600.0)); // 10 min without input: still "watching"
        assert_eq!(*c.phase(), Phase::Working);
        let events = c.tick(&input(1200.0));
        assert!(matches!(events[..], [NaturalBreak { .. }]));
        assert_eq!(*c.phase(), Phase::Away);
    }

    #[test]
    fn locked_screen_freezes_at_once_and_counts_as_away() {
        let mut c = clock();
        run(&mut c, &active(), 100);
        let locked = |idle| TickInput { idle_seconds: idle, screen_off: true, ..TickInput::active() };
        c.tick(&locked(5.0));
        assert_eq!(*c.phase(), Phase::Idle);
        assert_eq!(c.worked_seconds(), 100);
        assert_eq!(c.tick(&locked(180.0)), vec![NaturalBreak { worked_seconds: 100, uncounted_seconds: 0 }]);
    }

    #[test]
    fn locked_screen_beats_video_keeping_display_awake() {
        let mut c = clock();
        run(&mut c, &active(), 10);
        let t = TickInput { idle_seconds: 5.0, watching: true, screen_off: true, ..TickInput::active() };
        c.tick(&t);
        assert_eq!(*c.phase(), Phase::Idle);
    }

    #[test]
    fn input_coming_back_resets_passive_tally() {
        let mut c = clock();
        for s in 0..50 {
            c.tick(&input(s as f64));
        } // 20 passive seconds
        c.tick(&active());
        c.tick(&input(60.0));
        let events = c.tick(&input(180.0));
        assert_eq!(events, vec![NaturalBreak { worked_seconds: 51, uncounted_seconds: 0 }]);
    }

    // MARK: Meetings

    #[test]
    fn meeting_defers_break_until_it_ends() {
        let mut c = clock();
        run(&mut c, &active(), 590);
        let events = run(&mut c, &in_call(), 20);
        assert!(events.contains(&DeferredForMeeting));
        assert_eq!(*c.phase(), Phase::MeetingHold);
        assert_eq!(c.tick(&active()), vec![MeetingEnded { grace_seconds: 10 }]);
        assert_eq!(run(&mut c, &active(), 10), vec![BreakStarted]);
    }

    #[test]
    fn silent_meeting_is_not_a_natural_break() {
        let mut c = clock();
        run(&mut c, &active(), 50);
        c.tick(&TickInput { idle_seconds: 600.0, in_meeting: true, ..TickInput::active() });
        assert_eq!(c.worked_seconds(), 51);
    }

    #[test]
    fn meeting_during_break_suspends_it() {
        let mut c = clock();
        c.start_break_now();
        assert_eq!(c.tick(&in_call()), vec![BreakInterruptedByMeeting]);
        assert_eq!(*c.phase(), Phase::MeetingHold);
    }

    #[test]
    fn walking_away_after_meeting_counts_as_the_break() {
        let mut c = clock();
        run(&mut c, &active(), 599);
        c.tick(&in_call());
        assert_eq!(c.tick(&active()), vec![MeetingEnded { grace_seconds: 10 }]);
        let events = c.tick(&input(200.0));
        assert_eq!(events, vec![NaturalBreak { worked_seconds: 600, uncounted_seconds: 0 }]);
        assert_eq!(*c.phase(), Phase::Away);
    }

    #[test]
    fn new_call_during_grace_goes_back_to_hold() {
        let mut c = clock();
        run(&mut c, &active(), 599);
        c.tick(&in_call());
        c.tick(&active());
        assert_eq!(c.tick(&in_call()), vec![]);
        assert_eq!(*c.phase(), Phase::MeetingHold);
    }

    // MARK: Exceptions

    #[test]
    fn exception_during_break_ends_it_and_pushes_due() {
        let mut c = clock();
        run(&mut c, &active(), 600);
        assert!(c.phase().is_on_break());
        let events = c.grant_exception(5).unwrap();
        assert_eq!(events, vec![BreakEnded { completed: false }, ExceptionGranted { minutes: 5 }]);
        assert_eq!(c.seconds_until_break(), 300);
        assert_eq!(c.exceptions_left_today(), 1);
    }

    #[test]
    fn exception_budget_and_hard_cap() {
        let mut c = clock();
        assert_eq!(c.check_exception(25), Some(ExceptionDenial::HardCap { max_minutes: 20 }));
        c.grant_exception(15).unwrap();
        c.grant_exception(5).unwrap();
        assert_eq!(c.check_exception(5), Some(ExceptionDenial::NoneLeftToday));
        c.start_new_day();
        assert_eq!(c.check_exception(5), Some(ExceptionDenial::HardCap { max_minutes: 0 }));
    }

    #[test]
    fn exception_during_meeting_hold_goes_back_to_work() {
        let mut c = clock();
        run(&mut c, &active(), 599);
        assert_eq!(c.tick(&in_call()), vec![DeferredForMeeting]);
        assert_eq!(c.grant_exception(5).unwrap(), vec![ExceptionGranted { minutes: 5 }]);
        assert_eq!(*c.phase(), Phase::Working);
        assert_eq!(c.seconds_until_break(), 300);
    }

    #[test]
    fn no_exceptions_while_paused_or_away() {
        let mut c = clock();
        c.pause(None);
        assert_eq!(c.check_exception(5), Some(ExceptionDenial::NotNow));
        c.resume();
        c.tick(&input(200.0));
        assert_eq!(*c.phase(), Phase::Away);
        assert!(c.grant_exception(5).is_err());
    }

    #[test]
    fn warning_comes_again_after_an_exception() {
        let mut c = clock();
        run(&mut c, &active(), 540);
        c.grant_exception(5).unwrap(); // due 600 → 900
        assert_eq!(run(&mut c, &active(), 299), vec![]);
        assert_eq!(c.tick(&active()), vec![Warning { seconds_left: 60 }]);
    }

    #[test]
    fn exceptions_used_survive_relaunch() {
        let mut c = clock();
        c.restore_exceptions_used(2);
        assert_eq!(c.exceptions_left_today(), 0);
        assert_eq!(c.check_exception(5), Some(ExceptionDenial::NoneLeftToday));
    }

    // MARK: Breaks, sleep, pause

    #[test]
    fn idle_during_warning_does_not_warn_twice() {
        let mut c = clock();
        run(&mut c, &active(), 540);
        c.tick(&input(70.0));
        assert_eq!(*c.phase(), Phase::Idle);
        assert_eq!(c.tick(&active()), vec![]);
        assert_eq!(*c.phase(), Phase::Warning);
    }

    #[test]
    fn emergency_skip_ends_break_as_skipped() {
        let mut c = clock();
        run(&mut c, &active(), 200);
        assert_eq!(c.start_break_now(), vec![BreakStarted]);
        assert_eq!(c.start_break_now(), vec![], "starting a break during a break does nothing");
        assert_eq!(c.skip_break(), vec![BreakEnded { completed: false }]);
        assert_eq!(*c.phase(), Phase::Working);
        assert_eq!(c.worked_seconds(), 0);
        assert_eq!(c.skip_break(), vec![], "nothing to skip outside a break");
    }

    #[test]
    fn sleep_gap_counts_as_break() {
        let mut c = clock();
        run(&mut c, &active(), 300);
        assert_eq!(c.register_gap(30), vec![]);
        assert_eq!(c.register_gap(600), vec![NaturalBreak { worked_seconds: 300, uncounted_seconds: 0 }]);
        assert_eq!(c.worked_seconds(), 0);
    }

    #[test]
    fn sleeping_through_a_break_completes_it() {
        let mut c = clock();
        c.start_break_now();
        assert_eq!(c.register_gap(600), vec![BreakEnded { completed: true }]);
        assert_eq!(*c.phase(), Phase::Away);
    }

    #[test]
    fn sleep_while_paused_is_ignored() {
        let mut c = clock();
        c.pause(None);
        assert_eq!(c.register_gap(600), vec![]);
        assert_eq!(*c.phase(), Phase::Paused { until: None });
        assert_eq!(c.resume(), vec![Resumed]);
        assert_eq!(c.resume(), vec![], "resume only works when paused");
    }

    #[test]
    fn pause_until_date_resumes() {
        let mut c = clock();
        let now = Local::now();
        c.pause(Some(now + Duration::seconds(60)));
        assert_eq!(c.tick(&TickInput { now, ..active() }), vec![]);
        assert_eq!(c.tick(&TickInput { now: now + Duration::seconds(61), ..active() }), vec![Resumed]);
        assert_eq!(*c.phase(), Phase::Working);
    }

    #[test]
    fn changing_work_length_respects_exceptions() {
        let mut c = clock();
        let mut p = prefs();
        p.work_minutes = 20;
        c.update_prefs(p.clone());
        assert_eq!(c.seconds_until_break(), 1200);
        c.grant_exception(5).unwrap(); // due 1500
        p.work_minutes = 15;
        c.update_prefs(p);
        assert_eq!(c.seconds_until_break(), 1500, "an exception the user asked for isn't undone");
    }

    #[test]
    fn shortening_break_length_shortens_running_break() {
        let mut c = clock();
        c.start_break_now();
        let mut p = prefs();
        p.break_minutes = 1;
        c.update_prefs(p);
        assert_eq!(*c.phase(), Phase::OnBreak { remaining: 60 });
    }

    #[test]
    fn smoke_test_minute_length_scales_the_whole_cycle() {
        let mut c = SessionClock::new(prefs(), 1);
        assert_eq!(c.seconds_until_break(), 10);
        assert_eq!(c.max_exception_minutes(), 20);
        assert_eq!(run(&mut c, &active(), 10), vec![Warning { seconds_left: 9 }, BreakStarted]);
        assert_eq!(*c.phase(), Phase::OnBreak { remaining: 2 });
        assert_eq!(run(&mut c, &active(), 2), vec![BreakEnded { completed: true }]);
    }

    #[test]
    fn full_day_scenario() {
        let mut c = clock();
        let mut all = vec![];
        all.extend(run(&mut c, &active(), 600)); // warning + break
        all.extend(run(&mut c, &active(), 120)); // break completes
        all.extend(run(&mut c, &active(), 545)); // warning in cycle 2
        all.extend(c.grant_exception(5).unwrap());
        all.extend(run(&mut c, &active(), 354)); // one second before the new due point (900)
        all.extend(run(&mut c, &in_call(), 5)); // due while on a call → deferred
        all.extend(run(&mut c, &active(), 11)); // call ends, 10 s grace, break
        assert_eq!(
            all,
            vec![
                Warning { seconds_left: 60 },
                BreakStarted,
                BreakEnded { completed: true },
                Warning { seconds_left: 60 },
                ExceptionGranted { minutes: 5 },
                Warning { seconds_left: 60 },
                DeferredForMeeting,
                MeetingEnded { grace_seconds: 10 },
                BreakStarted,
            ]
        );
        assert_eq!(c.exceptions_left_today(), 1);
    }
}
