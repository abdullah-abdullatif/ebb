import SwiftUI

struct BreakView: View {
    let isPrimary: Bool
    @EnvironmentObject private var engine: SessionEngine
    @State private var showingRequest = false

    private static let tips = [
        "Look at something 6 metres away for 20 seconds. Your eyes will thank you.",
        "Stand up. Roll your shoulders back five times.",
        "Drink a full glass of water.",
        "Walk to a window. Let your eyes rest on the horizon.",
        "Stretch your wrists: palms out, fingers back, hold 15 seconds.",
        "Unclench your jaw. Drop your shoulders. Breathe out slowly.",
        "Step outside if you can — daylight resets your focus.",
    ]

    private var remaining: Int {
        if case .onBreak(let r) = engine.clock.phase { return r }
        return 0
    }

    var body: some View {
        ZStack {
            Theme.breakGradient.ignoresSafeArea()
            if isPrimary {
                VStack(spacing: 36) {
                    Spacer()
                    BreathingOrb()
                    VStack(spacing: 10) {
                        Text("Time to ebb")
                            .font(.system(size: 44, weight: .semibold, design: .rounded))
                        Text(SessionEngine.format(remaining))
                            .font(.system(size: 72, weight: .light, design: .rounded))
                            .monospacedDigit()
                            .contentTransition(.numericText(countsDown: true))
                        Text(tip)
                            .font(.title3)
                            .multilineTextAlignment(.center)
                            .frame(maxWidth: 520)
                            .opacity(0.85)
                    }
                    Spacer()
                    if showingRequest {
                        ExceptionRequestView(onCancel: { showingRequest = false })
                            .padding(24)
                            .frame(width: 440)
                            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 20))
                            .transition(.move(edge: .bottom).combined(with: .opacity))
                    } else {
                        actions
                    }
                    Spacer().frame(height: 40)
                }
                .foregroundStyle(.white)
                .animation(.easeInOut(duration: 0.25), value: showingRequest)
            } else {
                VStack(spacing: 12) {
                    Image(systemName: "cup.and.saucer.fill").font(.system(size: 48))
                    Text(SessionEngine.format(remaining))
                        .font(.system(size: 56, weight: .light, design: .rounded))
                        .monospacedDigit()
                }
                .foregroundStyle(.white.opacity(0.9))
            }
        }
    }

    private var tip: String {
        // New tip every 30 s.
        let index = (engine.prefs.breakSeconds - remaining) / 30
        return Self.tips[index % Self.tips.count]
    }

    @ViewBuilder private var actions: some View {
        let left = engine.clock.exceptionsLeftToday
        HStack(spacing: 16) {
            Button {
                showingRequest = true
            } label: {
                Label(left > 0 ? "I need more time (\(left) left today)" : "No exceptions left today",
                      systemImage: "bolt.fill")
                    .padding(.horizontal, 18).padding(.vertical, 10)
            }
            .buttonStyle(.plain)
            .background(.white.opacity(0.16), in: Capsule())
            .disabled(left == 0 || engine.clock.maxExceptionMinutes < 1)
            .opacity(left == 0 ? 0.5 : 1)

            if !engine.prefs.strictMode {
                HoldToSkipButton(duration: engine.prefs.emergencySkipHoldSeconds) {
                    engine.emergencySkip()
                }
            }
        }
        .font(.callout.weight(.medium))
    }
}

/// Slow 4-in / 6-out breathing guide.
struct BreathingOrb: View {
    var body: some View {
        TimelineView(.animation) { context in
            let t = context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 10)
            let inhaling = t < 4
            let progress = inhaling ? t / 4 : 1 - (t - 4) / 6
            let eased = 0.5 - cos(progress * .pi) / 2
            ZStack {
                Circle()
                    .fill(RadialGradient(colors: [Theme.sun.opacity(0.9), Theme.sun.opacity(0.15)],
                                         center: .center, startRadius: 4, endRadius: 120))
                    .frame(width: 220, height: 220)
                    .scaleEffect(0.6 + 0.4 * eased)
                    .blur(radius: 2)
                Text(inhaling ? "breathe in" : "breathe out")
                    .font(.headline)
                    .foregroundStyle(Theme.night.opacity(0.8))
            }
        }
        .frame(width: 240, height: 240)
    }
}

/// Deliberately slow escape hatch: you have to mean it.
struct HoldToSkipButton: View {
    let duration: Double
    let action: () -> Void
    @State private var pressing = false

    var body: some View {
        Text(pressing ? "Keep holding…" : "Hold to skip")
            .padding(.horizontal, 18).padding(.vertical, 10)
            .background(alignment: .leading) {
                GeometryReader { geo in
                    Capsule()
                        .fill(.white.opacity(0.28))
                        .frame(width: pressing ? geo.size.width : 0)
                        .animation(pressing ? .linear(duration: duration) : .easeOut(duration: 0.2),
                                   value: pressing)
                }
            }
            .background(.white.opacity(0.08), in: Capsule())
            .clipShape(Capsule())
            .onLongPressGesture(minimumDuration: duration, pressing: { pressing = $0 }, perform: action)
            .opacity(0.75)
    }
}
