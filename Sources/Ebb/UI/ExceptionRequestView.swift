import SwiftUI

/// "I'm in the middle of something." Asking costs a little friction on purpose:
/// pick how long, say why, and it comes out of a small daily budget.
struct ExceptionRequestView: View {
    var onCancel: () -> Void
    @EnvironmentObject private var engine: SessionEngine

    @State private var minutes: Int?
    @State private var category = Self.categories[0]
    @State private var reason = ""

    static let categories = ["Deep work / in flow", "Deadline", "Joining a meeting", "Finishing a call", "Other"]

    private var reasonOK: Bool {
        !engine.prefs.requireExceptionReason || reason.trimmingCharacters(in: .whitespaces).count >= 3
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text("Ask for more time").font(.title2.weight(.semibold))
                Spacer()
                Text("\(engine.clock.exceptionsLeftToday) of \(engine.prefs.exceptionsPerDay) left today")
                    .font(.caption).foregroundStyle(.secondary)
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("How long?").font(.subheadline.weight(.medium))
                HStack {
                    ForEach(engine.prefs.exceptionChoicesMinutes, id: \.self) { m in
                        let allowed = engine.exceptionDenial(minutes: m) == nil
                        Button("\(m) min") { minutes = m }
                            .buttonStyle(.bordered)
                            .tint(minutes == m ? Theme.tide : nil)
                            .disabled(!allowed)
                    }
                }
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("Why?").font(.subheadline.weight(.medium))
                Picker("Why", selection: $category) {
                    ForEach(Self.categories, id: \.self) { Text($0) }
                }
                .labelsHidden()
                TextField(engine.prefs.requireExceptionReason ? "One line about what you're finishing (required)"
                                                              : "One line about what you're finishing",
                          text: $reason)
                    .textFieldStyle(.roundedBorder)
            }

            if let message = denialMessage {
                Label(message, systemImage: "exclamationmark.triangle.fill")
                    .font(.callout).foregroundStyle(.orange)
            }

            HStack {
                Button("Never mind", action: onCancel)
                    .keyboardShortcut(.cancelAction)
                Spacer()
                Button("Grant me \(minutes.map { "\($0) min" } ?? "time")") {
                    guard let minutes else { return }
                    if engine.requestException(minutes: minutes, category: category, reason: reason) {
                        onCancel()
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(minutes == nil || !reasonOK)
            }
        }
    }

    private var denialMessage: String? {
        if engine.clock.exceptionsLeftToday == 0 {
            return "You've used all of today's exceptions. Take the break — the work will still be there."
        }
        if engine.clock.maxExceptionMinutes < (engine.prefs.exceptionChoicesMinutes.min() ?? 1) {
            return "You've hit the \(engine.prefs.hardCapMinutes)-minute limit of continuous work."
        }
        switch engine.lastDenial {
        case .hardCap(let max)?: return "Only \(max) more minutes allowed before the hard limit."
        case .noneLeftToday?: return "No exceptions left today."
        case .notNow?: return "Nothing to extend right now."
        case nil: return nil
        }
    }
}
