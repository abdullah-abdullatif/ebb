// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "Ebb",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "Ebb",
            path: "Sources/Ebb",
            linkerSettings: [
                .linkedFramework("CoreMediaIO"),
                .linkedFramework("CoreAudio"),
                .linkedFramework("EventKit"),
                .linkedFramework("ServiceManagement"),
                .linkedFramework("UserNotifications"),
            ]
        ),
        .testTarget(
            name: "EbbTests",
            dependencies: ["Ebb"],
            path: "Tests/EbbTests"
        ),
    ]
)
