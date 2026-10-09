// swift-tools-version: 5.9
// SPDX-License-Identifier: Apache-2.0
import PackageDescription

// The phone and Mac window import SwiftUI and AVFoundation, which this Linux
// host cannot compile. The contract target is the argument list and the JSON
// models those screens decode, and it builds here.
var targets: [Target] = [
    .target(name: "OpenWorldContract", path: "Sources/OpenWorldContract"),
    .testTarget(
        name: "OpenWorldContractTests",
        dependencies: ["OpenWorldContract"],
        path: "Tests/OpenWorldContractTests"
    ),
]
#if !os(Linux)
targets.append(
    .executableTarget(
        name: "OpenWorld",
        dependencies: ["OpenWorldContract"],
        path: "Sources/OpenWorld"
    )
)
#endif

#if os(Linux)
let package = Package(name: "OpenWorld", targets: targets)
#else
let package = Package(
    name: "OpenWorld",
    platforms: [
        .macOS(.v14),
        .iOS(.v17),
    ],
    targets: targets
)
#endif
