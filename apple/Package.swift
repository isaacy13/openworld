// swift-tools-version: 5.9
// SPDX-License-Identifier: Apache-2.0
import PackageDescription

let package = Package(
    name: "OpenWorld",
    platforms: [
        .macOS(.v14),
        .iOS(.v17),
    ],
    targets: [
        .executableTarget(name: "OpenWorld", path: "Sources/OpenWorld"),
    ]
)
