import SwiftUI

struct SettingsView: View {
    var body: some View {
        TabView {
            RhythmSettings().tabItem { Label("Rhythm", systemImage: "water.waves") }
            ExceptionSettings().tabItem { Label("Exceptions", systemImage: "bolt") }
            MeetingSettings().tabItem { Label("Meetings", systemImage: "video") }
            HistoryView().tabItem { Label("History", systemImage: "chart.bar") }
        }
        .frame(width: 520, height: 460)
    }
}

private struct RhythmSettings: View {
    @EnvironmentObject private var store: PreferencesStore

    var body: some View {
        Form {
            Section("Work & rest") {
                Stepper("Work for \(store.prefs.workMinutes) min", value: $store.prefs.workMinutes, in: 10...180, step: 5)
                Stepper("Break for \(store.prefs.breakMinutes) min", value: $store.prefs.breakMinutes, in: 1...60)
                Stepper("Warn \(store.prefs.warningSeconds / 60) min before", value: $store.prefs.warningSeconds,
                        in: 0...600, step: 60)
            }
            Section {
                Stepper("Away \(store.prefs.naturalBreakMinutes) min counts as a break",
                        value: $store.prefs.naturalBreakMinutes, in: 2...30)
                Stepper("Pause clock after \(store.prefs.idlePauseSeconds) s without input",
                        value: $store.prefs.idlePauseSeconds, in: 15...300, step: 15)
            } header: {
                Text("Activity")
            } footer: {
                Text("Ebb only counts time you're actually using the Mac. Step away long enough and the cycle starts fresh.")
            }
            Section("Enforcement") {
                Toggle("Strict mode", isOn: $store.prefs.strictMode)
                Text("Hides the Dock and menu bar, blocks ⌘-Tab and removes the emergency skip during breaks. Exceptions still work.")
                    .font(.caption).foregroundStyle(.secondary)
                if !store.prefs.strictMode {
                    Stepper("Hold \(Int(store.prefs.emergencySkipHoldSeconds)) s to skip",
                            value: $store.prefs.emergencySkipHoldSeconds, in: 1...10, step: 1)
                }
            }
            Section("System") {
                Toggle("Open Ebb at login", isOn: $store.prefs.launchAtLogin)
                Toggle("Show countdown in menu bar", isOn: $store.prefs.showTimerInMenuBar)
                Button("Restore defaults") { store.resetToDefaults() }
            }
        }
        .formStyle(.grouped)
    }
}

private struct ExceptionSettings: View {
    @EnvironmentObject private var store: PreferencesStore

    var body: some View {
        Form {
            Section {
                Stepper("\(store.prefs.exceptionsPerDay) exceptions per day",
                        value: $store.prefs.exceptionsPerDay, in: 0...10)
                Toggle("Require a reason", isOn: $store.prefs.requireExceptionReason)
                Stepper("Never more than \(store.prefs.hardCapMinutes) min straight",
                        value: $store.prefs.hardCapMinutes, in: 60...300, step: 15)
            } footer: {
                Text("Exceptions push the break back when you're deep in something. The hard limit applies even with exceptions — no one should stare at a screen for three hours.")
            }
        }
        .formStyle(.grouped)
    }
}

private struct MeetingSettings: View {
    @EnvironmentObject private var store: PreferencesStore

    var body: some View {
        Form {
            Section {
                Toggle("Camera is on", isOn: $store.prefs.detectCamera)
                Toggle("Microphone is in use", isOn: $store.prefs.detectMicrophone)
                Toggle("Calendar event happening now", isOn: $store.prefs.detectCalendar)
            } header: {
                Text("Treat me as “in a meeting” when…")
            } footer: {
                Text("Works with Zoom, Meet, Teams, Slack huddles, FaceTime — anything that uses the camera or mic. Ebb never records or opens your devices; it only asks macOS whether they're in use.")
            }
            Section("After the meeting") {
                Stepper("Start the break \(store.prefs.postMeetingGraceSeconds) s after it ends",
                        value: $store.prefs.postMeetingGraceSeconds, in: 0...600, step: 30)
            }
        }
        .formStyle(.grouped)
    }
}

private struct HistoryView: View {
    @EnvironmentObject private var stats: StatsStore

    var body: some View {
        List {
            ForEach(stats.history(), id: \.day) { day in
                Section {
                    HStack {
                        metric("Screen", SessionEngine.format(day.activeSeconds))
                        metric("Meetings", SessionEngine.format(day.meetingSeconds))
                        metric("Breaks", "\(day.breaksCompleted + day.naturalBreaks)")
                        metric("Skipped", "\(day.breaksSkipped)")
                        metric("Balance", "\(day.balanceScore)")
                    }
                    ForEach(day.exceptions) { e in
                        HStack(alignment: .firstTextBaseline) {
                            Text(e.date.formatted(date: .omitted, time: .shortened))
                                .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                            Text("+\(e.minutes) min").font(.caption.weight(.semibold))
                            Text(e.category).font(.caption)
                            if !e.reason.isEmpty {
                                Text("— \(e.reason)").font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                } header: {
                    Text(day.day)
                }
            }
        }
        .overlay {
            if stats.days.isEmpty {
                ContentUnavailableView("No history yet", systemImage: "chart.bar",
                                       description: Text("Your days will show up here."))
            }
        }
    }

    private func metric(_ label: String, _ value: String) -> some View {
        VStack(alignment: .leading) {
            Text(value).font(.body.monospacedDigit())
            Text(label).font(.caption2).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
