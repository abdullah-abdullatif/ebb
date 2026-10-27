use crate::prefs::{Nudge, Preferences};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NudgeKind {
    Blink,
    Posture,
    Water,
    Stretch,
}

impl NudgeKind {
    pub const ALL: [NudgeKind; 4] = [NudgeKind::Blink, NudgeKind::Posture, NudgeKind::Water, NudgeKind::Stretch];

    fn setting(self, p: &Preferences) -> &Nudge {
        match self {
            NudgeKind::Blink => &p.blink,
            NudgeKind::Posture => &p.posture,
            NudgeKind::Water => &p.water,
            NudgeKind::Stretch => &p.stretch,
        }
    }

    pub fn message(self) -> (&'static str, &'static str) {
        match self {
            NudgeKind::Blink => ("Blink", "Close your eyes slowly, ten times. Staring at a screen halves how often you blink."),
            NudgeKind::Posture => ("Sit tall", "Shoulders down and back, feet flat, screen at eye level."),
            NudgeKind::Water => ("Drink some water", "A few sips now keeps headaches and tiredness away."),
            NudgeKind::Stretch => ("Stretch", "Stand up, reach for the ceiling, then roll your neck gently."),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WellnessEvent {
    EyeBreakStarted { seconds: i64 },
    EyeBreakEnded { completed: bool },
    Nudge { nudge: NudgeKind },
}

/// Never show two reminders closer together than this, in real seconds.
const QUIET_SECONDS: i64 = 90;

/// Small, frequent care between the big breaks: 20-20-20 eye breaks and gentle nudges.
/// Pure like `SessionClock`: one `tick` per second.
#[derive(Clone, Debug)]
pub struct Wellness {
    minute_length: i64,
    eye_counter: i64,
    nudge_counters: [i64; 4],
    since_last_reminder: i64,
    eye_break_remaining: Option<i64>,
}

impl Wellness {
    pub fn new(minute_length: i64) -> Self {
        Self {
            minute_length,
            eye_counter: 0,
            nudge_counters: [0; 4],
            since_last_reminder: QUIET_SECONDS,
            eye_break_remaining: None,
        }
    }

    pub fn eye_break_remaining(&self) -> Option<i64> {
        self.eye_break_remaining
    }

    /// Seconds of screen time until the next eye break.
    pub fn seconds_until_eye_break(&self, p: &Preferences) -> i64 {
        (p.eye_break_every_minutes * self.minute_length - self.eye_counter).max(0)
    }

    /// `at_screen`: the work clock counted this second.
    /// `can_interrupt`: nothing more important is going on (no meeting, no break soon).
    pub fn tick(&mut self, p: &Preferences, at_screen: bool, can_interrupt: bool) -> Vec<WellnessEvent> {
        let mut events = vec![];
        self.since_last_reminder += 1;

        if let Some(r) = self.eye_break_remaining {
            if !can_interrupt {
                // A call started or a big break began: drop the eye break, it's covered.
                self.eye_break_remaining = None;
                events.push(WellnessEvent::EyeBreakEnded { completed: false });
            } else if r <= 1 {
                self.eye_break_remaining = None;
                events.push(WellnessEvent::EyeBreakEnded { completed: true });
            } else {
                self.eye_break_remaining = Some(r - 1);
            }
            return events;
        }

        if !at_screen {
            return events;
        }
        self.eye_counter += 1;
        for (i, kind) in NudgeKind::ALL.iter().enumerate() {
            if kind.setting(p).enabled {
                self.nudge_counters[i] += 1;
            }
        }
        if !can_interrupt || self.since_last_reminder < QUIET_SECONDS.min(self.minute_length * 2) {
            return events;
        }

        if p.eye_breaks_enabled && self.eye_counter >= p.eye_break_every_minutes * self.minute_length {
            self.eye_counter = 0;
            self.since_last_reminder = 0;
            // Looking far away also rests you from blinking less.
            self.nudge_counters[0] = 0;
            self.eye_break_remaining = Some(p.eye_break_seconds);
            events.push(WellnessEvent::EyeBreakStarted { seconds: p.eye_break_seconds });
            return events;
        }

        // At most one nudge at a time; the most overdue goes first.
        let due = NudgeKind::ALL
            .iter()
            .enumerate()
            .filter(|(_, k)| k.setting(p).enabled)
            .map(|(i, k)| (i, *k, self.nudge_counters[i] - k.setting(p).every_minutes * self.minute_length))
            .filter(|(_, _, over)| *over >= 0)
            .max_by_key(|(i, _, over)| (*over, std::cmp::Reverse(*i)));
        if let Some((i, kind, _)) = due {
            self.nudge_counters[i] = 0;
            self.since_last_reminder = 0;
            events.push(WellnessEvent::Nudge { nudge: kind });
        }
        events
    }

    pub fn skip_eye_break(&mut self) -> Vec<WellnessEvent> {
        if self.eye_break_remaining.take().is_some() {
            vec![WellnessEvent::EyeBreakEnded { completed: false }]
        } else {
            vec![]
        }
    }

    /// After a real break (taken or natural), eyes and body are rested: start the counts again.
    pub fn rested(&mut self) {
        self.eye_counter = 0;
        self.nudge_counters = [0; 4];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use WellnessEvent::*;

    fn prefs() -> Preferences {
        let mut p = Preferences::default();
        p.eye_break_every_minutes = 20;
        p.eye_break_seconds = 20;
        p.blink = Nudge { enabled: false, every_minutes: 15 };
        p.posture = Nudge { enabled: false, every_minutes: 30 };
        p.water = Nudge { enabled: false, every_minutes: 45 };
        p.stretch = Nudge { enabled: false, every_minutes: 60 };
        p
    }

    fn run(w: &mut Wellness, p: &Preferences, n: usize, at_screen: bool, ok: bool) -> Vec<WellnessEvent> {
        (0..n).flat_map(|_| w.tick(p, at_screen, ok)).collect()
    }

    #[test]
    fn eye_break_every_twenty_minutes_of_screen_time() {
        let p = prefs();
        let mut w = Wellness::new(60);
        assert!(run(&mut w, &p, 1199, true, true).is_empty());
        assert_eq!(w.tick(&p, true, true), vec![EyeBreakStarted { seconds: 20 }]);
        assert_eq!(w.eye_break_remaining(), Some(20));
        assert_eq!(run(&mut w, &p, 20, true, true), vec![EyeBreakEnded { completed: true }]);
        assert_eq!(w.eye_break_remaining(), None);
    }

    #[test]
    fn time_away_from_screen_does_not_count() {
        let p = prefs();
        let mut w = Wellness::new(60);
        run(&mut w, &p, 1000, false, true);
        assert_eq!(w.seconds_until_eye_break(&p), 1200);
    }

    #[test]
    fn waits_while_it_cannot_interrupt_then_fires() {
        let p = prefs();
        let mut w = Wellness::new(60);
        assert!(run(&mut w, &p, 1300, true, false).is_empty(), "no eye break during a meeting");
        assert_eq!(w.tick(&p, true, true), vec![EyeBreakStarted { seconds: 20 }]);
    }

    #[test]
    fn meeting_mid_eye_break_cancels_it() {
        let p = prefs();
        let mut w = Wellness::new(60);
        run(&mut w, &p, 1200, true, true);
        assert_eq!(w.tick(&p, true, false), vec![EyeBreakEnded { completed: false }]);
    }

    #[test]
    fn skip_and_rest_reset_things() {
        let p = prefs();
        let mut w = Wellness::new(60);
        assert!(w.skip_eye_break().is_empty());
        run(&mut w, &p, 1200, true, true);
        assert_eq!(w.skip_eye_break(), vec![EyeBreakEnded { completed: false }]);
        run(&mut w, &p, 600, true, true);
        w.rested();
        assert_eq!(w.seconds_until_eye_break(&p), 1200);
    }

    #[test]
    fn nudges_fire_on_their_own_intervals_one_at_a_time() {
        let mut p = prefs();
        p.eye_breaks_enabled = false;
        p.water = Nudge { enabled: true, every_minutes: 2 };
        p.posture = Nudge { enabled: true, every_minutes: 2 };
        let mut w = Wellness::new(60);
        let events = run(&mut w, &p, 120, true, true);
        assert_eq!(events, vec![Nudge { nudge: NudgeKind::Posture }]);
        // Water was due at the same time but waits for the quiet gap.
        let events = run(&mut w, &p, 90, true, true);
        assert_eq!(events, vec![Nudge { nudge: NudgeKind::Water }]);
    }

    #[test]
    fn disabled_nudges_never_fire() {
        let mut p = prefs();
        p.eye_breaks_enabled = false;
        let mut w = Wellness::new(60);
        assert!(run(&mut w, &p, 4 * 3600, true, true).is_empty());
    }
}
