import Foundation

/// Where we are in the work → break cycle.
enum Phase: Equatable {
    /// At the screen, the work clock is running.
    case working
    /// Break is close; a heads-up was sent.
    case warning
    /// Short pause in input (reading, thinking). Clock is frozen, not reset.
    case idle
    /// Away long enough to count as a real break. Next activity starts a fresh cycle.
    case away
    /// A break is due but you're in a meeting, so it waits.
    case meetingHold
    /// Meeting just ended; break starts when this runs out.
    case postMeetingGrace(remaining: Int)
    /// Screen is blocked.
    case onBreak(remaining: Int)
    /// User paused Ebb (e.g. "pause for 1 hour"). nil = until resumed.
    case paused(until: Date?)

    var isOnBreak: Bool { if case .onBreak = self { return true } else { return false } }
}

/// One second of the outside world, as seen by the clock.
struct TickInput {
    var idleSeconds: Double
    var inMeeting: Bool
    var now: Date = Date()
}

enum ClockEvent: Equatable {
    case warning(secondsLeft: Int)
    case breakStarted
    case breakEnded(completed: Bool)
    case naturalBreak(workedSeconds: Int)
    case deferredForMeeting
    case breakInterruptedByMeeting
    case meetingEnded(graceSeconds: Int)
    case exceptionGranted(minutes: Int)
    case resumed
}

enum ExceptionDenial: Error, Equatable {
    case noneLeftToday
    /// Would push continuous work past the hard cap. `maxMinutes` is what's still allowed (may be 0).
    case hardCap(maxMinutes: Int)
    case notNow
}

/// The pure state machine behind Ebb. No timers, no UI, no system calls —
/// feed it one `tick` per second and it tells you what happened.
struct SessionClock {
    private(set) var prefs: Preferences
    private(set) var phase: Phase = .working
    /// Active screen seconds since the last real break.
    private(set) var workedSeconds = 0
    /// `workedSeconds` value at which the break is due. Grows with exceptions.
    private(set) var breakDueAt: Int
    private(set) var exceptionsUsedToday = 0
    private var warned = false
    /// Seconds in one "minute" of the preferences. 60 in real life; tiny in smoke tests.
    let minuteLength: Int

    init(prefs: Preferences, minuteLength: Int = 60) {
        self.prefs = prefs
        self.minuteLength = minuteLength
        self.breakDueAt = prefs.workMinutes * minuteLength
    }

    private var workSeconds: Int { prefs.workMinutes * minuteLength }
    private var breakSeconds: Int { prefs.breakMinutes * minuteLength }
    private var naturalBreakSeconds: Int { prefs.naturalBreakMinutes * minuteLength }
    private var hardCapSeconds: Int { prefs.hardCapMinutes * minuteLength }

    var secondsUntilBreak: Int { max(0, breakDueAt - workedSeconds) }
    var exceptionsLeftToday: Int { max(0, prefs.exceptionsPerDay - exceptionsUsedToday) }
    var cycleProgress: Double {
        guard breakDueAt > 0 else { return 0 }
        return min(1, Double(workedSeconds) / Double(breakDueAt))
    }

    // MARK: Tick

    mutating func tick(_ input: TickInput) -> [ClockEvent] {
        var events: [ClockEvent] = []

        switch phase {
        case .paused(let until):
            if let until, input.now >= until {
                resetCycle()
                phase = .working
                events.append(.resumed)
            }
            return events

        case .onBreak(let remaining):
            if input.inMeeting {
                // A call came in mid-break: never block a meeting. Break waits.
                phase = .meetingHold
                events.append(.breakInterruptedByMeeting)
            } else if remaining <= 1 {
                events += finishBreak(completed: true)
            } else {
                phase = .onBreak(remaining: remaining - 1)
            }
            return events

        case .meetingHold:
            if input.inMeeting {
                workedSeconds += 1
            } else {
                phase = .postMeetingGrace(remaining: prefs.postMeetingGraceSeconds)
                events.append(.meetingEnded(graceSeconds: prefs.postMeetingGraceSeconds))
            }
            return events

        case .postMeetingGrace(let remaining):
            if input.inMeeting {
                phase = .meetingHold
            } else if input.idleSeconds >= Double(naturalBreakSeconds) {
                events.append(.naturalBreak(workedSeconds: workedSeconds))
                resetCycle()
                phase = .away
            } else if remaining <= 1 {
                events += startBreak()
            } else {
                phase = .postMeetingGrace(remaining: remaining - 1)
            }
            return events

        case .working, .warning, .idle, .away:
            break
        }

        // Sitting silently in a call is still screen time, not a break.
        let idle = input.inMeeting ? 0 : input.idleSeconds

        if idle >= Double(naturalBreakSeconds) {
            if phase != .away {
                if workedSeconds > 0 { events.append(.naturalBreak(workedSeconds: workedSeconds)) }
                resetCycle()
                phase = .away
            }
            return events
        }
        if idle >= Double(prefs.idlePauseSeconds) {
            if phase != .away { phase = .idle }
            return events
        }

        // Active.
        workedSeconds += 1
        phase = .working
        let left = breakDueAt - workedSeconds

        if left <= 0 {
            if input.inMeeting {
                phase = .meetingHold
                events.append(.deferredForMeeting)
            } else {
                events += startBreak()
            }
        } else if left <= prefs.warningSeconds {
            phase = .warning
            if !warned {
                warned = true
                events.append(.warning(secondsLeft: left))
            }
        }
        return events
    }

    /// The Mac slept or the app was suspended for `seconds`. Long gaps count as a break.
    mutating func registerGap(seconds: Int) -> [ClockEvent] {
        guard seconds >= naturalBreakSeconds else { return [] }
        if case .paused = phase { return [] }
        var events: [ClockEvent] = []
        if phase.isOnBreak {
            events += finishBreak(completed: true)
        } else if workedSeconds > 0 {
            events.append(.naturalBreak(workedSeconds: workedSeconds))
        }
        resetCycle()
        phase = .away
        return events
    }

    // MARK: User actions

    mutating func startBreakNow() -> [ClockEvent] {
        if phase.isOnBreak { return [] }
        return startBreak()
    }

    /// Emergency skip. Ends the break without granting extra time; a new full cycle starts.
    mutating func skipBreak() -> [ClockEvent] {
        guard phase.isOnBreak else { return [] }
        return finishBreak(completed: false)
    }

    mutating func pause(until: Date?) {
        phase = .paused(until: until)
    }

    mutating func resume() -> [ClockEvent] {
        guard case .paused = phase else { return [] }
        resetCycle()
        phase = .working
        return [.resumed]
    }

    /// Longest exception that would still respect the hard cap, in whole minutes.
    var maxExceptionMinutes: Int {
        let base = max(breakDueAt, workedSeconds)
        return max(0, (hardCapSeconds - base) / minuteLength)
    }

    func checkException(minutes: Int) -> ExceptionDenial? {
        if case .paused = phase { return .notNow }
        if case .away = phase { return .notNow }
        if exceptionsLeftToday == 0 { return .noneLeftToday }
        if minutes > maxExceptionMinutes { return .hardCap(maxMinutes: maxExceptionMinutes) }
        return nil
    }

    /// "I'm in the middle of something" — push the break back.
    mutating func grantException(minutes: Int) -> Result<[ClockEvent], ExceptionDenial> {
        if let denial = checkException(minutes: minutes) { return .failure(denial) }
        var events: [ClockEvent] = []
        let wasOnBreak = phase.isOnBreak
        breakDueAt = max(breakDueAt, workedSeconds) + minutes * minuteLength
        exceptionsUsedToday += 1
        warned = false
        phase = .working
        if wasOnBreak { events.append(.breakEnded(completed: false)) }
        events.append(.exceptionGranted(minutes: minutes))
        return .success(events)
    }

    // MARK: Housekeeping

    mutating func updatePrefs(_ new: Preferences) {
        // Only move the due point if no exception has stretched it.
        if breakDueAt == workSeconds { breakDueAt = new.workMinutes * minuteLength }
        let newBreak = new.breakMinutes * minuteLength
        if case .onBreak(let remaining) = phase, newBreak < remaining {
            phase = .onBreak(remaining: newBreak)
        }
        prefs = new
    }

    mutating func startNewDay() { exceptionsUsedToday = 0 }

    /// After a relaunch, today's exceptions still count against the budget.
    mutating func restoreExceptionsUsed(_ count: Int) { exceptionsUsedToday = count }

    // MARK: Private

    private mutating func startBreak() -> [ClockEvent] {
        phase = .onBreak(remaining: breakSeconds)
        return [.breakStarted]
    }

    private mutating func finishBreak(completed: Bool) -> [ClockEvent] {
        resetCycle()
        phase = .working
        return [.breakEnded(completed: completed)]
    }

    private mutating func resetCycle() {
        workedSeconds = 0
        breakDueAt = workSeconds
        warned = false
    }
}
