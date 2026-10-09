// SPDX-License-Identifier: Apache-2.0
import Foundation
import OpenWorldContract
import XCTest

/// The iPhone screen's argument list and JSON models, run against the openworld program.
/// Drawing the screens on Linux is OpenWorldUITests, and that target uses OpenSwiftUI.
/// A PNG is copied to frame_000000.png, which is what the phone does for a PNG still.
final class PhoneContractTests: XCTestCase {
    func testSceneShowsACandidateAndAFaceThatWasNotCompared() throws {
        let report = try scan("scene")
        XCTAssertEqual(report.status, "complete")
        XCTAssertEqual(report.summary, "Possible candidate. Not an identification.")
        XCTAssertTrue(report.disclosure.contains("Nothing is uploaded."))
        XCTAssertTrue(report.disclosure.contains("Nobody is enrolled."))
        XCTAssertTrue(report.disclosure.contains("OpenWorld does not train on this file."))
        XCTAssertTrue(report.disclosure.contains("OpenWorld does not contact an agency."))
        XCTAssertTrue(report.disclosure.contains("A candidate is not an identification."))
        XCTAssertTrue(report.disclosure.contains("No candidate is not a clearance."))
        XCTAssertTrue(report.disclosure.contains("This file is not authenticated."))
        XCTAssertTrue(report.disclosure.contains("On-device does not mean the file is real."))
        XCTAssertTrue(report.perceptionNote?.contains("Fixture markers were read.") == true)
        let labels = report.inventory.map(\.label)
        XCTAssertTrue(labels.contains("Possible candidate. Not an identification."))
        XCTAssertTrue(labels.contains("Not compared."))
        XCTAssertTrue(labels.contains("A vehicle is not a person."))
        XCTAssertGreaterThanOrEqual(report.facesSeenNotCompared, 1)
        XCTAssertFalse(report.candidates.isEmpty)
        XCTAssertEqual(report.candidates[0].leaving, "You are leaving OpenWorld.")
        XCTAssertTrue(report.candidates[0].fbiUrl.hasPrefix("https://www.fbi.gov"))
    }

    func testBlankStillIsAClearance() throws {
        let report = try scan("blank")
        XCTAssertEqual(report.summary, "No candidate is not a clearance.")
        XCTAssertTrue(report.candidates.isEmpty)
        XCTAssertTrue(report.inventory.isEmpty)
    }

    func testGenuineFaceBelowTheCutoffIsNotACandidate() throws {
        let report = try scan("below")
        XCTAssertEqual(report.summary, "No candidate is not a clearance.")
        XCTAssertTrue(report.candidates.isEmpty)
        XCTAssertTrue(report.inventory.map(\.label).contains("Below the locked cutoff. Not a candidate."))
        let matched = report.comparisons.filter { $0.posterFiducialId == $0.fiducialId }
        XCTAssertFalse(matched.isEmpty)
        XCTAssertTrue(matched.allSatisfy { $0.passed == false })
    }

    func testImpostorFaceIsNotACandidate() throws {
        let report = try scan("impostor")
        XCTAssertEqual(report.summary, "No candidate is not a clearance.")
        XCTAssertTrue(report.candidates.isEmpty)
        XCTAssertFalse(report.inventory.map(\.label).contains("Possible candidate. Not an identification."))
        XCTAssertFalse(report.comparisons.isEmpty)
        for row in report.comparisons {
            XCTAssertFalse(row.passed)
            if let poster = row.posterFiducialId {
                XCTAssertNotEqual(row.fiducialId, poster)
            }
        }
    }

    func testFaceUnder64PixelsIsLeftOut() throws {
        let report = try scan("tiny")
        XCTAssertEqual(report.summary, "No candidate is not a clearance.")
        XCTAssertEqual(report.facesSeenNotCompared, 0)
        XCTAssertTrue(report.inventory.isEmpty)
        XCTAssertTrue(report.candidates.isEmpty)
    }

    func testFaceAt64PixelsIsSeenAndNotCompared() throws {
        let report = try scan("uncompared")
        XCTAssertEqual(report.summary, "No candidate is not a clearance.")
        XCTAssertEqual(report.inventory.map(\.label), ["Not compared."])
        XCTAssertGreaterThanOrEqual(report.facesSeenNotCompared, 1)
        XCTAssertTrue(report.candidates.isEmpty)
    }

    func testMeasuredCoverageNamesTheBriefFaceBanner() throws {
        let report = try scan("blank", coverage: "measured")
        XCTAssertEqual(report.status, "complete")
        XCTAssertEqual(report.coverageBanner, "A brief face can be missed.")
        XCTAssertEqual(report.summary, "No candidate is not a clearance.")
        XCTAssertTrue(report.candidates.isEmpty)
    }

    func testDisagreeingContainerTimeWarnsAndDoesNotRefuse() throws {
        let report = try scan("blank", containerUnix: 1_000_000_000)
        XCTAssertEqual(report.status, "complete")
        XCTAssertEqual(report.summary, "No candidate is not a clearance.")
        XCTAssertTrue(report.warnings.contains("The file timestamps disagree."))
        XCTAssertTrue(report.candidates.isEmpty)
    }

    func testPhoneEstimateNamesCpuHeatAndBattery() throws {
        let file = try still("blank")
        let size = try pngSize(file)
        let rows = try JSONDecoder().decode(BundleList.self, from: run(PhoneArguments.bundles(catalog: bundlesDir()))).bundles
        XCTAssertTrue(rows.contains { $0.id == "fast" && $0.preselected })
        XCTAssertTrue(rows.contains { $0.id == "accurate" && $0.curveLine.contains("Not measured yet.") })
        let media = MediaArguments(width: size.0, height: size.1, fps: 0, frames: 1, duration: 0, video: false, containerUnix: nil)
        let estimate = try JSONDecoder().decode(
            Estimate.self,
            from: run(PhoneArguments.estimate(
                catalog: bundlesDir(),
                input: file.path,
                bundle: "fast",
                longSide: "640",
                coverage: "complete",
                phone: true,
                media: media
            ))
        )
        XCTAssertTrue(estimate.human.contains("second"))
        XCTAssertEqual(estimate.caveat, "This is a planning estimate, not a thermal measurement.")
        XCTAssertEqual(estimate.deviceNote, "This scan runs on the CPU. It will be slower, warmer, and use more battery.")
        XCTAssertTrue(estimate.heatNote?.contains("This phone may get hot.") == true)
        XCTAssertEqual(estimate.batteryNote, "A long scan uses a lot of battery.")
        XCTAssertNil(estimate.suggestComputerText)
        let measured = PhoneArguments.estimate(
            catalog: bundlesDir(),
            input: file.path,
            bundle: "fast",
            longSide: "640",
            coverage: "measured",
            phone: true,
            media: media
        )
        XCTAssertTrue(measured.contains("measured"))
        XCTAssertTrue(measured.contains("phone"))
    }

    func testScanArgumentsMatchThePhoneContract() {
        let media = MediaArguments(
            width: 640, height: 480, fps: 30, frames: 2, duration: 1, video: true, containerUnix: 1000
        )
        let args = PhoneArguments.scan(
            catalog: "/bundles",
            input: "/in.png",
            bundle: "fast",
            longSide: "640",
            coverage: "measured",
            posters: "/posters",
            frames: "/frames",
            out: "/out",
            phone: true,
            media: media
        )
        XCTAssertEqual(
            args,
            [
                "--json", "--bundles", "/bundles", "scan",
                "--input", "/in.png",
                "--bundle", "fast",
                "--long-side", "640",
                "--coverage", "measured",
                "--posters", "/posters",
                "--out", "/out",
                "--form-factor", "phone",
                "--provider", "cpu",
                "--width", "640",
                "--height", "480",
                "--fps", "30.0",
                "--frame-count", "2",
                "--duration", "1.0",
                "--video",
                "--container-unix", "1000",
                "--frames", "/frames",
            ]
        )
    }

    private func scan(_ kind: String, coverage: String = "complete", containerUnix: Int? = nil) throws -> ScanReport {
        let file = try still(kind)
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let posters = root.appendingPathComponent("posters")
        let frames = root.appendingPathComponent("frames")
        try FileManager.default.createDirectory(at: frames, withIntermediateDirectories: true)
        try FileManager.default.copyItem(at: file, to: frames.appendingPathComponent("frame_000000.png"))
        _ = try run(PhoneArguments.writeFixture(out: posters.path))
        let size = try pngSize(file)
        let media = MediaArguments(width: size.0, height: size.1, fps: 0, frames: 1, duration: 0, video: false, containerUnix: nil)
        let data = try run(PhoneArguments.scan(
            catalog: bundlesDir(),
            input: file.path,
            bundle: "fast",
            longSide: "640",
            coverage: coverage,
            posters: posters.path,
            frames: frames.path,
            out: root.appendingPathComponent("result").path,
            phone: true,
            media: MediaArguments(
                width: media.width,
                height: media.height,
                fps: media.fps,
                frames: media.frames,
                duration: media.duration,
                video: media.video,
                containerUnix: containerUnix
            )
        ))
        return try JSONDecoder().decode(ScanReport.self, from: data)
    }

    private func still(_ kind: String) throws -> URL {
        let out = FileManager.default.temporaryDirectory.appendingPathComponent("ow-\(kind)-\(UUID().uuidString).png")
        var args = ["--json", "--bundles", bundlesDir(), "fixture-still", "--out", out.path]
        switch kind {
        case "scene": args.append("--scene")
        case "blank": args.append("--blank")
        case "impostor": args.append(contentsOf: ["--id", "99", "--module", "16", "--x", "16", "--y", "16"])
        case "below": args.append(contentsOf: ["--id", "7", "--module", "16", "--below-cutoff"])
        case "uncompared": args.append(contentsOf: ["--id", "11", "--module", "8", "--x", "16", "--y", "16"])
        case "tiny": args.append(contentsOf: ["--id", "7", "--module", "4", "--x", "16", "--y", "16"])
        default: XCTFail(kind)
        }
        _ = try run(args)
        return out
    }

    private func run(_ args: [String]) throws -> Data {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: binary())
        process.arguments = args
        let out = Pipe()
        let err = Pipe()
        process.standardOutput = out
        process.standardError = err
        try process.run()
        process.waitUntilExit()
        let data = out.fileHandleForReading.readDataToEndOfFile()
        if process.terminationStatus != 0 {
            let message = String(data: data, encoding: .utf8) ?? ""
            let error = String(data: err.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
            XCTFail("exit \(process.terminationStatus) \(message) \(error)")
        }
        return data
    }

    private func binary() -> String {
        if let env = ProcessInfo.processInfo.environment["OPENWORLD_BIN"], !env.isEmpty {
            return env
        }
        return "openworld"
    }

    private func bundlesDir() -> String {
        if let env = ProcessInfo.processInfo.environment["OPENWORLD_BUNDLES"], !env.isEmpty {
            return env
        }
        return "bundles"
    }

    private func pngSize(_ url: URL) throws -> (Int, Int) {
        let bytes = [UInt8](try Data(contentsOf: url).prefix(24))
        XCTAssertGreaterThanOrEqual(bytes.count, 24)
        let width = (Int(bytes[16]) << 24) | (Int(bytes[17]) << 16) | (Int(bytes[18]) << 8) | Int(bytes[19])
        let height = (Int(bytes[20]) << 24) | (Int(bytes[21]) << 16) | (Int(bytes[22]) << 8) | Int(bytes[23])
        return (width, height)
    }
}
