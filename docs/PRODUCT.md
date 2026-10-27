# Ebb — product & system design

> **Work in flow, rest in ebb.**
> A menu-bar / system-tray app for macOS, Windows and Linux that tracks how long you've really been at the screen, makes you take a
> break when it's time, lets you ask for more time when you're deep in something, and never
> interrupts a meeting.

## 1. Name & icon

**Ebb**: the tide going out. Work comes in like the tide (flow), and rest is the ebb.
It's short, easy to say, and doesn't sound like a stopwatch or a nag.

The icon is a warm sun setting into two teal waves under a night sky. The waves are flow and
ebb, and the setting sun means *wind down*. Source: `Resources/AppIcon.svg`; rendered:
`Resources/AppIcon.png` (1024²). Inside the app the same palette is in `ui/style.css`; the tray icon is drawn in code (`waves_icon` in `src-tauri/src/lib.rs`).

Other names considered: *Tidewell*, *Respite*, *Recess*, *Undertow*, *Lull*.

## 2. Core loop

```
            active input                       break due, no meeting
 ┌─────────┐ ───────────▶ ┌─────────┐  T-2min  ┌─────────┐ ─────────▶ ┌──────────┐
 │  idle   │              │ working │ ───────▶ │ warning │            │ on break │
 └─────────┘ ◀─────────── └─────────┘          └─────────┘            └──────────┘
     │ no input ≥ grace (15m)  │  ▲                  │ break due,        │  ▲  │
     │                         │  │ exception        │ in meeting        │  │  │ timer done
     │ away ≥ max(grace, 5m)   │  └──────────────────┤                   │  │  ▼
     ▼                         │                     ▼                   │  │ fresh cycle
 ┌─────────┐                   │              ┌──────────────┐  meeting │  │
 │  away   │ (natural break —  │              │ meeting hold │ ◀────────┘  │
 └─────────┘  cycle resets)    │              └──────────────┘ starts      │
                               │                     │ meeting ends        │
                               │                     ▼                     │
                               │              ┌──────────────┐  grace up   │
                               │              │ post-meeting │ ────────────┘
                               │              │    grace     │
                               │              └──────────────┘
```

All of this lives in one pure struct, `SessionClock` in `src-tauri/src/clock.rs`. It doesn't
know about timers, windows or the OS. It takes one `tick` per second and returns events, which
keeps it fully unit-tested (`cargo test`).

### What counts as "work"

Being at the screen doesn't mean touching it: people watch videos, read, or watch a long job
run. Version 0.1 froze the clock after 60 s without input, so that time went uncounted. Now:

| Signal | Effect |
|---|---|
| Keyboard/mouse input | Work clock runs |
| A video is playing (an app keeps the display awake) | Clock runs, however long you watch (toggle: *Playing videos always count*) |
| In a meeting (even with no input) | Clock runs; never a break |
| No input, screen on, less than the **grace period** (default 15 min) | Clock **keeps running** (reading, watching) |
| No input for longer than the grace period | Clock freezes |
| No input for ≥ max(grace, 5 min) | **Natural break**: the cycle resets. The silent seconds already counted (everything after the first 30 s without input) are **taken back off** today's screen time, because you'd actually left |
| Screen locked, screensaver, or display asleep | Clock freezes **at once**; away 5 min → natural break |
| Computer slept ≥ 5 min | Natural break |

Signals per OS (none need special permissions):

| | macOS | Windows | Linux |
|---|---|---|---|
| Idle time | HID idle counter (`CGEventSource`) | `GetLastInputInfo` | GNOME Mutter / KDE over D-Bus, X11 screensaver ext. |
| Locked / off | `CGSessionCopyCurrentDictionary`, `CGDisplayIsAsleep` | input desktop not openable, screensaver running | screensaver `GetActive`, logind `LockedHint` |
| Video playing | `PreventUserIdleDisplaySleep` power assertion | `ES_DISPLAY_REQUIRED` execution state | GNOME idle inhibitor, MPRIS video players |

## 3. The break

* Full-screen overlay on **every display**. On macOS it sits on every Space, above full-screen apps
  (screen-saver window level); on Linux it goes full-screen to cover panels.
* It shows a breathing guide (4 s in, 6 s out), a countdown, and a new wellbeing tip every 30 s.
* **Normal mode** has an emergency *hold-to-skip* button (3 s hold). A skip is recorded as a skipped
  break.
* **Strict mode** removes skip and keeps focus on the break screen. On macOS it also hides the Dock and menu bar and blocks ⌘-Tab. You can still use exceptions.
* An optional one-tap **mood check-in** (😣 … 😄) feeds the weekly report.
* A heads-up card comes 2 min before the break, with **Take break now** and **I'm in flow**
  actions.

## 4. Exceptions ("I'm doing VIP work")

You can ask for more time from the menu bar, from the warning notification, or from the break
screen itself.

| Rule | Default | Why |
|---|---|---|
| Choose duration | 5 / 10 / 15 / 25 min | Small, honest chunks |
| Reason category + one-line note | required | A little friction stops reflex-clicking |
| Daily budget | 3 per day | Exceptions should be rare |
| Hard cap on continuous work | 120 min | Exceptions can't add up to a 3-hour stretch |
| Logged | always | History tab shows when and why |

If you ask during a break, the break ends right away and the due time moves forward. Breaks
skipped this way count as *skipped* in stats.

## 5. Meetings never get blocked

A meeting is detected if **any** of these are true (each can be toggled):

1. **Camera in use** by any app. macOS: CoreMediaIO `DeviceIsRunningSomewhere`. Windows: the
   CapabilityAccessManager usage records (what drives the taskbar's camera indicator). Linux: an
   open `/dev/video*`.
2. **Microphone in use** by any app. macOS: CoreAudio, input devices only, so music doesn't count.
   Windows: as for the camera. Linux: a PulseAudio/PipeWire recording stream (`pactl`).
3. **Calendar event happening now** (macOS, EventKit, opt-in): busy, not declined, not all-day.
4. **Manual toggle**: "I'm in a meeting" for in-person meetings or screen-share-only calls.

This works with Zoom, Meet, Teams, Slack huddles, FaceTime, Webex and Discord, with no per-app
integration. Ebb only *asks the system whether* a device is in use. It never opens the camera or mic, so
it doesn't trigger camera/mic permission prompts.

Behaviour:

* Break due during a meeting → **meeting hold**. The break waits with no limit.
* Meeting ends → notification "Meeting over, break in 1:30" → break.
* Call starts **during** a break → the break is suspended immediately so you can join, and it resumes
  after the call.
* If you walk away after the meeting, that counts as the break.

## 6. Eyes, body and night

* **20-20-20 eye breaks.** After every 20 min of screen time, a small card asks you to look 6 m
  (20 ft) away for 20 s, with a filling ring. Skippable; never during a meeting, a heads-up or a break.
* **Nudges**: blink (15 min), posture (30), water (45), stretch (60, off by default). Each has its
  own timer on screen time, at most one card at a time and none within 90 s of another.
* A real break (taken or natural) resets all of these.
* **Daily limit** (off by default, 8 h): a heads-up at 90 %, then a reminder every 30 min while over.
* **Night wind-down** (off by default, 22:30–06:00): a reminder when it starts and every 30 min,
  plus a warm, click-through tint over every screen. Late-night screen time is tracked either way.

All of this is `Wellness` in `src-tauri/src/wellness.rs` (pure, tested) plus checks in the engine.

## 7. Menu bar / tray

* Icon + countdown (e.g. `≋ 23:10` on macOS; in the tooltip on Windows).
* Click: a panel with a progress ring, the current state, *why* Ebb thinks you're in a meeting
  or still counting ("Watching a video", "No input, still counting"), and these actions: a progress ring, the current state, *why* Ebb thinks you're in a meeting, and these actions:
  Take a break now · Ask for more time · I'm in a meeting · Pause (30 min / 1 h / 2 h / until resumed),
  plus today's stats.

* Right-click (or any click on Linux): the same actions as a menu, plus Weekly report, Check for updates.

## 8. Stats, wellness score and weekly report

Stored per day in `stats.json` in the app data folder (the 0.1 Mac app's history is imported
on first launch): screen time, meeting time, passive (reading/watching) time, late-night time,
breaks completed / natural / skipped, eye breaks taken / skipped, nudges, meeting deferrals,
longest stretch, moods, and every exception with its reason.

**Wellness score** (0–100) for a day:

| Part | Points | Full marks when |
|---|---|---|
| Breaks taken | 35 | every break taken (share of taken vs skipped) |
| Longest stretch | 20 | ≤ work length + 10 min; zero at 2.5 × work length |
| Eye breaks | 15 | share taken vs skipped |
| Screen time | 15 | ≤ daily limit (8 h if none); zero 4 h past it |
| Late nights | 15 | none; zero at 1 h |
| Exceptions | −3 each | |

The **weekly report** (Settings → Weekly report, and a card on the first day of each week)
shows screen time per day with each day's score, averages, the change on the week before,
average mood, and suggestions drawn from the numbers (skipped breaks, long stretches, late
nights, low mood, …).

## 9. Settings

Rhythm (work/break/warning lengths, grace period, video counting, natural-break length), strict
mode, hold-to-skip time, eye breaks, nudges, mood check-in, daily limit, wind-down hours and dim
level, exception budget/reason/hard cap, meeting signals, post-meeting grace, launch at login,
countdown in the tray, automatic updates.

## 10. Architecture

```
src-tauri/src/clock.rs      Pure work/break state machine (tested)
src-tauri/src/wellness.rs   Pure eye-break and nudge scheduler (tested)
src-tauri/src/stats.rs      Daily history, wellness score, weekly report and tips (tested)
src-tauri/src/prefs.rs      Settings in prefs.json (tested)
src-tauri/src/engine.rs     1 Hz loop: OS signals → clock + wellness → effects (tested)
src-tauri/src/platform/     macos.rs · windows.rs · linux.rs
src-tauri/src/ui.rs         Windows: break overlay per monitor, toast, eye card, dim, panel, settings
src-tauri/src/updater.rs    Auto-update from GitHub Releases (signed latest.json)
src-tauri/src/lib.rs        Tray, commands, startup
ui/                         Plain HTML/CSS/JS, no build step; draws the state the core broadcasts
```

* [Tauri 2](https://tauri.app): Rust core, the system web view for UI (WebKit on macOS and Linux,
  WebView2 on Windows). Installers are ~5–10 MB.
* The core owns all state and broadcasts a snapshot to every window once a second; windows only
  draw it and call commands.
* **Updates:** CI signs each release with a private key (GitHub secret); the app only installs
  updates signed by the matching public key in `tauri.conf.json`. Downloads happen in the
  background; installing waits until you step away or click *Restart now*.

## 11. Roadmap

* Gentle pre-break dimming in the last 30 s instead of a hard cut.
* Per-app rules: e.g. never block while presenting.
* Global hotkey to ask for an exception.
* Calendar detection on Windows/Linux (ICS link).
* Wayland idle detection beyond GNOME/KDE (`ext-idle-notify`).
* Code signing (Apple Developer ID, Windows Authenticode) so first launch has no warning.
* Onboarding: pick a rhythm preset (Pomodoro 25/5, 50/10, 90/20 Ultradian).
