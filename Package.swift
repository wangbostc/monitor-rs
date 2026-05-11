// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "MonitorRSApp",
    platforms: [.macOS(.v14)],
    targets: [
        .target(
            name: "MonitorRSC",
            path: "Sources/MonitorRSC",
            publicHeadersPath: "include"
        ),
        .target(
            name: "MonitorRSLogic",
            path: "Sources/MonitorRSLogic"
        ),
        .testTarget(
            name: "MonitorRSLogicTests",
            dependencies: ["MonitorRSLogic"],
            path: "Tests/MonitorRSLogicTests"
        ),
        .executableTarget(
            name: "MonitorRSApp",
            dependencies: ["MonitorRSC", "MonitorRSLogic"],
            path: "Sources/MonitorRSApp",
            linkerSettings: [
                .linkedLibrary("monitor_rs"),
                .unsafeFlags(["-L", "target/release"])
            ]
        )
    ]
)
