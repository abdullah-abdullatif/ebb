import AppKit
import SwiftUI

/// Opens Settings in a window Ebb owns. SwiftUI's `SettingsLink` is unreliable in a
/// menu-bar-only (LSUIElement) app: the window often never appears or opens behind others.
@MainActor
final class SettingsWindowController: NSObject, NSWindowDelegate {
    private let prefs: PreferencesStore
    private let stats: StatsStore
    private var window: NSWindow?

    init(prefs: PreferencesStore, stats: StatsStore) {
        self.prefs = prefs
        self.stats = stats
    }

    func show() {
        if window == nil {
            let hosting = NSHostingController(rootView: SettingsView()
                .environmentObject(prefs)
                .environmentObject(stats))
            let w = NSWindow(contentViewController: hosting)
            w.title = "Ebb Settings"
            w.styleMask = [.titled, .closable, .miniaturizable]
            w.isReleasedWhenClosed = false
            w.delegate = self
            w.center()
            window = w
        }
        // A menu-bar-only app can't bring a window forward; become a regular app while it's open.
        NSApp.setActivationPolicy(.regular)
        DispatchQueue.main.async { [weak self] in
            NSApp.activate(ignoringOtherApps: true)
            self?.window?.makeKeyAndOrderFront(nil)
        }
    }

    func windowWillClose(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
    }
}
