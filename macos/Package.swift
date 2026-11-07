// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "PurrMacOS",
    platforms: [
        .macOS(.v14)
    ],
    targets: [
        .systemLibrary(
            name: "PurrCore",
            path: "Sources/PurrCore"
        ),
        .executableTarget(
            name: "PurrMacOS",
            dependencies: ["PurrCore"],
            path: "Sources/PurrMacOS",
            linkerSettings: [
                .unsafeFlags(["-L", Context.packageDirectory + "/../target/release", "-lpurr_core"]),
                .linkedFramework("AppKit"),
                .linkedFramework("SwiftUI"),
                .linkedFramework("AVFoundation"),
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("Security")
            ]
        )
    ]
)
