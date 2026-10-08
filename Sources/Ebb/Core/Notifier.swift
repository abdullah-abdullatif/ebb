import Foundation
import UserNotifications

/// Local notifications with quick actions ("Break now", "+10 min").
final class Notifier: NSObject, UNUserNotificationCenterDelegate {
    enum Action: String {
        case breakNow = "EBB_BREAK_NOW"
        case extend = "EBB_EXTEND"
    }

    static let warningCategory = "EBB_WARNING"
    /// Called on the main actor when the user taps an action.
    var onAction: (@MainActor (Action) -> Void)?

    private var center: UNUserNotificationCenter { .current() }
    /// UNUserNotificationCenter crashes outside an .app bundle (e.g. `swift run`, Xcode on the raw binary).
    private let available = Bundle.main.bundleIdentifier != nil

    func setUp() {
        guard available else { return }
        center.delegate = self
        let breakNow = UNNotificationAction(identifier: Action.breakNow.rawValue, title: "Take break now")
        let extend = UNNotificationAction(identifier: Action.extend.rawValue, title: "I'm in flow — ask for more time")
        center.setNotificationCategories([
            UNNotificationCategory(identifier: Self.warningCategory, actions: [breakNow, extend],
                                   intentIdentifiers: [], options: []),
        ])
        center.requestAuthorization(options: [.alert, .sound]) { _, _ in }
    }

    func post(id: String, title: String, body: String, category: String? = nil, sound: Bool = false) {
        guard available else { NSLog("Ebb: \(title) — \(body)"); return }
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        if let category { content.categoryIdentifier = category }
        if sound { content.sound = .default }
        center.removeDeliveredNotifications(withIdentifiers: [id])
        center.add(UNNotificationRequest(identifier: id, content: content, trigger: nil))
    }

    func clear(id: String) {
        guard available else { return }
        center.removeDeliveredNotifications(withIdentifiers: [id])
    }

    // MARK: UNUserNotificationCenterDelegate

    func userNotificationCenter(_ center: UNUserNotificationCenter,
                                willPresent notification: UNNotification,
                                withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void) {
        // Show banners even though Ebb is technically "frontmost" while its menu is open.
        completionHandler([.banner, .sound])
    }

    func userNotificationCenter(_ center: UNUserNotificationCenter,
                                didReceive response: UNNotificationResponse,
                                withCompletionHandler completionHandler: @escaping () -> Void) {
        let action = Action(rawValue: response.actionIdentifier)
        Task { @MainActor in
            if let action { self.onAction?(action) }
            completionHandler()
        }
    }
}
