import Foundation
import Combine

/// Everything the user can tune. Stored as one JSON blob in UserDefaults.
struct Preferences: Codable, Equatable {
    // Rhythm
    var workMinutes: Int = 50
    var breakMinutes: Int = 10
    /// Heads-up before a break starts.
    var warningSeconds: Int = 120
    /// Being away from keyboard/mouse this long counts as a break you took yourself.
    var naturalBreakMinutes: Int = 5
    /// No input for this long and the work clock pauses (reading, thinking).
    var idlePauseSeconds: Int = 60

    // Enforcement
    /// Strict: hides Dock/menu bar and blocks app switching during a break; no emergency skip.
    var strictMode: Bool = false
    /// Hold the skip button this long to escape a break (non-strict only).
    var emergencySkipHoldSeconds: Double = 3

    // Exceptions ("I'm in the middle of something")
    var exceptionsPerDay: Int = 3
    var exceptionChoicesMinutes: [Int] = [5, 10, 15, 25]
    var requireExceptionReason: Bool = true
    /// Never allow more than this much continuous screen work, exceptions included.
    var hardCapMinutes: Int = 120

    // Meetings
    var detectCamera: Bool = true
    var detectMicrophone: Bool = true
    var detectCalendar: Bool = false
    /// After a meeting ends, wait this long before the deferred break starts.
    var postMeetingGraceSeconds: Int = 90

    // System
    var launchAtLogin: Bool = false
    var showTimerInMenuBar: Bool = true

    var workSeconds: Int { workMinutes * 60 }
    var breakSeconds: Int { breakMinutes * 60 }
    var naturalBreakSeconds: Int { naturalBreakMinutes * 60 }
    var hardCapSeconds: Int { hardCapMinutes * 60 }
}

final class PreferencesStore: ObservableObject {
    private static let key = "ebb.preferences.v1"
    private let defaults: UserDefaults

    @Published var prefs: Preferences {
        didSet {
            guard prefs != oldValue else { return }
            if let data = try? JSONEncoder().encode(prefs) {
                defaults.set(data, forKey: Self.key)
            }
        }
    }

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        if let data = defaults.data(forKey: Self.key),
           let saved = try? JSONDecoder().decode(Preferences.self, from: data) {
            prefs = saved
        } else {
            prefs = Preferences()
        }
    }

    func resetToDefaults() { prefs = Preferences() }
}
