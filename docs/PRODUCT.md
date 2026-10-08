# Ebb — product & system design

> **Work in flow, rest in ebb.**
> A macOS menu-bar app that tracks how long you've really been at the screen, makes you take a
> break when it's time, lets you ask for more time when you're deep in something, and never
> interrupts a meeting.

## 1. Name & icon

**Ebb**: the tide going out. Work comes in like the tide (flow), and rest is the ebb.
It's short, easy to say, and doesn't sound like a stopwatch or a nag.

The icon is a warm sun setting into two teal waves under a night sky. The waves are flow and
ebb, and the setting sun means *wind down*. Source: `Resources/AppIcon.svg`; rendered:
`Resources/AppIcon.png` (1024²). Inside the app the same palette is in `UI/Theme.swift`.

Other names considered: *Tidewell*, *Respite*, *Recess*, *Undertow*, *Lull*.

## 2. Core loop

```
            active input                       break due, no meeting
 ┌─────────┐ ───────────▶ ┌─────────┐  T-2min  ┌─────────┐ ─────────▶ ┌──────────┐
 │  idle   │              │ working │ ───────▶ │ warning │            │ on break │
 └─────────┘ ◀─────────── └─────────┘          └─────────┘            └──────────┘
     │   no input ≥ 60 s       │  ▲                  │ break due,        │  ▲  │
     │                         │  │ exception        │ in meeting        │  │  │ timer done
     │ no input ≥ 5 min        │  └──────────────────┤                   │  │  ▼
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

All of this lives in one pure struct, `Core/SessionClock.swift`. It doesn't know about
timers or AppKit. It takes one `tick` per second and returns events, which keeps it fully
unit-tested (`Tests/EbbTests`).

### What counts as "work"

| Signal | Effect |
|---|---|
| Keyboard/mouse input within the last 60 s | Work clock runs |
| No input for 60 s – 5 min | Clock **freezes** (reading, thinking) |
| No input for ≥ 5 min, or the Mac slept/locked that long | **Natural break**: the cycle resets and it counts in stats |
| In a meeting (even with no input) | Counts as screen time, never as a break |

Ebb reads the system HID idle counter, so it needs **no Accessibility or Input Monitoring
permission**.

## 3. The break

* Full-screen blurred overlay on **every display** and every Space, above full-screen apps
  (`.screenSaver` window level).
* It shows a breathing guide (4 s in, 6 s out), a countdown, and a new wellbeing tip every 30 s.
* **Normal mode** has an emergency *hold-to-skip* button (3 s hold). A skip is recorded as a skipped
  break.
* **Strict mode** hides the Dock and menu bar, blocks ⌘-Tab, and removes skip. You can still use exceptions.
* A heads-up notification comes 2 min before the break, with **Take break now** and **I'm in flow**
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

1. **Camera in use** by any app: CoreMediaIO `kCMIODevicePropertyDeviceIsRunningSomewhere`.
2. **Microphone in use** by any app: CoreAudio, input devices only, so music doesn't count.
3. **Calendar event happening now** (EventKit, opt-in): busy, not declined, not all-day.
4. **Manual toggle**: "I'm in a meeting" for in-person meetings or screen-share-only calls.

This works with Zoom, Meet, Teams, Slack huddles, FaceTime, Webex and Discord, with no per-app
integration. Ebb only *asks macOS whether* a device is in use. It never opens the camera or mic, so
it doesn't trigger camera/mic permission prompts.

Behaviour:

* Break due during a meeting → **meeting hold**. The break waits with no limit.
* Meeting ends → notification "Meeting over, break in 1:30" → break.
* Call starts **during** a break → the break is suspended immediately so you can join, and it resumes
  after the call.
* If you walk away after the meeting, that counts as the break.

## 6. Menu bar

* Icon + countdown (e.g. `≋ 23:10`). The icon changes with state: waves, hourglass, video, cup, moon, pause.
* The popover has a progress ring, the current state, *why* Ebb thinks you're in a meeting, and these actions:
  Take a break now · Ask for more time · I'm in a meeting · Pause (30 min / 1 h / 2 h / until resumed),
  plus today's stats.

## 7. Stats & history

Stored per day in `~/Library/Application Support/Ebb/stats.json`:
screen time, meeting time, breaks completed / natural / skipped, meeting deferrals, longest
stretch, and every exception with its reason. The **balance score** (0–100) is the share of
breaks you took, minus 5 points per exception.

## 8. Settings

Rhythm (work/break/warning lengths, idle thresholds), strict mode, hold-to-skip time,
exception budget/reason/hard cap, meeting signals, post-meeting grace, launch at login
(`SMAppService`), countdown in menu bar.

## 9. Architecture

```
App/EbbApp.swift            MenuBarExtra + Settings scenes, AppDelegate (disables App Nap)
Core/SessionClock.swift     Pure state machine (tested)
Core/SessionEngine.swift    1 Hz timer → reads idle + meeting signals → clock → side effects
Core/Preferences.swift      Codable prefs in UserDefaults
Core/StatsStore.swift       Daily JSON history
Core/Notifier.swift         UserNotifications with action buttons
Detection/ActivityMonitor   CGEventSource idle time
Detection/MeetingDetector   Camera / mic / calendar / manual
UI/BreakOverlayController   One borderless window per screen, strict-mode presentation options
UI/BreakView, ExceptionRequestView, MenuBarView, SettingsView, Theme
```

* macOS 14+, Swift 5.9, SwiftUI + small AppKit pieces. No third-party dependencies.
* Builds with plain `swift build`. `scripts/build-app.sh` wraps the binary into `Ebb.app`
  (`LSUIElement`, so there's no Dock icon).
* Not sandboxed: the overlay and device-usage checks are simplest outside the sandbox. For a Mac
  App Store build see the roadmap.

## 10. Roadmap

**v0.2**
* Gentle pre-break dimming in the last 30 s instead of a hard cut.
* "Micro-breaks": a 20-second eye break every 20 min (20-20-20 rule).
* Per-app rules: e.g. never block while Keynote is presenting or a full-screen video is playing.
* Global hotkey to ask for an exception.

**v0.3**
* Weekly report and trends chart (Swift Charts).
* Focus-mode integration: follow macOS Focus filters (e.g. "Deep Work" gives longer cycles).
* Screen-sharing detection (CGWindowList / ScreenCaptureKit).

**v1.0**
* Developer ID signing + notarization, Sparkle auto-updates.
* Optional sandboxed Mac App Store build.
* Onboarding flow: pick a rhythm preset (Pomodoro 25/5, 50/10, 90/20 Ultradian).
* iCloud sync of stats across Macs.
