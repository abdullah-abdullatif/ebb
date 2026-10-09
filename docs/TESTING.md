# Testing Ebb

## Automatic (runs on every push)

GitHub Actions runs these on a real Mac (`.github/workflows/build.yml`):

| Check | What it proves |
|---|---|
| `swift test` | The work/break logic, exceptions, meetings, sleep, pause, settings, history and formatting (35+ tests) |
| Universal build | The app compiles for Apple Silicon and Intel without errors |
| `scripts/smoke-test.sh` | The **real app** launches, opens Settings, warns, shows the break screen, finishes the break and goes back to work, without crashing. Screenshots are attached to the run as the `smoke-test` artifact |

Run the same things locally with `make test` and `make smoke`.

## Manual checklist (things only a real person and Mac can check)

For quick testing, set **Settings → Rhythm → Work** to 10 min and **Break** to 1 min.

### Menu bar
- [ ] The waves icon and countdown appear in the menu bar; there's no Dock icon.
- [ ] The countdown goes down while you type or use the mouse.
- [ ] Stop touching the Mac for over 60 s: the clock pauses ("Paused — no activity").
- [ ] Leave it for 5+ min: you're greeted with "Welcome back" and a fresh cycle.
- [ ] **Settings…** opens the Settings window in front of everything.
- [ ] **Pause Ebb → 30 minutes** pauses it; **Resume** brings it back.

### Heads-up & break
- [ ] 2 min before the break, a "Break in 2:00" notification appears.
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
- [ ] Turn on **Calendar** detection, allow access, and during a calendar event it counts as a meeting.
- [ ] The **I'm in a meeting** checkbox works with no camera or mic.

### Strict mode
- [ ] Turn on Strict mode. During a break the Dock and menu bar are hidden, ⌘-Tab does nothing, and there's no skip button.
- [ ] Exceptions still work in strict mode.

### System
- [ ] Close the lid for 10 min, open it: Ebb counts that as a break.
- [ ] **Open Ebb at login** (with Ebb in /Applications): log out and in, and Ebb starts.
- [ ] Quit and reopen Ebb: settings and today's history are kept, and so is the number of exceptions used.
- [ ] Plug in or unplug a monitor during a break: the break screen adjusts.
