import CoreGraphics

/// How long since the last keyboard, mouse or trackpad input, system-wide.
/// Uses the HID idle counter, so no Accessibility or Input Monitoring permission is needed.
enum ActivityMonitor {
    static func idleSeconds() -> Double {
        // kCGAnyInputEventType (~0) = any kind of input event.
        guard let anyInput = CGEventType(rawValue: ~0) else { return 0 }
        return CGEventSource.secondsSinceLastEventType(.combinedSessionState, eventType: anyInput)
    }
}
