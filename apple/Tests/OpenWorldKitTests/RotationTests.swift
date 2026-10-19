// SPDX-License-Identifier: Apache-2.0
import OpenWorldKit
import XCTest

/// AVFoundation's `preferredTransform` to the clockwise turn the core applies.
final class RotationTests: XCTestCase {
    func testQuarterTurns() {
        // Landscape, as recorded.
        XCTAssertEqual(Rotation.of(a: 1, b: 0, c: 0, d: 1), .up)
        // iPhone portrait: (0, 1, -1, 0) with tx = height. Upright needs 90 clockwise.
        XCTAssertEqual(Rotation.of(a: 0, b: 1, c: -1, d: 0), .right)
        // Upside-down landscape.
        XCTAssertEqual(Rotation.of(a: -1, b: 0, c: 0, d: -1), .down)
        // Portrait the other way up.
        XCTAssertEqual(Rotation.of(a: 0, b: -1, c: 1, d: 0), .left)
    }

    func testTranslationDoesNotMatterAndNoiseSnaps() {
        // A transform read back from a file is rarely exact.
        XCTAssertEqual(Rotation.of(a: 1e-7, b: 1, c: -1, d: -2e-7), .right)
        XCTAssertEqual(Rotation.of(a: -0.99999, b: 0.0001, c: -0.0001, d: -0.99999), .down)
        // 40 degrees is nearer to no turn than to a quarter turn.
        let r = 40.0 * .pi / 180
        XCTAssertEqual(Rotation.of(a: cos(r), b: sin(r), c: -sin(r), d: cos(r)), .up)
        XCTAssertEqual(Rotation.of(a: .nan, b: 0, c: 0, d: 1), .up)
        XCTAssertEqual(Rotation.of(a: 0, b: 0, c: 0, d: 0), .up)
    }

    func testMirroredFrontCameraStaysUpright() {
        // A horizontal flip: upright, mirrored. Not a half turn.
        XCTAssertEqual(Rotation.of(a: -1, b: 0, c: 0, d: 1), .up)
        XCTAssertTrue(Rotation.isMirrored(a: -1, b: 0, c: 0, d: 1))
        // Portrait front camera: flip then a quarter turn.
        XCTAssertEqual(Rotation.of(a: 0, b: -1, c: -1, d: 0), .right)
        XCTAssertTrue(Rotation.isMirrored(a: 0, b: -1, c: -1, d: 0))
        XCTAssertFalse(Rotation.isMirrored(a: 0, b: 1, c: -1, d: 0))
    }

    func testUprightSize() {
        XCTAssertEqual(Rotation.right.upright(width: 1920, height: 1080).width, 1080)
        XCTAssertEqual(Rotation.down.upright(width: 1920, height: 1080).width, 1920)
    }
}
