<p align="center"><img src="Resources/AppIcon.png" width="160" alt="Ebb icon"></p>

<h1 align="center">Ebb</h1>
<p align="center"><b>Work in flow, rest in ebb.</b><br>
A macOS menu-bar app that makes you take screen breaks, lets you ask for more time when it matters, and never interrupts a meeting.</p>

---

## What it does

- **Counts real screen time.** It only counts minutes you're actually using the Mac. Step away for 5 minutes and that counts as a break.
- **Blocks the screen when it's break time.** A calm full-screen overlay covers every display, with a breathing guide, a countdown and tips. You get a heads-up 2 minutes before.
- **Exceptions for important work.** Choose 5–25 minutes, say why, and it comes out of a small daily budget (3/day by default). A hard cap of 120 min continuous work applies even with exceptions.
- **Meeting-aware.** If the camera or mic is in use, a calendar event is happening, or you've switched on "I'm in a meeting", the break waits until the call ends. Works with Zoom, Meet, Teams, Slack, FaceTime and others.
- **Strict mode** hides the Dock and menu bar and blocks ⌘-Tab during breaks.
- **History**: screen time, breaks taken or skipped, every exception and its reason, and a daily balance score.

Full design, state machine and roadmap: **[docs/PRODUCT.md](docs/PRODUCT.md)**.

## Install (no coding needed)

1. Download **Ebb.zip** from the latest release on the [Releases page](../../releases/latest), then unzip it.
2. Drag **Ebb** into **Applications** and open it.
3. The first time, macOS blocks it because the developer isn't verified. Go to **System Settings → Privacy & Security** and click **Open Anyway**.

Needs macOS 14+, and works on Apple Silicon and Intel Macs.

## Publishing a new version

```bash
git tag v0.2.0 && git push origin v0.2.0
```
GitHub Actions then tests the code, builds a universal `Ebb.app`, and publishes a release with `Ebb.zip` attached.

## Build & run (on your Mac)

Needs macOS 14+ and Xcode 15+ (or just the Command Line Tools: `xcode-select --install`).

```bash
git clone https://github.com/abdullah-abdullatif/ebb.git
cd ebb
make test      # run the state-machine unit tests
make run       # build build/Ebb.app and launch it
make install   # copy to /Applications (needed for "Open at login")
```

Ebb shows up in the menu bar as `≋ 49:58`, with no Dock icon.

**To work on it in Xcode:** `open Package.swift`, choose the *Ebb* scheme, then Run.
(When run from Xcode the binary isn't inside a `.app`, so notifications and launch-at-login are disabled. Use `make run` to test those.)

### Permissions

| Feature | Permission |
|---|---|
| Idle detection | none |
| Camera / mic "in use" detection | none: Ebb asks macOS whether a device is in use and never opens it |
| Calendar meetings (opt-in) | Calendar access, asked when you enable it |
| Notifications | Asked on first launch |

### Quick test settings

To see a break quickly, open **Settings → Rhythm**, set *Work* to 10 min and *Break* to 1 min, or click **Take a break now** in the menu.

## Project layout

```
Package.swift
Sources/Ebb/
  App/        EbbApp (MenuBarExtra + Settings)
  Core/       SessionClock (pure state machine), SessionEngine, Preferences, StatsStore, Notifier
  Detection/  ActivityMonitor (idle), MeetingDetector (camera/mic/calendar)
  UI/         BreakOverlayController, BreakView, ExceptionRequestView, MenuBarView, SettingsView, Theme
Tests/EbbTests/   SessionClock tests
Resources/        Info.plist, AppIcon.svg, AppIcon.png
scripts/          build-app.sh, make-icns.sh
```
