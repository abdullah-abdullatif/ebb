import AppKit
import Combine
import ServiceManagement

/// Runs the clock once a second against the real world and turns its events
/// into overlays, notifications and stats.
@MainActor
final class SessionEngine: ObservableObject {
    @Published private(set) var clock: SessionClock
    @Published private(set) var meeting = MeetingSignals()
    @Published private(set) var lastDenial: ExceptionDenial?

    let prefsStore: PreferencesStore
    let stats: StatsStore
    let detector = MeetingDetector()
    let notifier = Notifier()
    private lazy var overlay = BreakOverlayController(engine: self)

    private var timer: Timer?
    private var lastTick = Date()
    private var tickCount = 0
    private var currentDay = StatsStore.key()
    private var cancellables: Set<AnyCancellable> = []

    private enum NotificationID {
        static let warning = "ebb.warning"
        static let meeting = "ebb.meeting"
        static let info = "ebb.info"
    }

    /// `EBB_SMOKE_TEST=1`: used by CI to run the real app unattended. A "minute" lasts one
    /// second, input is treated as constant, and every clock event is logged as `EBB_EVENT …`.
    nonisolated static let isSmokeTest = ProcessInfo.processInfo.environment["EBB_SMOKE_TEST"] == "1"

    init(prefsStore: PreferencesStore, stats: StatsStore) {
        self.prefsStore = prefsStore
        self.stats = stats
        self.clock = SessionClock(prefs: prefsStore.prefs, minuteLength: Self.isSmokeTest ? 1 : 60)
        clock.restoreExceptionsUsed(stats.today.exceptions.count)
    }

    var prefs: Preferences { prefsStore.prefs }

    func start() {
        notifier.onAction = { [weak self] action in self?.handle(notificationAction: action) }
        notifier.setUp()

        prefsStore.$prefs
            .dropFirst()
            .sink { [weak self] new in self?.apply(prefs: new) }
            .store(in: &cancellables)

        if prefs.detectCalendar {
            Task { _ = await detector.requestCalendarAccess() }
        }

        let ws = NSWorkspace.shared.notificationCenter
        ws.addObserver(forName: NSWorkspace.didWakeNotification, object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in self?.catchUpAfterGap() }
        }

        lastTick = Date()
        let t = Timer(timeInterval: 1, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.tick() }
        }
        t.tolerance = 0.2
        RunLoop.main.add(t, forMode: .common)
        timer = t
    }

    func shutdown() {
        timer?.invalidate()
        stats.saveIfNeeded()
    }

    // MARK: Tick

    private func tick() {
        let now = Date()
        rollDayIfNeeded()
        catchUpAfterGap(now: now)
        lastTick = now
        tickCount += 1

        // Hardware polling is cheap, but every 2 s is plenty.
        if tickCount % 2 == 0 || meeting.manual != detector.manualOverride {
            let fresh = detector.signals(prefs: prefs, now: now)
            if fresh != meeting { meeting = fresh }
        }

        let idle = Self.isSmokeTest ? 0 : ActivityMonitor.idleSeconds()
        let events = clock.tick(TickInput(idleSeconds: idle, inMeeting: meeting.inMeeting, now: now))

        recordTime(idle: idle)
        handle(events)

        if tickCount % 30 == 0 { stats.saveIfNeeded() }
    }

    /// Timers don't fire while the Mac sleeps; a big jump means you were away.
    private func catchUpAfterGap(now: Date = Date()) {
        let gap = Int(now.timeIntervalSince(lastTick))
        guard gap > 5 else { return }
        lastTick = now
        handle(clock.registerGap(seconds: gap))
    }

    private func recordTime(idle: Double) {
        let phase = clock.phase
        guard phase == .working || phase == .warning || phase == .meetingHold else { return }
        let worked = clock.workedSeconds
        let inMeeting = meeting.inMeeting
        stats.update { d in
            d.activeSeconds += 1
            if inMeeting { d.meetingSeconds += 1 }
            d.longestStretchSeconds = max(d.longestStretchSeconds, worked)
        }
    }

    private func rollDayIfNeeded() {
        let key = StatsStore.key()
        guard key != currentDay else { return }
        stats.saveIfNeeded()
        currentDay = key
        clock.startNewDay()
    }

    // MARK: Events → side effects

    private func handle(_ events: [ClockEvent]) {
        for event in events {
            if Self.isSmokeTest { NSLog("EBB_EVENT %@", String(describing: event)) }
            switch event {
            case .warning(let left):
                notifier.post(id: NotificationID.warning,
                              title: "Break in \(Self.format(left))",
                              body: "Start wrapping up. In the middle of something? Ask for more time.",
                              category: Notifier.warningCategory)

            case .breakStarted:
                notifier.clear(id: NotificationID.warning)
                overlay.show(strict: prefs.strictMode)
                NSSound(named: "Glass")?.play()

            case .breakEnded(let completed):
                overlay.hide()
                stats.update { d in
                    if completed { d.breaksCompleted += 1 } else { d.breaksSkipped += 1 }
                }
                if completed {
                    notifier.post(id: NotificationID.info, title: "Welcome back",
                                  body: "Fresh \(prefs.workMinutes)-minute cycle started.")
                }

            case .naturalBreak:
                stats.update { $0.naturalBreaks += 1 }

            case .deferredForMeeting:
                stats.update { $0.meetingDeferrals += 1 }
                notifier.post(id: NotificationID.meeting, title: "Break postponed — you're in a meeting",
                              body: "Ebb will wait until the call ends.")

            case .breakInterruptedByMeeting:
                overlay.hide()
                stats.update { $0.meetingDeferrals += 1 }

            case .meetingEnded(let grace):
                notifier.post(id: NotificationID.meeting, title: "Meeting over",
                              body: "Your break starts in \(Self.format(grace)). Grab some water.",
                              sound: true)

            case .exceptionGranted, .resumed:
                break
            }
        }
    }

    private func handle(notificationAction action: Notifier.Action) {
        switch action {
        case .breakNow: startBreakNow()
        case .extend: overlay.showExceptionRequest()
        }
    }

    // MARK: User intents (called from UI)

    func startBreakNow() { handle(clock.startBreakNow()) }

    func emergencySkip() {
        guard !prefs.strictMode else { return }
        handle(clock.skipBreak())
    }

    func pause(minutes: Int?) {
        clock.pause(until: minutes.map { Date().addingTimeInterval(TimeInterval($0 * 60)) })
        overlay.hide()
    }

    func resume() { handle(clock.resume()) }

    func setManualMeeting(_ on: Bool) {
        detector.manualOverride = on
        meeting.manual = on
    }

    func openExceptionRequest() { overlay.showExceptionRequest() }

    func exceptionDenial(minutes: Int) -> ExceptionDenial? { clock.checkException(minutes: minutes) }

    @discardableResult
    func requestException(minutes: Int, category: String, reason: String) -> Bool {
        switch clock.grantException(minutes: minutes) {
        case .success(let events):
            lastDenial = nil
            stats.update { d in
                d.exceptions.append(ExceptionRecord(date: Date(), minutes: minutes,
                                                    reason: reason, category: category))
            }
            stats.saveIfNeeded()
            handle(events)
            overlay.hideExceptionRequest()
            return true
        case .failure(let denial):
            lastDenial = denial
            return false
        }
    }

    // MARK: Prefs

    private func apply(prefs new: Preferences) {
        let old = clock.prefs
        clock.updatePrefs(new)
        if new.detectCalendar && !old.detectCalendar {
            Task { _ = await detector.requestCalendarAccess() }
        }
        if new.launchAtLogin != old.launchAtLogin {
            do {
                if new.launchAtLogin { try SMAppService.mainApp.register() }
                else { try SMAppService.mainApp.unregister() }
            } catch {
                NSLog("Ebb: launch-at-login change failed: \(error)")
            }
        }
    }

    // MARK: Formatting

    static func format(_ seconds: Int) -> String {
        let s = max(0, seconds)
        if s >= 3600 { return String(format: "%d:%02d:%02d", s / 3600, (s % 3600) / 60, s % 60) }
        return String(format: "%d:%02d", s / 60, s % 60)
    }
}
