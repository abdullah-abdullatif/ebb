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
}
