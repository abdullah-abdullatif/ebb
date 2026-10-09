import XCTest
@testable import Ebb

final class PreferencesStoreTests: XCTestCase {
    private func freshDefaults() -> UserDefaults {
        UserDefaults(suiteName: "ebb.tests.\(UUID().uuidString)")!
    }

    func testDefaults() {
        let p = PreferencesStore(defaults: freshDefaults()).prefs
        XCTAssertEqual(p.workMinutes, 50)
        XCTAssertEqual(p.breakMinutes, 10)
        XCTAssertEqual(p.exceptionsPerDay, 3)
        XCTAssertEqual(p.hardCapMinutes, 120)
        XCTAssertFalse(p.strictMode)
        XCTAssertTrue(p.detectCamera && p.detectMicrophone)
        XCTAssertFalse(p.detectCalendar)
    }

    func testChangesAreSavedAndReloaded() {
        let defaults = freshDefaults()
        let store = PreferencesStore(defaults: defaults)
        store.prefs.workMinutes = 25
        store.prefs.strictMode = true
        let reloaded = PreferencesStore(defaults: defaults).prefs
        XCTAssertEqual(reloaded.workMinutes, 25)
        XCTAssertTrue(reloaded.strictMode)
    }

    func testResetToDefaults() {
        let defaults = freshDefaults()
        let store = PreferencesStore(defaults: defaults)
        store.prefs.breakMinutes = 30
        store.resetToDefaults()
        XCTAssertEqual(PreferencesStore(defaults: defaults).prefs, Preferences())
    }

    func testCorruptSettingsFallBackToDefaults() {
        let defaults = freshDefaults()
        defaults.set(Data("not json".utf8), forKey: "ebb.preferences.v1")
        XCTAssertEqual(PreferencesStore(defaults: defaults).prefs, Preferences())
    }
}

final class StatsStoreTests: XCTestCase {
    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory.appendingPathComponent("ebb-tests-\(UUID().uuidString)")
    }

    @MainActor
    func testTodayIsSavedAndReloaded() {
        let dir = tempDir()
        let store = StatsStore(directory: dir)
        store.update { d in
            d.breaksCompleted += 2
            d.activeSeconds += 90
            d.exceptions.append(ExceptionRecord(date: Date(), minutes: 10, reason: "deck", category: "Deadline"))
        }
        store.saveIfNeeded()

        let reloaded = StatsStore(directory: dir).today
        XCTAssertEqual(reloaded.breaksCompleted, 2)
        XCTAssertEqual(reloaded.activeSeconds, 90)
        XCTAssertEqual(reloaded.exceptions.first?.reason, "deck")
        XCTAssertEqual(reloaded.exceptions.first?.minutes, 10)
    }

    @MainActor
    func testNothingIsWrittenWithoutChanges() {
        let dir = tempDir()
        StatsStore(directory: dir).saveIfNeeded()
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("stats.json").path))
    }

    @MainActor
    func testHistoryIsNewestFirstAndLimited() throws {
        let dir = tempDir()
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let days = ["2026-10-01", "2026-10-03", "2026-10-02"].reduce(into: [String: DayStats]()) {
            $0[$1] = DayStats(day: $1)
        }
        try JSONEncoder().encode(days).write(to: dir.appendingPathComponent("stats.json"))

        XCTAssertEqual(StatsStore(directory: dir).history(limit: 2).map(\.day), ["2026-10-03", "2026-10-02"])
    }

    @MainActor
    func testCorruptHistoryStartsEmpty() throws {
        let dir = tempDir()
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Data("{oops".utf8).write(to: dir.appendingPathComponent("stats.json"))
        XCTAssertTrue(StatsStore(directory: dir).days.isEmpty)
    }

    func testDayKeyIsGregorianLocalDate() {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = .current
        let date = cal.date(from: DateComponents(year: 2026, month: 1, day: 5, hour: 23, minute: 30))!
        XCTAssertEqual(StatsStore.key(for: date), "2026-01-05")
    }

    func testBalanceScore() {
        var d = DayStats(day: "x")
        XCTAssertEqual(d.balanceScore, 0, "no activity, no score")
        d.activeSeconds = 10
        XCTAssertEqual(d.balanceScore, 100, "worked but no break due yet")
        d.breaksCompleted = 3
        d.naturalBreaks = 1
        d.breaksSkipped = 1
        XCTAssertEqual(d.balanceScore, 80)
        d.exceptions = (0..<2).map { _ in ExceptionRecord(date: Date(), minutes: 5, reason: "", category: "Other") }
        XCTAssertEqual(d.balanceScore, 70, "5 points per exception")
        d.breaksCompleted = 0
        d.naturalBreaks = 0
        d.breaksSkipped = 5
        XCTAssertEqual(d.balanceScore, 0, "never below zero")
    }
}

final class MeetingSignalsTests: XCTestCase {
    func testNoSignalsMeansNoMeeting() {
        let s = MeetingSignals()
        XCTAssertFalse(s.inMeeting)
        XCTAssertNil(s.summary)
    }

    func testAnySignalMeansMeeting() {
        XCTAssertTrue(MeetingSignals(camera: true).inMeeting)
        XCTAssertTrue(MeetingSignals(microphone: true).inMeeting)
        XCTAssertTrue(MeetingSignals(calendarEvent: "Standup").inMeeting)
        XCTAssertTrue(MeetingSignals(manual: true).inMeeting)
    }

    func testSummaryExplainsWhy() {
        XCTAssertEqual(MeetingSignals(camera: true, microphone: true).summary, "camera on · mic on")
        XCTAssertEqual(MeetingSignals(calendarEvent: "Standup", manual: true).summary, "marked by you · “Standup”")
    }
}

final class FormattingTests: XCTestCase {
    @MainActor
    func testCountdownFormat() {
        XCTAssertEqual(SessionEngine.format(0), "0:00")
        XCTAssertEqual(SessionEngine.format(65), "1:05")
        XCTAssertEqual(SessionEngine.format(3725), "1:02:05")
        XCTAssertEqual(SessionEngine.format(-5), "0:00")
    }
}
