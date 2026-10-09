// swift-tools-version: 5.9
// SPDX-License-Identifier: Apache-2.0
import PackageDescription

// The iPhone and Mac app imports SwiftUI. That executable exists only when this
// package is built on macOS. Linux CI compiles the same screens against
// OpenSwiftUI, and only the test target depends on it.
let contract = Target.target(name: "OpenWorldContract", path: "Sources/OpenWorldContract")
let contractTests = Target.testTarget(
    name: "OpenWorldContractTests",
    dependencies: ["OpenWorldContract"],
    path: "Tests/OpenWorldContractTests"
)

#if os(Linux)
let package = Package(
    name: "OpenWorld",
    dependencies: [
        .package(
            url: "https://github.com/OpenSwiftUIProject/OpenSwiftUI.git",
            revision: "aefa4e6edc9c37a992a0cf9c6cf317fb670ed305"
        ),
    ],
    targets: [
        contract,
        contractTests,
        .testTarget(
            name: "OpenWorldUITests",
            dependencies: [
                "OpenWorldContract",
                .product(name: "OpenSwiftUI", package: "OpenSwiftUI"),
            ],
            path: "Sources/OpenWorldUI"
        ),
    ]
)
#else
let package = Package(
    name: "OpenWorld",
    platforms: [
        .macOS(.v14),
        .iOS(.v17),
    ],
    targets: [
        contract,
        contractTests,
        .target(
            name: "OpenWorldUI",
            dependencies: ["OpenWorldContract"],
            path: "Sources/OpenWorldUI",
            exclude: ["LinuxScreenTests.swift"]
        ),
        .executableTarget(
            name: "OpenWorld",
            dependencies: ["OpenWorldUI"],
            path: "Sources/OpenWorld"
        ),
    ]
)
#endif
