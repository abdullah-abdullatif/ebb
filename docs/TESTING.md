# Testing Ebb

## Automatic (runs on every push)

GitHub Actions runs these on real macOS, Windows and Linux machines (`.github/workflows/build.yml`):

| Check | What it proves |
|---|---|
| `cargo test` | The work/break clock (including the new "watching without touching" rules), eye breaks and nudges, wellness score, weekly report and tips, settings, storage and the import of 0.1 history, and the engine's effects |
| Build | Installers build for each OS (universal binary on macOS), signed for the updater |
| `scripts/smoke-test.sh` | The **real app** launches, opens Settings, shows an eye break, warns, shows the break screen, finishes the break and goes back to work without crashing, and writes its history. On Linux it runs under a virtual display. macOS screenshots are attached to the run as `smoke-test-macOS` |

**After every release** (`.github/workflows/update-test.yml`, also runnable from the Actions tab): on each OS, an old build labelled 0.0.1 is run against the real GitHub release and must find the update, download it, verify its signature, install it and restart. The test then checks the result on disk: `Ebb.app` reports the new version (macOS), the installed version in the registry matches (Windows), and the AppImage is byte-for-byte the released one (Linux).

Run the same things locally with `make test` and `make smoke`.

## Manual checklist (things only a real person and computer can check)

Run it on each OS you ship to. Items marked macOS only don't apply elsewhere.

For quick testing, set **Settings → Rhythm → Work** to 10 min and **Break** to 1 min.

### Menu bar
- [ ] The waves icon (and countdown on macOS/Linux) appears in the menu bar / tray; there's no Dock or taskbar icon.
- [ ] The countdown goes down while you type or use the mouse.
- [ ] Stop touching the computer for a few minutes: the clock **keeps running** and the panel says "No input, still counting".
- [ ] Play a YouTube video full-screen for 20+ min without touching anything: the panel says "Watching a video" and the clock keeps running.
- [ ] Lock the screen: the clock pauses at once ("Paused: screen locked"). Stay locked 5+ min: "Welcome back" and a fresh cycle.
- [ ] Walk away without locking for longer than the grace period: a fresh cycle, and today's screen time drops by the time you were gone.
- [ ] **Settings…** opens the Settings window in front of everything.
- [ ] **Pause Ebb → 30 minutes** pauses it; **Resume** brings it back.

### Eyes, body, night
- [ ] After 20 min of screen time, the "Look far away" card appears at the top for 20 s; **Skip** closes it.
- [ ] Blink / posture / water cards appear on their intervals and disappear by themselves.
- [ ] Turn on a daily limit of 1 h: a heads-up at 54 min, then "Daily screen time reached".
- [ ] Turn on wind-down with a start time a minute from now: a reminder appears and the screen gets a warm tint you can click through. Moving the slider changes it live.
- [ ] Mood faces on the break screen; the weekly report shows the average.
- [ ] **Settings → Weekly report**: bars per day with scores, hover tooltips, ← → between weeks, suggestions.

### Updates
- [ ] Install an older release, publish a newer tag: within a few hours (or via **Check for updates…**) the "Ebb x.y.z is ready" card appears; **Restart now** opens the new version.
- [ ] With an update downloaded, lock the screen for 5+ min: Ebb restarts into the new version by itself.

### Heads-up & break
- [ ] 2 min before the break, a "Break in 2:00" card appears in the top-right corner.
- [ ] Its **Take break now** button starts the break.
- [ ] The break screen covers **every display**, including a full-screen app (try a full-screen Safari window).
- [ ] The countdown, breathing circle and tips show; tips change every 30 s.
- [ ] When the break ends, the screen clears and "Welcome back" appears.
- [ ] **Hold to skip**: a short click does nothing; holding 3 s skips.

### Exceptions
- [ ] From the menu: **I'm in flow — ask for more time…** opens the request window.
- [ ] You can't confirm without a reason (if "Require a reason" is on).
- [ ] Granting 10 min moves the countdown forward by 10 min.
- [ ] From the break screen: **I need more time** ends the break and grants the time.
- [ ] After 3 exceptions in a day, the button says "No exceptions left today".
- [ ] The **History** tab lists each exception with its reason.

### Meetings
- [ ] Join a Zoom/Meet/FaceTime call with the camera on: the menu shows "In a meeting: camera on".
- [ ] Mic only (camera off): shows "mic on".
- [ ] Let the break come due during the call: you get "Break postponed — you're in a meeting" and **no break screen**.
- [ ] Hang up: "Meeting over — break in 1:30", then the break starts.
- [ ] Start a call **during** a break: the break screen disappears so you can join.
- [ ] (macOS) Turn on **Calendar** detection, allow access, and during a calendar event it counts as a meeting.
- [ ] The **I'm in a meeting** checkbox works with no camera or mic.

### Strict mode
- [ ] Turn on Strict mode. During a break there's no skip button and clicking another window brings the break back. (macOS) The Dock and menu bar are hidden and ⌘-Tab does nothing.
- [ ] Exceptions still work in strict mode.

### System
- [ ] Close the lid for 10 min, open it: Ebb counts that as a break.
- [ ] **Open Ebb at login** (with Ebb in /Applications): log out and in, and Ebb starts.
- [ ] Quit and reopen Ebb: settings and today's history are kept, and so is the number of exceptions used.
- [ ] (macOS, upgrading from 0.1) History from the old app shows up in Settings → Today.
- [ ] Plug in or unplug a monitor during a break: the break screen adjusts.
