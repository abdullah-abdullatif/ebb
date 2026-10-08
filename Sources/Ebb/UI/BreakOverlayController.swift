import AppKit
import SwiftUI

/// Borderless windows can't take keyboard focus by default; the exception form needs it.
private final class OverlayWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}

/// Covers every display with the break screen, and hosts the floating
/// "ask for more time" panel when the request comes from outside a break.
@MainActor
final class BreakOverlayController {
    private unowned let engine: SessionEngine
    private var windows: [NSWindow] = []
    private var screenObserver: NSObjectProtocol?
    private var requestPanel: NSPanel?

    init(engine: SessionEngine) {
        self.engine = engine
    }

    var isShowing: Bool { !windows.isEmpty }

    func show(strict: Bool) {
        hideExceptionRequest()
        buildWindows()
        NSApp.activate(ignoringOtherApps: true)
        if strict {
            // Hide Dock + menu bar and block ⌘-Tab while the break runs.
            NSApp.presentationOptions = [.hideDock, .hideMenuBar, .disableProcessSwitching,
                                         .disableHideApplication, .disableSessionTermination]
        }
        if screenObserver == nil {
            screenObserver = NotificationCenter.default.addObserver(
                forName: NSApplication.didChangeScreenParametersNotification, object: nil, queue: .main
            ) { [weak self] _ in
                Task { @MainActor in
                    guard let self, self.isShowing else { return }
                    self.buildWindows()
                }
            }
        }
    }

    func hide() {
        windows.forEach { $0.orderOut(nil) }
        windows.removeAll()
        NSApp.presentationOptions = []
        if let screenObserver { NotificationCenter.default.removeObserver(screenObserver) }
        screenObserver = nil
    }

    private func buildWindows() {
        windows.forEach { $0.orderOut(nil) }
        windows.removeAll()
        let primary = NSScreen.main ?? NSScreen.screens.first
        for screen in NSScreen.screens {
            let isPrimary = screen == primary
            let window = OverlayWindow(contentRect: screen.frame, styleMask: [.borderless],
                                       backing: .buffered, defer: false)
            window.setFrame(screen.frame, display: false)
            window.level = .screenSaver
            window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
            window.isOpaque = false
            window.backgroundColor = .clear
            window.hasShadow = false
            window.isReleasedWhenClosed = false
            window.contentView = Self.blurredHost(
                BreakView(isPrimary: isPrimary).environmentObject(engine))
            window.makeKeyAndOrderFront(nil)
            windows.append(window)
        }
        windows.first { $0.screen == primary }?.makeKey()
    }

    private static func blurredHost<V: View>(_ view: V) -> NSView {
        let blur = NSVisualEffectView()
        blur.material = .fullScreenUI
        blur.blendingMode = .behindWindow
        blur.state = .active
        let host = NSHostingView(rootView: view)
        host.translatesAutoresizingMaskIntoConstraints = false
        blur.addSubview(host)
        NSLayoutConstraint.activate([
            host.leadingAnchor.constraint(equalTo: blur.leadingAnchor),
            host.trailingAnchor.constraint(equalTo: blur.trailingAnchor),
            host.topAnchor.constraint(equalTo: blur.topAnchor),
            host.bottomAnchor.constraint(equalTo: blur.bottomAnchor),
        ])
        return blur
    }

    // MARK: Exception request panel (outside a break)

    func showExceptionRequest() {
        if isShowing { return } // the break screen has its own form
        if let requestPanel {
            requestPanel.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            return
        }
        let panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 420, height: 440),
                            styleMask: [.titled, .closable, .nonactivatingPanel, .fullSizeContentView],
                            backing: .buffered, defer: false)
        panel.title = "Ask for more time"
        panel.titlebarAppearsTransparent = true
        panel.isFloatingPanel = true
        panel.level = .floating
        panel.isReleasedWhenClosed = false
        panel.contentView = NSHostingView(rootView:
            ExceptionRequestView(onCancel: { [weak self] in self?.hideExceptionRequest() })
                .environmentObject(engine)
                .padding(20)
                .frame(width: 420))
        panel.center()
        panel.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        requestPanel = panel
    }

    func hideExceptionRequest() {
        requestPanel?.close()
        requestPanel = nil
    }
}
