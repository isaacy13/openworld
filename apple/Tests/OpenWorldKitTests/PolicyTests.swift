// SPDX-License-Identifier: Apache-2.0
@testable import OpenWorldKit
import XCTest

final class PolicyTests: XCTestCase {
    let copy = CopyTable(texts: ["old_file": "This file is older than about 30 days.", "timestamps_disagree": "The file timestamps disagree."])

    func testStopPolicy() {
        XCTAssertNil(StopPolicy.reason(for: DeviceFacts(kind: "phone", thermal: "fair", battery: 0.5, charging: false)))
        XCTAssertEqual(StopPolicy.reason(for: DeviceFacts(kind: "phone", thermal: "serious")), .heat)
        XCTAssertEqual(StopPolicy.reason(for: DeviceFacts(kind: "phone", thermal: "critical", battery: 0.05, charging: false)), .heat)
        XCTAssertEqual(StopPolicy.reason(for: DeviceFacts(kind: "phone", thermal: "nominal", battery: 0.09, charging: false)), .battery)
        XCTAssertNil(StopPolicy.reason(for: DeviceFacts(kind: "phone", thermal: "nominal", battery: 0.09, charging: true)))
        // UIDevice reports -1 when the level is unknown.
        XCTAssertNil(StopPolicy.reason(for: DeviceFacts(kind: "phone", battery: -1, charging: false)))
        XCTAssertNil(StopPolicy.reason(for: .computer))
    }

    func testFileWarningsMatchTheCoreRules() {
        let now = Date(timeIntervalSince1970: 100 * 86400)
        let old = Date(timeIntervalSince1970: 10 * 86400)
        XCTAssertEqual(FileWarnings.lines(modified: old, recorded: now, now: now, copy: copy).map(\.code), ["old_file", "timestamps_disagree"])
        XCTAssertEqual(FileWarnings.lines(modified: now, recorded: now.addingTimeInterval(-3600), now: now, copy: copy), [])
        XCTAssertEqual(FileWarnings.lines(modified: nil, recorded: old, now: now, copy: copy), [])
        XCTAssertEqual(FileWarnings.lines(modified: old, recorded: nil, now: now, copy: copy).first?.text, "This file is older than about 30 days.")
    }

    func testExifDates() throws {
        let utc = try XCTUnwrap(TimeZone(identifier: "UTC"))
        let plain = try XCTUnwrap(ExifDate.parse("2024:05:01 14:03:22", zone: utc))
        XCTAssertEqual(plain.timeIntervalSince1970, 1_714_572_202)
        let offset = try XCTUnwrap(ExifDate.parse("2024:05:01 14:03:22", offset: "+02:00", zone: utc))
        XCTAssertEqual(offset.timeIntervalSince1970, 1_714_572_202 - 7200)
        let west = try XCTUnwrap(ExifDate.parse("2024:05:01 14:03:22", offset: "-05:30", zone: utc))
        XCTAssertEqual(west.timeIntervalSince1970, 1_714_572_202 + 5 * 3600 + 1800)
        XCTAssertNil(ExifDate.parse("0000:00:00 00:00:00"))
        XCTAssertNil(ExifDate.parse("2024:02:30 10:00:00"))
        XCTAssertNil(ExifDate.parse("yesterday"))
        // A bad offset falls back to the zone.
        XCTAssertEqual(ExifDate.parse("2024:05:01 14:03:22", offset: "Z", zone: utc), plain)
    }

    func testAnimationTiming() {
        let gif = AnimationTiming.rate(delays: [0.04, 0.04, nil, 0])
        XCTAssertEqual(gif.duration, 0.28, accuracy: 1e-9)
        XCTAssertEqual(gif.fps, 4 / 0.28, accuracy: 1e-9)
        XCTAssertEqual(AnimationTiming.rate(delays: []).fps, 0)
        XCTAssertEqual(AnimationTiming.delay(.infinity), 0.1)
    }

    func testColorMatrix() {
        XCTAssertTrue(ColorMatrix.isBT709(matrix: "ITU_R_709_2", primaries: nil, height: 480))
        XCTAssertFalse(ColorMatrix.isBT709(matrix: "ITU_R_601_4", primaries: "ITU_R_709_2", height: 1080))
        XCTAssertFalse(ColorMatrix.isBT709(matrix: "SMPTE_240M_1995", primaries: nil, height: 1080))
        XCTAssertTrue(ColorMatrix.isBT709(matrix: "ITU_R_2020", primaries: nil, height: 2160))
        XCTAssertFalse(ColorMatrix.isBT709(matrix: nil, primaries: "SMPTE_C", height: 1080))
        XCTAssertTrue(ColorMatrix.isBT709(matrix: nil, primaries: "P3_D65", height: 480))
        // Untagged: HD and up is BT.709, standard definition is BT.601.
        XCTAssertTrue(ColorMatrix.isBT709(matrix: nil, primaries: nil, height: 720))
        XCTAssertFalse(ColorMatrix.isBT709(matrix: nil, primaries: nil, height: 576))
    }

    func testRequestsAreVersionedAndCarryOnlyKnownFields() throws {
        let scan = ScanRequest(
            input: "/a/clip.mov", bundles: "/app/bundles", bundle: "fast", posters: "/app/posters", out: "/app/results/x",
            coverage: .measured, missing: true, wanted: false, device: DeviceFacts(kind: "phone", thermal: "fair", battery: 0.5, charging: true),
            media: MediaFacts(width: 1080, height: 1920, fps: 29.97, frames: 1798, durationSec: 60, video: true, containerUnix: 1_700_000_000),
            fileMtimeUnix: 1_700_000_100
        )
        let estimate = try object(Request.estimate(scan).json)
        XCTAssertEqual(estimate["v"] as? Int, 1)
        XCTAssertEqual(estimate["op"] as? String, "estimate")
        XCTAssertEqual(Set(estimate.keys), ["v", "op", "input", "bundles", "bundle", "posters", "out", "coverage", "missing", "wanted", "accelerate", "device", "media", "file_mtime_unix"])
        XCTAssertEqual(estimate["coverage"] as? String, "measured")
        let media = try XCTUnwrap(estimate["media"] as? [String: Any])
        XCTAssertEqual(Set(media.keys), ["width", "height", "fps", "frames", "duration_sec", "video", "container_unix"])
        XCTAssertTrue(Request.estimate(scan).json.contains(#""input":"/a/clip.mov""#), "slashes stay readable")

        let job = try object(Request.job(scan).json)
        XCTAssertNil(job["op"], "ow_job_start refuses an op field")
        XCTAssertEqual(job["v"] as? Int, 1)

        var still = scan
        still.media = MediaFacts(width: 4032, height: 3024)
        still.fileMtimeUnix = nil
        still.device = .computer
        let body = try object(Request.job(still).json)
        XCTAssertNil(body["file_mtime_unix"])
        XCTAssertEqual(Set((body["device"] as? [String: Any] ?? [:]).keys), ["kind"])
        XCTAssertNil((body["media"] as? [String: Any])?["container_unix"])

        XCTAssertEqual(try object(Request.leave(url: "https://www.fbi.gov/x").json)["url"] as? String, "https://www.fbi.gov/x")
        XCTAssertEqual(try object(Request.copy.json).count, 2)
        XCTAssertEqual(try object(Request.sweep(root: "/r").json)["root"] as? String, "/r")
    }

    private func object(_ json: String) throws -> [String: Any] {
        try XCTUnwrap(JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any])
    }
}
