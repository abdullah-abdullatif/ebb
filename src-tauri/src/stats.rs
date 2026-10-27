use crate::prefs::Preferences;
use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionRecord {
    #[serde(default = "uuid::Uuid::new_v4")]
    pub id: uuid::Uuid,
    pub date: DateTime<Utc>,
    pub minutes: i64,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub category: String,
}

/// How you said you felt on a break. 1 = drained … 5 = great.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoodRecord {
    pub date: DateTime<Utc>,
    pub mood: u8,
}

/// One local calendar day. Field names match the original macOS app's stats.json,
/// so history carries over.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DayStats {
    pub day: String,
    pub active_seconds: i64,
    pub meeting_seconds: i64,
    pub breaks_completed: i64,
    pub breaks_skipped: i64,
    pub natural_breaks: i64,
    pub meeting_deferrals: i64,
    pub longest_stretch_seconds: i64,
    pub exceptions: Vec<ExceptionRecord>,
    // Added with the wellness features.
    pub passive_seconds: i64,
    pub eye_breaks_taken: i64,
    pub eye_breaks_skipped: i64,
    pub nudges: i64,
    pub late_night_seconds: i64,
    pub moods: Vec<MoodRecord>,
}

impl Default for DayStats {
    fn default() -> Self {
        Self::new(String::new())
    }
}

impl DayStats {
    pub fn new(day: String) -> Self {
        Self {
            day,
            active_seconds: 0,
            meeting_seconds: 0,
            breaks_completed: 0,
            breaks_skipped: 0,
            natural_breaks: 0,
            meeting_deferrals: 0,
            longest_stretch_seconds: 0,
            exceptions: vec![],
            passive_seconds: 0,
            eye_breaks_taken: 0,
            eye_breaks_skipped: 0,
            nudges: 0,
            late_night_seconds: 0,
            moods: vec![],
        }
    }

    pub fn breaks_taken(&self) -> i64 {
        self.breaks_completed + self.natural_breaks
    }

    pub fn average_mood(&self) -> Option<f64> {
        average(self.moods.iter().map(|m| m.mood as f64))
    }

    /// 0–100 for how kind the day was to your eyes and mind, with the parts that make it up.
    pub fn wellness(&self, p: &Preferences) -> Wellbeing {
        if self.active_seconds == 0 {
            return Wellbeing { score: 0, parts: vec![] };
        }
        let ratio = |good: i64, bad: i64| if good + bad == 0 { 1.0 } else { good as f64 / (good + bad) as f64 };
        // 1.0 at or below `ok`, falling linearly to 0.0 at `zero`.
        let falloff = |value: f64, ok: f64, zero: f64| {
            if value <= ok { 1.0 } else { (1.0 - (value - ok) / (zero - ok).max(1.0)).max(0.0) }
        };

        let work = (p.work_minutes * 60) as f64;
        let limit_minutes = if p.daily_limit_enabled { p.daily_limit_minutes } else { 8 * 60 };
        let limit = limit_minutes as f64 * 60.0;
        let parts = vec![
            Part::new("Breaks taken", 35.0, ratio(self.breaks_taken(), self.breaks_skipped)),
            Part::new("Longest stretch", 20.0, falloff(self.longest_stretch_seconds as f64, work + 600.0, work * 2.5)),
            Part::new("Eye breaks", 15.0, ratio(self.eye_breaks_taken, self.eye_breaks_skipped)),
            Part::new("Screen time", 15.0, falloff(self.active_seconds as f64, limit, limit + 4.0 * 3600.0)),
            Part::new("Late nights", 15.0, falloff(self.late_night_seconds as f64, 0.0, 3600.0)),
        ];
        let penalty = self.exceptions.len() as f64 * 3.0;
        let total: f64 = parts.iter().map(|x| x.points).sum::<f64>() - penalty;
        Wellbeing { score: total.round().clamp(0.0, 100.0) as i64, parts }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub label: &'static str,
    pub points: f64,
    pub max: f64,
}

impl Part {
    fn new(label: &'static str, max: f64, fraction: f64) -> Self {
        Self { label, max, points: (max * fraction.clamp(0.0, 1.0) * 10.0).round() / 10.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Wellbeing {
    pub score: i64,
    pub parts: Vec<Part>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySummary {
    pub day: String,
    pub weekday: String,
    pub active_seconds: i64,
    pub score: i64,
    pub mood: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekReport {
    /// Oldest first, always 7 entries ending today.
    pub days: Vec<DaySummary>,
    pub total_seconds: i64,
    pub average_seconds: i64,
    pub average_score: Option<i64>,
    pub previous_average_score: Option<i64>,
    pub breaks_taken: i64,
    pub breaks_skipped: i64,
    pub eye_breaks_taken: i64,
    pub eye_breaks_skipped: i64,
    pub late_night_seconds: i64,
    pub exceptions: i64,
    pub longest_stretch_seconds: i64,
    pub average_mood: Option<f64>,
    pub tips: Vec<String>,
}

fn average(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, n) = values.fold((0.0, 0), |(s, n), v| (s + v, n + 1));
    (n > 0).then(|| sum / n as f64)
}

pub fn day_key(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

pub fn today_key() -> String {
    day_key(Local::now().date_naive())
}

/// Daily history in stats.json.
pub struct StatsStore {
    path: PathBuf,
    pub days: BTreeMap<String, DayStats>,
    dirty: bool,
}

impl StatsStore {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("stats.json");
        let days = std::fs::read(&path)
            .ok()
            .and_then(|d| serde_json::from_slice(&d).ok())
            .unwrap_or_default();
        Self { path, days, dirty: false }
    }

    /// Bring over history from the original macOS app the first time the new one runs.
    pub fn import_legacy(&mut self, legacy: &Path) {
        if !self.days.is_empty() {
            return;
        }
        if let Some(days) = std::fs::read(legacy).ok().and_then(|d| serde_json::from_slice(&d).ok()) {
            self.days = days;
            self.dirty = true;
            self.save_if_needed();
        }
    }

    pub fn today(&self) -> DayStats {
        let k = today_key();
        self.days.get(&k).cloned().unwrap_or_else(|| DayStats::new(k))
    }

    /// Most recent first.
    pub fn history(&self, limit: usize) -> Vec<DayStats> {
        self.days.values().rev().take(limit).cloned().collect()
    }

    pub fn update(&mut self, change: impl FnOnce(&mut DayStats)) {
        let k = today_key();
        let d = self.days.entry(k.clone()).or_insert_with(|| DayStats::new(k));
        change(d);
        self.dirty = true;
    }

    pub fn save_if_needed(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        if let Ok(data) = serde_json::to_vec_pretty(&self.days) {
            write_atomic(&self.path, &data);
        }
    }

    pub fn week_report(&self, p: &Preferences, today: NaiveDate) -> WeekReport {
        week_report(&self.days, p, today)
    }
}

pub fn week_report(all: &BTreeMap<String, DayStats>, p: &Preferences, today: NaiveDate) -> WeekReport {
    let get = |d: NaiveDate| all.get(&day_key(d)).cloned().unwrap_or_else(|| DayStats::new(day_key(d)));
    let week: Vec<DayStats> = (0..7).rev().map(|i| get(today - Duration::days(i))).collect();
    let previous: Vec<DayStats> = (7..14).rev().map(|i| get(today - Duration::days(i))).collect();

    let avg_score = |days: &[DayStats]| {
        average(days.iter().filter(|d| d.active_seconds > 0).map(|d| d.wellness(p).score as f64)).map(|s| s.round() as i64)
    };
    let sum = |f: fn(&DayStats) -> i64| week.iter().map(f).sum::<i64>();
    let used_days = week.iter().filter(|d| d.active_seconds > 0).count().max(1) as i64;

    let mut r = WeekReport {
        days: week
            .iter()
            .map(|d| DaySummary {
                weekday: NaiveDate::parse_from_str(&d.day, "%Y-%m-%d").map(|n| n.format("%a").to_string()).unwrap_or_default(),
                day: d.day.clone(),
                active_seconds: d.active_seconds,
                score: d.wellness(p).score,
                mood: d.average_mood(),
            })
            .collect(),
        total_seconds: sum(|d| d.active_seconds),
        average_seconds: sum(|d| d.active_seconds) / used_days,
        average_score: avg_score(&week),
        previous_average_score: avg_score(&previous),
        breaks_taken: sum(|d| d.breaks_taken()),
        breaks_skipped: sum(|d| d.breaks_skipped),
        eye_breaks_taken: sum(|d| d.eye_breaks_taken),
        eye_breaks_skipped: sum(|d| d.eye_breaks_skipped),
        late_night_seconds: sum(|d| d.late_night_seconds),
        exceptions: sum(|d| d.exceptions.len() as i64),
        longest_stretch_seconds: week.iter().map(|d| d.longest_stretch_seconds).max().unwrap_or(0),
        average_mood: average(week.iter().flat_map(|d| d.moods.iter().map(|m| m.mood as f64))),
        tips: vec![],
    };
    r.tips = tips(&r, p);
    r
}

/// Plain-language suggestions from the week's numbers. The most important come first.
fn tips(r: &WeekReport, p: &Preferences) -> Vec<String> {
    let mut t = vec![];
    if r.total_seconds == 0 {
        return vec!["Use your computer as usual. Your first weekly report builds up over the next few days.".into()];
    }
    if r.breaks_skipped > 0 && r.breaks_skipped * 3 >= r.breaks_taken {
        t.push(format!(
            "You skipped {} of {} breaks. Try a shorter break length so it's easier to say yes.",
            r.breaks_skipped,
            r.breaks_taken + r.breaks_skipped
        ));
    }
    if r.longest_stretch_seconds > (p.work_minutes * 60 + 1800) {
        t.push(format!(
            "Your longest stretch was {} without a break. Long stretches strain eyes and focus; keep exceptions for real emergencies.",
            hm(r.longest_stretch_seconds)
        ));
    }
    if r.late_night_seconds > 1800 {
        t.push(format!(
            "{} of screen time this week was late at night. Screens before bed delay sleep; try turning on wind-down.",
            hm(r.late_night_seconds)
        ));
    }
    if r.eye_breaks_skipped > r.eye_breaks_taken {
        t.push("You skipped most eye breaks. Twenty seconds of looking out a window really does reduce eye strain.".into());
    }
    let limit = if p.daily_limit_enabled { p.daily_limit_minutes * 60 } else { 8 * 3600 };
    if r.average_seconds > limit {
        t.push(format!("You average {} a day at the screen. Can one task move away from the computer?", hm(r.average_seconds)));
    }
    if r.exceptions >= 7 {
        t.push(format!("You asked for more time {} times. If breaks keep landing at bad moments, a longer work cycle may fit better.", r.exceptions));
    }
    if let Some(m) = r.average_mood {
        if m < 2.75 {
            t.push("You've been feeling low on breaks. Step outside, move, or talk to someone. If it lasts, reach out to someone you trust.".into());
        }
    }
    if let (Some(now), Some(before)) = (r.average_score, r.previous_average_score) {
        if now >= before + 5 {
            t.push(format!("Your wellness score is up {} points on last week. Keep it going.", now - before));
        } else if now + 5 <= before {
            t.push(format!("Your wellness score is down {} points on last week.", before - now));
        }
    }
    if t.is_empty() {
        t.push("A balanced week: regular breaks and healthy stretches. Keep it up.".into());
    }
    t
}

fn hm(seconds: i64) -> String {
    let (h, m) = (seconds / 3600, (seconds % 3600) / 60);
    if h > 0 { format!("{h} h {m:02} min") } else { format!("{m} min") }
}

/// Write via a temp file so a crash never leaves half a file behind.
pub fn write_atomic(path: &Path, data: &[u8]) {
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, data).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::tempdir;

    fn day(active_h: f64) -> DayStats {
        let mut d = DayStats::new("2026-10-20".into());
        d.active_seconds = (active_h * 3600.0) as i64;
        d.longest_stretch_seconds = 45 * 60;
        d
    }

    #[test]
    fn reads_the_original_mac_app_format() {
        let json = r#"{"2026-10-08":{"activeSeconds":3600,"breaksCompleted":2,"breaksSkipped":1,"day":"2026-10-08",
            "exceptions":[{"category":"Deadline","date":"2026-10-08T12:00:00Z","id":"6F9619FF-8B86-D011-B42D-00C04FC964FF","minutes":10,"reason":"ship it"}],
            "longestStretchSeconds":3000,"meetingDeferrals":0,"meetingSeconds":0,"naturalBreaks":1}}"#;
        let days: BTreeMap<String, DayStats> = serde_json::from_str(json).unwrap();
        let d = &days["2026-10-08"];
        assert_eq!(d.breaks_taken(), 3);
        assert_eq!(d.exceptions[0].reason, "ship it");
        assert_eq!(d.eye_breaks_taken, 0);
    }

    #[test]
    fn legacy_import_only_when_empty() {
        let dir = tempdir();
        let legacy = dir.join("legacy.json");
        std::fs::write(&legacy, r#"{"2026-10-01":{"day":"2026-10-01","activeSeconds":60}}"#).unwrap();
        let mut s = StatsStore::load(&dir);
        s.import_legacy(&legacy);
        assert_eq!(s.days.len(), 1);
        assert_eq!(StatsStore::load(&dir).days.len(), 1, "import is saved");

        std::fs::write(&legacy, r#"{"2026-10-02":{"day":"2026-10-02"}}"#).unwrap();
        s.import_legacy(&legacy);
        assert!(s.days.contains_key("2026-10-01"));
        assert!(!s.days.contains_key("2026-10-02"));
    }

    #[test]
    fn update_save_load_round_trip() {
        let dir = tempdir();
        let mut s = StatsStore::load(&dir);
        s.update(|d| {
            d.active_seconds += 5;
            d.moods.push(MoodRecord { date: Utc::now(), mood: 4 });
        });
        s.save_if_needed();
        let again = StatsStore::load(&dir);
        assert_eq!(again.today().active_seconds, 5);
        assert_eq!(again.today().average_mood(), Some(4.0));
        assert_eq!(again.history(5).len(), 1);
    }

    #[test]
    fn perfect_day_scores_100() {
        let mut d = day(6.0);
        d.breaks_completed = 6;
        d.eye_breaks_taken = 10;
        assert_eq!(d.wellness(&Preferences::default()).score, 100);
    }

    #[test]
    fn empty_day_scores_zero() {
        assert_eq!(DayStats::new("x".into()).wellness(&Preferences::default()).score, 0);
    }

    #[test]
    fn skipping_and_late_nights_cost_points() {
        let p = Preferences::default();
        let mut d = day(6.0);
        d.breaks_completed = 2;
        d.breaks_skipped = 2;
        assert_eq!(d.wellness(&p).score, 83); // half of 35 lost, rounded
        d.late_night_seconds = 3600;
        assert_eq!(d.wellness(&p).score, 68);
        d.exceptions.push(ExceptionRecord { id: uuid::Uuid::new_v4(), date: Utc::now(), minutes: 5, reason: "".into(), category: "".into() });
        assert_eq!(d.wellness(&p).score, 65);
    }

    #[test]
    fn long_stretch_and_long_days_cost_points() {
        let p = Preferences::default(); // work 50 min
        let mut d = day(14.0);
        d.longest_stretch_seconds = 125 * 60; // past 2.5 × 50 min
        let w = d.wellness(&p);
        assert_eq!(w.parts[1].points, 0.0);
        assert_eq!(w.parts[3].points, 0.0, "14 h is 6 h past the 8 h default");
        assert_eq!(w.score, 65);
    }

    #[test]
    fn weekly_report_compares_with_last_week_and_gives_tips() {
        let p = Preferences::default();
        let today = NaiveDate::from_ymd_opt(2026, 10, 27).unwrap();
        let mut all = BTreeMap::new();
        for i in 0..14 {
            let date = today - Duration::days(i);
            let mut d = day(4.0);
            d.day = day_key(date);
            if i < 7 {
                d.breaks_completed = 4;
                d.late_night_seconds = 600;
                d.moods.push(MoodRecord { date: Utc::now(), mood: 2 });
            } else {
                d.breaks_completed = 1;
                d.breaks_skipped = 3;
            }
            all.insert(d.day.clone(), d);
        }
        let r = week_report(&all, &p, today);
        assert_eq!(r.days.len(), 7);
        assert_eq!(r.days[6].day, "2026-10-27");
        assert_eq!(r.days[6].weekday, "Tue");
        assert_eq!(r.total_seconds, 7 * 4 * 3600);
        assert_eq!(r.average_seconds, 4 * 3600);
        assert_eq!(r.late_night_seconds, 4200);
        assert!(r.average_score.unwrap() > r.previous_average_score.unwrap());
        assert!(r.tips.iter().any(|t| t.contains("late at night")));
        assert!(r.tips.iter().any(|t| t.contains("feeling low")));
        assert!(r.tips.iter().any(|t| t.contains("up")));
    }

    #[test]
    fn empty_week_has_a_welcome_tip() {
        let r = week_report(&BTreeMap::new(), &Preferences::default(), NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        assert_eq!(r.average_score, None);
        assert_eq!(r.tips.len(), 1);
    }
}
