import SwiftUI

@main
struct EbbApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var app

    var body: some Scene {
        MenuBarExtra {
            MenuBarView()
                .environmentObject(app.engine)
                .environmentObject(app.stats)
        } label: {
            MenuBarLabel(engine: app.engine)
        }
        .menuBarExtraStyle(.window)

        Settings {
            SettingsView()
                .environmentObject(app.prefs)
                .environmentObject(app.stats)
        }
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let prefs = PreferencesStore()
    let stats = StatsStore()
    lazy var engine = SessionEngine(prefsStore: prefs, stats: stats)
    private var activity: NSObjectProtocol?

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Keep the 1-second clock honest: App Nap would otherwise throttle a menu-bar-only app.
        activity = ProcessInfo.processInfo.beginActivity(
            options: [.userInitiatedAllowingIdleSystemSleep],
            reason: "Tracking work and break time")
        engine.start()
    }

    func applicationWillTerminate(_ notification: Notification) {
        engine.shutdown()
    }
}
