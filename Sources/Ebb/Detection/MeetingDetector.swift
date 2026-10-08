import Foundation
import CoreAudio
import CoreMediaIO
import EventKit

/// Why we think you're in a meeting. Shown in the menu so the user can trust the decision.
struct MeetingSignals: Equatable {
    var camera = false
    var microphone = false
    var calendarEvent: String?
    var manual = false

    var inMeeting: Bool { camera || microphone || calendarEvent != nil || manual }

    var summary: String? {
        var parts: [String] = []
        if manual { parts.append("marked by you") }
        if camera { parts.append("camera on") }
        if microphone { parts.append("mic on") }
        if let calendarEvent { parts.append("“\(calendarEvent)”") }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }
}

/// Detects calls and meetings without any special permission for camera/mic:
/// it asks CoreMediaIO / CoreAudio whether *any* process is using a device,
/// it never opens the devices itself.
@MainActor
final class MeetingDetector {
    private let eventStore = EKEventStore()
    private var calendarAuthorized = false
    private var cachedEvent: String?
    private var lastCalendarCheck = Date.distantPast

    var manualOverride = false

    func requestCalendarAccess() async -> Bool {
        do {
            calendarAuthorized = try await eventStore.requestFullAccessToEvents()
        } catch {
            calendarAuthorized = false
        }
        return calendarAuthorized
    }

    func signals(prefs: Preferences, now: Date = Date()) -> MeetingSignals {
        var s = MeetingSignals()
        s.manual = manualOverride
        if prefs.detectCamera { s.camera = Self.isCameraInUse() }
        if prefs.detectMicrophone { s.microphone = Self.isMicrophoneInUse() }
        if prefs.detectCalendar { s.calendarEvent = currentCalendarEvent(now: now) }
        return s
    }

    // MARK: Calendar

    private func currentCalendarEvent(now: Date) -> String? {
        if !calendarAuthorized {
            calendarAuthorized = EKEventStore.authorizationStatus(for: .event) == .fullAccess
            if !calendarAuthorized { return nil }
        }
        // Calendar data changes slowly; once a minute is plenty.
        if now.timeIntervalSince(lastCalendarCheck) < 60 { return cachedEvent }
        lastCalendarCheck = now

        let predicate = eventStore.predicateForEvents(
            withStart: now.addingTimeInterval(-60), end: now.addingTimeInterval(60), calendars: nil)
        let event = eventStore.events(matching: predicate).first { e in
            !e.isAllDay
                && e.availability != .free
                && e.status != .canceled
                && e.startDate <= now && e.endDate > now
                && !Self.declinedByMe(e)
        }
        cachedEvent = event?.title
        return cachedEvent
    }

    private static func declinedByMe(_ event: EKEvent) -> Bool {
        event.attendees?.contains { $0.isCurrentUser && $0.participantStatus == .declined } ?? false
    }

    // MARK: Camera (CoreMediaIO)

    static func isCameraInUse() -> Bool {
        var addr = CMIOObjectPropertyAddress(
            mSelector: CMIOObjectPropertySelector(kCMIOHardwarePropertyDevices),
            mScope: CMIOObjectPropertyScope(kCMIOObjectPropertyScopeGlobal),
            mElement: CMIOObjectPropertyElement(kCMIOObjectPropertyElementMain))
        let system = CMIOObjectID(kCMIOObjectSystemObject)

        var size: UInt32 = 0
        guard CMIOObjectGetPropertyDataSize(system, &addr, 0, nil, &size) == noErr, size > 0 else { return false }
        var devices = [CMIOObjectID](repeating: 0, count: Int(size) / MemoryLayout<CMIOObjectID>.size)
        var used: UInt32 = 0
        guard CMIOObjectGetPropertyData(system, &addr, 0, nil, size, &used, &devices) == noErr else { return false }

        for device in devices {
            var runAddr = CMIOObjectPropertyAddress(
                mSelector: CMIOObjectPropertySelector(kCMIODevicePropertyDeviceIsRunningSomewhere),
                mScope: CMIOObjectPropertyScope(kCMIOObjectPropertyScopeGlobal),
                mElement: CMIOObjectPropertyElement(kCMIOObjectPropertyElementMain))
            var running: UInt32 = 0
            var out: UInt32 = 0
            let status = CMIOObjectGetPropertyData(
                device, &runAddr, 0, nil, UInt32(MemoryLayout<UInt32>.size), &out, &running)
            if status == noErr && running != 0 { return true }
        }
        return false
    }

    // MARK: Microphone (CoreAudio)

    static func isMicrophoneInUse() -> Bool {
        var addr = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDevices,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain)
        let system = AudioObjectID(kAudioObjectSystemObject)

        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(system, &addr, 0, nil, &size) == noErr, size > 0 else { return false }
        var devices = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
        guard AudioObjectGetPropertyData(system, &addr, 0, nil, &size, &devices) == noErr else { return false }

        for device in devices where hasInput(device) {
            var runAddr = AudioObjectPropertyAddress(
                mSelector: kAudioDevicePropertyDeviceIsRunningSomewhere,
                mScope: kAudioObjectPropertyScopeGlobal,
                mElement: kAudioObjectPropertyElementMain)
            var running: UInt32 = 0
            var runSize = UInt32(MemoryLayout<UInt32>.size)
            if AudioObjectGetPropertyData(device, &runAddr, 0, nil, &runSize, &running) == noErr, running != 0 {
                return true
            }
        }
        return false
    }

    /// Only input devices count — speakers run whenever music plays.
    private static func hasInput(_ device: AudioObjectID) -> Bool {
        var addr = AudioObjectPropertyAddress(
            mSelector: kAudioDevicePropertyStreams,
            mScope: kAudioObjectPropertyScopeInput,
            mElement: kAudioObjectPropertyElementMain)
        var size: UInt32 = 0
        return AudioObjectGetPropertyDataSize(device, &addr, 0, nil, &size) == noErr && size > 0
    }
}
