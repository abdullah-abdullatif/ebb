import SwiftUI

struct MenuBarLabel: View {
    @ObservedObject var engine: SessionEngine

    var body: some View {
        let phase = engine.clock.phase
        HStack(spacing: 4) {
            Image(systemName: Theme.symbol(for: phase, inMeeting: engine.meeting.inMeeting))
            if engine.prefs.showTimerInMenuBar, let text = timerText {
                Text(text).monospacedDigit()
            }
        }
    }

    private var timerText: String? {
        switch engine.clock.phase {
        case .working, .warning, .idle: return SessionEngine.format(engine.clock.secondsUntilBreak)
        case .onBreak(let r), .postMeetingGrace(let r): return SessionEngine.format(r)
        case .meetingHold, .away, .paused: return nil
        }
    }
}

struct MenuBarView: View {
    var openSettings: () -> Void
    @EnvironmentObject private var engine: SessionEngine
    @EnvironmentObject private var stats: StatsStore

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            header
            if let why = engine.meeting.summary {
                Label("In a meeting: \(why)", systemImage: "video.fill")
                    .font(.caption).foregroundStyle(.purple)
            }
            Divider()
            actions
            Divider()
            today
            Divider()
            footer
        }
        .padding(16)
        .frame(width: 300)
    }

    // MARK: Header

    private var header: some View {
        HStack(spacing: 14) {
            ZStack {
                Circle().stroke(.quaternary, lineWidth: 6)
                Circle()
                    .trim(from: 0, to: engine.clock.cycleProgress)
                    .stroke(Theme.color(for: engine.clock.phase),
                            style: StrokeStyle(lineWidth: 6, lineCap: .round))
                    .rotationEffect(.degrees(-90))
                Image(systemName: Theme.symbol(for: engine.clock.phase, inMeeting: engine.meeting.inMeeting))
                    .foregroundStyle(Theme.color(for: engine.clock.phase))
            }
            .frame(width: 52, height: 52)

            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.headline)
                Text(subtitle).font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    private var title: String {
        let c = engine.clock
        switch c.phase {
        case .working, .warning: return "Break in \(SessionEngine.format(c.secondsUntilBreak))"
        case .idle: return "Paused — no activity"
        case .away: return "Welcome back"
        case .meetingHold: return "Break waiting for your meeting"
        case .postMeetingGrace(let r): return "Break in \(SessionEngine.format(r))"
        case .onBreak(let r): return "On break · \(SessionEngine.format(r))"
        case .paused(let until):
            if let until { return "Paused until \(until.formatted(date: .omitted, time: .shortened))" }
            return "Paused"
        }
    }

    private var subtitle: String {
        "Focused for \(SessionEngine.format(engine.clock.workedSeconds)) this stretch"
    }

    // MARK: Actions

    @ViewBuilder private var actions: some View {
        let phase = engine.clock.phase
        VStack(alignment: .leading, spacing: 6) {
            if case .paused = phase {
                MenuButton("Resume", symbol: "play.fill") { engine.resume() }
            } else {
                MenuButton("Take a break now", symbol: "cup.and.saucer") { engine.startBreakNow() }
                MenuButton("I'm in flow — ask for more time…", symbol: "bolt") {
                    engine.openExceptionRequest()
                }
                .disabled(engine.clock.exceptionsLeftToday == 0)
                Toggle(isOn: Binding(get: { engine.meeting.manual },
                                     set: { engine.setManualMeeting($0) })) {
                    Label("I'm in a meeting (no camera/mic)", systemImage: "person.2")
                }
                .toggleStyle(.checkbox)
                Menu {
                    Button("30 minutes") { engine.pause(minutes: 30) }
                    Button("1 hour") { engine.pause(minutes: 60) }
                    Button("2 hours") { engine.pause(minutes: 120) }
                    Button("Until I resume") { engine.pause(minutes: nil) }
                } label: {
                    Label("Pause Ebb", systemImage: "pause")
                }
                .menuStyle(.button)
                .buttonStyle(.borderless)
            }
        }
    }

    // MARK: Today

    private var today: some View {
        let d = stats.today
        return VStack(alignment: .leading, spacing: 6) {
            Text("Today").font(.subheadline.weight(.semibold))
            Grid(alignment: .leading, horizontalSpacing: 18, verticalSpacing: 4) {
                GridRow {
                    Stat("Screen time", SessionEngine.format(d.activeSeconds))
                    Stat("Breaks", "\(d.breaksCompleted + d.naturalBreaks)")
                }
                GridRow {
                    Stat("Skipped", "\(d.breaksSkipped)")
                    Stat("Exceptions left", "\(engine.clock.exceptionsLeftToday)")
                }
            }
        }
    }

    private var footer: some View {
        HStack {
            Button("Settings…", action: openSettings)
            Spacer()
            Button("Quit Ebb") { NSApp.terminate(nil) }
        }
        .buttonStyle(.borderless)
    }
}

private struct MenuButton: View {
    let title: String
    let symbol: String
    let action: () -> Void
    init(_ title: String, symbol: String, action: @escaping () -> Void) {
        self.title = title; self.symbol = symbol; self.action = action
    }
    var body: some View {
        Button(action: action) {
            Label(title, systemImage: symbol).frame(maxWidth: .infinity, alignment: .leading)
        }
        .buttonStyle(.borderless)
    }
}

private struct Stat: View {
    let label: String
    let value: String
    init(_ label: String, _ value: String) { self.label = label; self.value = value }
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(value).font(.body.monospacedDigit().weight(.medium))
            Text(label).font(.caption2).foregroundStyle(.secondary)
        }
    }
}
