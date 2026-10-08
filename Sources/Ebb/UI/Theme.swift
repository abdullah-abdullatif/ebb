import SwiftUI

/// Colours lifted from the app icon: night sky → tide.
enum Theme {
    static let night = Color(red: 0.10, green: 0.17, blue: 0.36)
    static let dusk = Color(red: 0.17, green: 0.31, blue: 0.53)
    static let tide = Color(red: 0.18, green: 0.55, blue: 0.56)
    static let foam = Color(red: 0.50, green: 0.88, blue: 0.84)
    static let sun = Color(red: 1.00, green: 0.79, blue: 0.54)

    static let breakGradient = LinearGradient(
        colors: [night.opacity(0.92), dusk.opacity(0.88), tide.opacity(0.85)],
        startPoint: .top, endPoint: .bottom)

    static func color(for phase: Phase) -> Color {
        switch phase {
        case .working: return tide
        case .warning, .postMeetingGrace: return .orange
        case .meetingHold: return .purple
        case .onBreak: return sun
        case .idle, .away, .paused: return .secondary
        }
    }

    static func symbol(for phase: Phase, inMeeting: Bool) -> String {
        switch phase {
        case .working: return inMeeting ? "video.fill" : "water.waves"
        case .warning: return "hourglass"
        case .meetingHold, .postMeetingGrace: return "video.badge.checkmark"
        case .onBreak: return "cup.and.saucer.fill"
        case .idle: return "water.waves"
        case .away: return "moon.zzz.fill"
        case .paused: return "pause.circle"
        }
    }
}
