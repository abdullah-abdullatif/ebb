import Foundation

struct ExceptionRecord: Codable, Identifiable, Equatable {
    var id = UUID()
    var date: Date
    var minutes: Int
    var reason: String
    var category: String
}

struct DayStats: Codable, Equatable {
    var day: String
    var activeSeconds = 0
    var meetingSeconds = 0
    var breaksCompleted = 0
    var breaksSkipped = 0
    var naturalBreaks = 0
    var meetingDeferrals = 0
    var longestStretchSeconds = 0
    var exceptions: [ExceptionRecord] = []

    /// 0–100. Rewards taking breaks, penalises skipping and leaning on exceptions.
    var balanceScore: Int {
        let taken = breaksCompleted + naturalBreaks
        let total = taken + breaksSkipped
        guard total > 0 else { return activeSeconds > 0 ? 100 : 0 }
        let base = Double(taken) / Double(total) * 100
        let penalty = Double(exceptions.count) * 5
        return Int(max(0, min(100, base - penalty)))
    }
}

/// Daily history in ~/Library/Application Support/Ebb/stats.json.
@MainActor
final class StatsStore: ObservableObject {
    @Published private(set) var days: [String: DayStats] = [:]
    private let fileURL: URL
    private var dirty = false

    init(directory: URL? = nil) {
        let dir = directory ?? FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Ebb", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        fileURL = dir.appendingPathComponent("stats.json")
        if let data = try? Data(contentsOf: fileURL),
           let decoded = try? JSONDecoder.iso.decode([String: DayStats].self, from: data) {
            days = decoded
        }
    }

    /// Local calendar day, e.g. "2026-10-08".
    nonisolated static func key(for date: Date = Date()) -> String {
        // Gregorian on purpose: keys must sort the same whatever calendar the user's Mac uses.
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = .current
        let c = cal.dateComponents([.year, .month, .day], from: date)
        return String(format: "%04d-%02d-%02d", c.year ?? 0, c.month ?? 0, c.day ?? 0)
    }

    var today: DayStats { days[Self.key()] ?? DayStats(day: Self.key()) }

    /// Most recent first.
    func history(limit: Int = 14) -> [DayStats] {
        days.values.sorted { $0.day > $1.day }.prefix(limit).map { $0 }
    }

    func update(_ change: (inout DayStats) -> Void) {
        let k = Self.key()
        var d = days[k] ?? DayStats(day: k)
        change(&d)
        days[k] = d
        dirty = true
    }

    func saveIfNeeded() {
        guard dirty else { return }
        dirty = false
        if let data = try? JSONEncoder.iso.encode(days) {
            try? data.write(to: fileURL, options: .atomic)
        }
    }
}

private extension JSONEncoder {
    static let iso: JSONEncoder = {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        e.outputFormatting = [.prettyPrinted, .sortedKeys]
        return e
    }()
}

private extension JSONDecoder {
    static let iso: JSONDecoder = {
        let d = JSONDecoder()
        d.dateDecodingStrategy = .iso8601
        return d
    }()
}
