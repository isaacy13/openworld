// swift-tools-version: 5.10
// SPDX-License-Identifier: Apache-2.0
import Foundation
import PackageDescription

// OpenWorldKit is the app's model and core bridge. It builds and tests on Linux.
// The SwiftUI app, its decoders, and the share extension are in project.yml.
//
// OPENWORLD_LIB_DIR names a folder holding libopenworld_core (cargo build -p openworld-core).
// When it is set, the kit links the real C API and the integration tests run against it.
let libDir = ProcessInfo.processInfo.environment["OPENWORLD_LIB_DIR"].flatMap { $0.isEmpty ? nil : $0 }

let strict: [SwiftSetting] = [.enableUpcomingFeature("StrictConcurrency")]
var targets: [Target] = []
var kitDependencies: [Target.Dependency] = []
var linkFlags: [LinkerSetting] = []
if let libDir {
    targets.append(.systemLibrary(name: "OpenWorldCore", path: "Sources/OpenWorldCore"))
    kitDependencies.append("OpenWorldCore")
    linkFlags = [.unsafeFlags(["-L\(libDir)", "-Xlinker", "-rpath", "-Xlinker", libDir])]
}
targets += [
    .target(
        name: "OpenWorldKit",
        dependencies: kitDependencies,
        path: "Sources/OpenWorldKit",
        swiftSettings: strict,
        linkerSettings: linkFlags
    ),
    .testTarget(
        name: "OpenWorldKitTests",
        dependencies: ["OpenWorldKit"],
        path: "Tests/OpenWorldKitTests",
        swiftSettings: strict,
        linkerSettings: linkFlags
    ),
]

let package = Package(
    name: "OpenWorldKit",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [.library(name: "OpenWorldKit", targets: ["OpenWorldKit"])],
    targets: targets
)
