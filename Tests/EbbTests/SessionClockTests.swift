import XCTest
@testable import Ebb

final class SessionClockTests: XCTestCase {
    private func prefs() -> Preferences {
        var p = Preferences()
        p.workMinutes = 10
        p.breakMinutes = 2
        p.warningSeconds = 60
        p.naturalBreakMinutes = 3
        p.idlePauseSeconds = 30
        p.exceptionsPerDay = 2
        p.hardCapMinutes = 30
        p.postMeetingGraceSeconds = 10
        return p
    }

    private let active = TickInput(idleSeconds: 0, inMeeting: false)
    private let inCall = TickInput(idleSeconds: 0, inMeeting: true)

    @discardableResult
    private func run(_ clock: inout SessionClock, _ input: TickInput, times: Int) -> [ClockEvent] {
        (0..<times).flatMap { _ in clock.tick(input) }
    }

    func testWarnsThenStartsBreakThenFinishes() {
        var c = SessionClock(prefs: prefs())
        let early = run(&c, active, times: 539)
        XCTAssertTrue(early.isEmpty)
        XCTAssertEqual(c.tick(active), [.warning(secondsLeft: 60)])
        XCTAssertEqual(c.phase, .warning)

        let toBreak = run(&c, active, times: 60)
        XCTAssertEqual(toBreak, [.breakStarted])
        XCTAssertEqual(c.phase, .onBreak(remaining: 120))

        let done = run(&c, active, times: 120)
        XCTAssertEqual(done, [.breakEnded(completed: true)])
        XCTAssertEqual(c.workedSeconds, 0)
    }

    func testIdlePausesAndLongIdleCountsAsNaturalBreak() {
        var c = SessionClock(prefs: prefs())
        run(&c, active, times: 100)
        _ = c.tick(TickInput(idleSeconds: 45, inMeeting: false))
        XCTAssertEqual(c.phase, .idle)
        XCTAssertEqual(c.workedSeconds, 100)

        let events = c.tick(TickInput(idleSeconds: 200, inMeeting: false))
        XCTAssertEqual(events, [.naturalBreak(workedSeconds: 100)])
        XCTAssertEqual(c.phase, .away)
        XCTAssertEqual(c.workedSeconds, 0)
    }

    func testMeetingDefersBreakUntilItEnds() {
        var c = SessionClock(prefs: prefs())
        run(&c, active, times: 590)
        let events = run(&c, inCall, times: 20)
        XCTAssertTrue(events.contains(.deferredForMeeting))
        XCTAssertEqual(c.phase, .meetingHold)

        XCTAssertEqual(c.tick(active), [.meetingEnded(graceSeconds: 10)])
        let after = run(&c, active, times: 10)
        XCTAssertEqual(after, [.breakStarted])
    }

    func testSilentMeetingIsNotANaturalBreak() {
        var c = SessionClock(prefs: prefs())
        run(&c, active, times: 50)
        _ = c.tick(TickInput(idleSeconds: 600, inMeeting: true))
        XCTAssertEqual(c.workedSeconds, 51)
    }

    func testMeetingDuringBreakSuspendsIt() {
        var c = SessionClock(prefs: prefs())
        _ = c.startBreakNow()
        XCTAssertEqual(c.tick(inCall), [.breakInterruptedByMeeting])
        XCTAssertEqual(c.phase, .meetingHold)
    }

    func testExceptionDuringBreakEndsItAndPushesDue() {
        var c = SessionClock(prefs: prefs())
        run(&c, active, times: 600)
        XCTAssertTrue(c.phase.isOnBreak)

        guard case .success(let events) = c.grantException(minutes: 5) else { return XCTFail() }
        XCTAssertEqual(events, [.breakEnded(completed: false), .exceptionGranted(minutes: 5)])
        XCTAssertEqual(c.secondsUntilBreak, 300)
        XCTAssertEqual(c.exceptionsLeftToday, 1)
    }

    func testExceptionBudgetAndHardCap() {
        var c = SessionClock(prefs: prefs())
        // due at 10 min, cap 30 min → at most 20 extra minutes.
        XCTAssertEqual(c.checkException(minutes: 25), .hardCap(maxMinutes: 20))
        _ = c.grantException(minutes: 15)
        _ = c.grantException(minutes: 5)
        XCTAssertEqual(c.checkException(minutes: 5), .noneLeftToday)
        c.startNewDay()
        XCTAssertEqual(c.checkException(minutes: 5), .hardCap(maxMinutes: 0))
    }

    func testSleepGapCountsAsBreak() {
        var c = SessionClock(prefs: prefs())
        run(&c, active, times: 300)
        XCTAssertEqual(c.registerGap(seconds: 30), [])
        XCTAssertEqual(c.registerGap(seconds: 600), [.naturalBreak(workedSeconds: 300)])
        XCTAssertEqual(c.workedSeconds, 0)
    }

    func testPauseUntilDateResumes() {
        var c = SessionClock(prefs: prefs())
        let now = Date()
        c.pause(until: now.addingTimeInterval(60))
        XCTAssertEqual(c.tick(TickInput(idleSeconds: 0, inMeeting: false, now: now)), [])
        XCTAssertEqual(c.tick(TickInput(idleSeconds: 0, inMeeting: false, now: now.addingTimeInterval(61))), [.resumed])
        XCTAssertEqual(c.phase, .working)
    }

    // MARK: Edge cases

    func testIdleDuringWarningDoesNotWarnTwice() {
        var c = SessionClock(prefs: prefs())
        run(&c, active, times: 540)
        _ = c.tick(TickInput(idleSeconds: 45, inMeeting: false))
        XCTAssertEqual(c.phase, .idle)
        XCTAssertEqual(c.tick(active), [])
        XCTAssertEqual(c.phase, .warning)
    }

    func testEmergencySkipEndsBreakAsSkipped() {
        var c = SessionClock(prefs: prefs())
        run(&c, active, times: 200)
        XCTAssertEqual(c.startBreakNow(), [.breakStarted])
        XCTAssertEqual(c.startBreakNow(), [], "starting a break during a break does nothing")
        XCTAssertEqual(c.skipBreak(), [.breakEnded(completed: false)])
        XCTAssertEqual(c.phase, .working)
        XCTAssertEqual(c.workedSeconds, 0)
        XCTAssertEqual(c.skipBreak(), [], "nothing to skip outside a break")
    }

    func testExceptionDuringMeetingHoldGoesBackToWork() {
        var c = SessionClock(prefs: prefs())
        run(&c, active, times: 599)
        XCTAssertEqual(c.tick(inCall), [.deferredForMeeting])
        guard case .success(let events) = c.grantException(minutes: 5) else { return XCTFail() }
        XCTAssertEqual(events, [.exceptionGranted(minutes: 5)])
        XCTAssertEqual(c.phase, .working)
        XCTAssertEqual(c.secondsUntilBreak, 300)
    }

    func testWalkingAwayAfterMeetingCountsAsTheBreak() {
        var c = SessionClock(prefs: prefs())
        run(&c, active, times: 599)
        _ = c.tick(inCall)
        XCTAssertEqual(c.tick(active), [.meetingEnded(graceSeconds: 10)])
        let events = c.tick(TickInput(idleSeconds: 200, inMeeting: false))
        XCTAssertEqual(events, [.naturalBreak(workedSeconds: 600)])
        XCTAssertEqual(c.phase, .away)
        XCTAssertEqual(c.workedSeconds, 0)
    }

    func testNewCallDuringGraceGoesBackToHold() {
        var c = SessionClock(prefs: prefs())
        run(&c, active, times: 599)
        _ = c.tick(inCall)
        _ = c.tick(active)
        XCTAssertEqual(c.tick(inCall), [])
        XCTAssertEqual(c.phase, .meetingHold)
    }

    func testSleepingThroughABreakCompletesIt() {
        var c = SessionClock(prefs: prefs())
        _ = c.startBreakNow()
        XCTAssertEqual(c.registerGap(seconds: 600), [.breakEnded(completed: true)])
        XCTAssertEqual(c.phase, .away)
    }

    func testSleepWhilePausedIsIgnored() {
        var c = SessionClock(prefs: prefs())
        c.pause(until: nil)
        XCTAssertEqual(c.registerGap(seconds: 600), [])
        XCTAssertEqual(c.phase, .paused(until: nil))
        XCTAssertEqual(c.resume(), [.resumed])
        XCTAssertEqual(c.resume(), [], "resume only works when paused")
    }

    func testNoExceptionsWhilePausedOrAway() {
        var c = SessionClock(prefs: prefs())
        c.pause(until: nil)
        XCTAssertEqual(c.checkException(minutes: 5), .notNow)
        _ = c.resume()
        _ = c.tick(TickInput(idleSeconds: 200, inMeeting: false))
        XCTAssertEqual(c.phase, .away)
        XCTAssertEqual(c.checkException(minutes: 5), .notNow)
        if case .success = c.grantException(minutes: 5) { XCTFail("should be refused") }
    }

    func testWarningComesAgainAfterAnException() {
        var c = SessionClock(prefs: prefs())
        run(&c, active, times: 540)
        _ = c.grantException(minutes: 5)            // due moves from 600 to 900
        XCTAssertEqual(run(&c, active, times: 299), [])
        XCTAssertEqual(c.tick(active), [.warning(secondsLeft: 60)])
    }

    func testChangingWorkLengthRespectsExceptions() {
        var c = SessionClock(prefs: prefs())
        var p = prefs()
        p.workMinutes = 20
        c.updatePrefs(p)
        XCTAssertEqual(c.secondsUntilBreak, 1200)

        _ = c.grantException(minutes: 5)            // due 1500
        p.workMinutes = 15
        c.updatePrefs(p)
        XCTAssertEqual(c.secondsUntilBreak, 1500, "an exception the user asked for isn't undone")
    }

    func testShorteningBreakLengthShortensRunningBreak() {
        var c = SessionClock(prefs: prefs())
        _ = c.startBreakNow()
        var p = prefs()
        p.breakMinutes = 1
        c.updatePrefs(p)
        XCTAssertEqual(c.phase, .onBreak(remaining: 60))
    }

    func testExceptionsUsedSurviveRelaunch() {
        var c = SessionClock(prefs: prefs())
        c.restoreExceptionsUsed(2)
        XCTAssertEqual(c.exceptionsLeftToday, 0)
        XCTAssertEqual(c.checkException(minutes: 5), .noneLeftToday)
    }

    func testSmokeTestMinuteLengthScalesTheWholeCycle() {
        var c = SessionClock(prefs: prefs(), minuteLength: 1)
        XCTAssertEqual(c.secondsUntilBreak, 10)
        XCTAssertEqual(c.maxExceptionMinutes, 20)
        XCTAssertEqual(run(&c, active, times: 10), [.warning(secondsLeft: 9), .breakStarted])
        XCTAssertEqual(c.phase, .onBreak(remaining: 2))
        XCTAssertEqual(run(&c, active, times: 2), [.breakEnded(completed: true)])
    }

    func testFullDayScenario() {
        // Work, take the break, get an exception in the next cycle, defer for a call, then break.
        var c = SessionClock(prefs: prefs())
        var all: [ClockEvent] = []
        all += run(&c, active, times: 600)          // warning + break
        all += run(&c, active, times: 120)          // break completes
        all += run(&c, active, times: 545)          // warning in cycle 2
        if case .success(let e) = c.grantException(minutes: 5) { all += e }
        all += run(&c, active, times: 354)          // one second before the new due point (900)
        all += run(&c, inCall, times: 5)            // due while on a call → deferred
        all += run(&c, active, times: 11)           // call ends, 10 s grace, break
        XCTAssertEqual(all, [
            .warning(secondsLeft: 60), .breakStarted, .breakEnded(completed: true),
            .warning(secondsLeft: 60), .exceptionGranted(minutes: 5),
            .warning(secondsLeft: 60), .deferredForMeeting,
            .meetingEnded(graceSeconds: 10), .breakStarted,
        ])
        XCTAssertEqual(c.exceptionsLeftToday, 1)
    }
}
