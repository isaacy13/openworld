// SPDX-License-Identifier: Apache-2.0
#if os(Linux)
import Foundation
import OpenSwiftUI
import OpenWorldContract
import OpenWorldUI
import XCTest

/// Linux CI builds these screens with OpenSwiftUI and runs the phone model
/// against the same fixture stills as the other shells. The app target is not
/// part of this package on Linux.
final class OpenWorldUITests: XCTestCase {
    func testPhoneScreensBuild() throws {
        try MainActor.assumeIsolated {
            try self.drawScreens()
        }
    }

    func testSceneScanReachesTheResultsScreen() throws {
        try MainActor.assumeIsolated {
            let model = try self.scan("scene")
            XCTAssertEqual(model.step, .results)
            XCTAssertEqual(model.report?.status, "complete")
            XCTAssertEqual(model.report?.summary, Copy.possible)
            let labels = model.report?.inventory.map(\.label) ?? []
            XCTAssertTrue(labels.contains(Copy.possible))
            XCTAssertTrue(labels.contains(Copy.notCompared))
            XCTAssertTrue(labels.contains("A vehicle is not a person."))
            XCTAssertGreaterThanOrEqual(model.report?.facesSeenNotCompared ?? 0, 1)
            XCTAssertFalse(model.report?.candidates.isEmpty ?? true)
            XCTAssertEqual(model.report?.candidates.first?.leaving, Copy.leaving)
            XCTAssertTrue(model.report?.candidates.first?.fbiUrl.hasPrefix("https://www.fbi.gov") == true)
            let disclosure = model.report?.disclosure ?? []
            XCTAssertTrue(disclosure.contains("Nothing is uploaded."))
            XCTAssertTrue(disclosure.contains("Nobody is enrolled."))
            XCTAssertTrue(disclosure.contains("OpenWorld does not train on this file."))
            XCTAssertTrue(disclosure.contains("OpenWorld does not contact an agency."))
            XCTAssertTrue(disclosure.contains("A candidate is not an identification."))
            XCTAssertTrue(disclosure.contains(Copy.clearance))
            XCTAssertTrue(disclosure.contains("This file is not authenticated."))
            XCTAssertTrue(disclosure.contains("On-device does not mean the file is real."))
            XCTAssertTrue(model.report?.perceptionNote?.contains("Fixture markers were read.") == true)
            if let page = model.report?.candidates.first?.fbiUrl {
                model.leavingURL = URL(string: page)
            }
            _ = FlowView(model: model, importControl: self.control).body
            XCTAssertEqual(model.leavingURL?.host, "www.fbi.gov")
            model.leavingURL = nil
            XCTAssertNil(model.leavingURL)
            _ = FlowView(model: model, importControl: self.control).body
        }
    }

    func testBlankMeasuredOldFileIsAClearanceWithTheWarning() throws {
        try MainActor.assumeIsolated {
            let file = try self.still("blank")
            let old = Date().addingTimeInterval(-40 * 24 * 3600)
            try FileManager.default.setAttributes([.modificationDate: old], ofItemAtPath: file.path)
            let model = FlowModel(phone: true)
            model.choose(file)
            XCTAssertTrue(model.oldFile)
            model.loadBundles()
            XCTAssertTrue(model.bundles.contains { $0.id == "fast" && $0.preselected })
            XCTAssertTrue(model.bundles.contains { $0.id == "accurate" && $0.curveLine.contains("Not measured yet.") })
            model.longSide = "640"
            model.coverage = "measured"
            model.step = .size
            _ = FlowView(model: model, importControl: self.control).body
            model.loadEstimate()
            XCTAssertEqual(model.estimate?.deviceNote, "This scan runs on the CPU. It will be slower, warmer, and use more battery.")
            XCTAssertTrue(model.estimate?.heatNote?.contains("This phone may get hot.") == true)
            model.analyze()
            XCTAssertEqual(model.report?.summary, Copy.clearance)
            XCTAssertEqual(model.report?.coverageBanner, Copy.brief)
            XCTAssertTrue(model.report?.warnings.contains(Copy.oldFile) == true)
            XCTAssertTrue(model.report?.candidates.isEmpty == true)
            XCTAssertTrue(model.report?.inventory.isEmpty == true)
            _ = FlowView(model: model, importControl: self.control).body
        }
    }

    func testGenuineFaceBelowTheCutoffIsNotACandidate() throws {
        try MainActor.assumeIsolated {
            let model = try self.scan("below")
            XCTAssertEqual(model.report?.summary, Copy.clearance)
            XCTAssertTrue(model.report?.candidates.isEmpty == true)
            XCTAssertTrue(model.report?.inventory.map(\.label).contains("Below the locked cutoff. Not a candidate.") == true)
        }
    }

    func testImpostorFaceIsNotACandidate() throws {
        try MainActor.assumeIsolated {
            let model = try self.scan("impostor")
            XCTAssertEqual(model.report?.summary, Copy.clearance)
            XCTAssertTrue(model.report?.candidates.isEmpty == true)
            XCTAssertFalse(model.report?.inventory.map(\.label).contains(Copy.possible) == true)
        }
    }

    func testFaceUnder64PixelsIsLeftOut() throws {
        try MainActor.assumeIsolated {
            let model = try self.scan("tiny")
            XCTAssertEqual(model.report?.summary, Copy.clearance)
            XCTAssertEqual(model.report?.facesSeenNotCompared, 0)
            XCTAssertTrue(model.report?.inventory.isEmpty == true)
            XCTAssertTrue(model.report?.candidates.isEmpty == true)
        }
    }

    func testFaceAt64PixelsIsSeenAndNotCompared() throws {
        try MainActor.assumeIsolated {
            let model = try self.scan("uncompared")
            XCTAssertEqual(model.report?.summary, Copy.clearance)
            XCTAssertEqual(model.report?.inventory.map(\.label), [Copy.notCompared])
            XCTAssertGreaterThanOrEqual(model.report?.facesSeenNotCompared ?? 0, 1)
            XCTAssertTrue(model.report?.candidates.isEmpty == true)
        }
    }

    func testANonPngRefusesBeforeAnalyze() throws {
        try MainActor.assumeIsolated {
            let junk = FileManager.default.temporaryDirectory.appendingPathComponent("ow-junk-\(UUID().uuidString).txt")
            try Data("this is not a photo".utf8).write(to: junk)
            let model = FlowModel(phone: true)
            model.choose(junk)
            model.loadEstimate()
            XCTAssertEqual(model.step, .estimate)
            XCTAssertNil(model.estimate)
            XCTAssertTrue(model.error?.contains("AVFoundation decodes on macOS and iOS. Refusing.") == true)
            _ = FlowView(model: model, importControl: self.control).body
        }
    }

    @MainActor
    private var control: AnyView {
        AnyView(Button(action: {}) { Text("Choose File") })
    }

    @MainActor
    private func scan(_ kind: String, coverage: String = "complete") throws -> FlowModel {
        let model = FlowModel(phone: true)
        model.choose(try still(kind))
        model.loadBundles()
        model.longSide = "640"
        model.coverage = coverage
        model.loadEstimate()
        XCTAssertNotNil(model.estimate)
        model.analyze()
        XCTAssertEqual(model.step, .results)
        XCTAssertNil(model.error)
        return model
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
        let process = Process()
        process.executableURL = URL(fileURLWithPath: binary())
        process.arguments = args
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        try process.run()
        process.waitUntilExit()
        if process.terminationStatus != 0 {
            let text = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
            XCTFail(text)
        }
        return out
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

    @MainActor
    private func drawScreens() throws {
        let model = FlowModel(phone: true)
        _ = FlowView(model: model, importControl: control).body

        model.choose(URL(fileURLWithPath: "/tmp/scene.png"))
        let step = model.step
        XCTAssertEqual(step, .device)
        _ = FlowView(model: model, importControl: control).body

        model.bundles = [try row()]
        model.step = .bundle
        _ = FlowView(model: model, importControl: control).body

        model.step = .size
        model.coverage = "measured"
        _ = FlowView(model: model, importControl: control).body
        XCTAssertEqual(Copy.brief, "A brief face can be missed.")

        model.estimate = try JSONDecoder().decode(Estimate.self, from: Data(estimateJSON.utf8))
        model.coverage = "complete"
        model.step = .estimate
        _ = FlowView(model: model, importControl: control).body
        let deviceNote = model.estimate?.deviceNote
        XCTAssertEqual(deviceNote, "This scan runs on the CPU. It will be slower, warmer, and use more battery.")

        model.report = try JSONDecoder().decode(ScanReport.self, from: Data(reportJSON.utf8))
        model.leavingURL = URL(string: "https://www.fbi.gov/wanted")
        model.step = .results
        _ = FlowView(model: model, importControl: control).body
        let summary = model.report?.summary
        let disclosure = model.report?.disclosure ?? []
        let warnings = model.report?.warnings ?? []
        XCTAssertEqual(summary, Copy.possible)
        XCTAssertEqual(warnings, [Copy.disagree])
        XCTAssertEqual(Copy.leaving, "You are leaving OpenWorld.")
        XCTAssertEqual(Copy.clearance, "No candidate is not a clearance.")
        XCTAssertEqual(Copy.incomplete, "Incomplete.")
        XCTAssertEqual(Copy.notCompared, "Not compared.")
        XCTAssertTrue(disclosure.contains("Nothing is uploaded."))
        XCTAssertTrue(disclosure.contains("Nobody is enrolled."))
        XCTAssertTrue(disclosure.contains("OpenWorld does not train on this file."))
        XCTAssertTrue(disclosure.contains("OpenWorld does not contact an agency."))
        XCTAssertTrue(disclosure.contains("A candidate is not an identification."))
        XCTAssertTrue(disclosure.contains("No candidate is not a clearance."))
        XCTAssertTrue(disclosure.contains("This file is not authenticated."))
        XCTAssertTrue(disclosure.contains("On-device does not mean the file is real."))
    }

    private func row() throws -> BundleRow {
        try JSONDecoder().decode(BundleRow.self, from: Data("""
        {"id":"fast","name":"Fast","best_for":"Phones and long video.","curve_line":"Fixture curve measured. Real FBI photos stay off.","preselected":true}
        """.utf8))
    }
}

private let estimateJSON = """
{"human":"Less than a second","caveat":"This is a planning estimate, not a thermal measurement.","device_note":"This scan runs on the CPU. It will be slower, warmer, and use more battery.","heat_note":"This phone may get hot. If heat or the system stops the scan, the result is Incomplete.","battery_note":"A long scan uses a lot of battery."}
"""

private let reportJSON = """
{"status":"complete","summary":"Possible candidate. Not an identification.","bundle_name":"Fast","perception_note":"Fixture markers were read.","disclosure":["Nothing is uploaded.","Nobody is enrolled.","OpenWorld does not train on this file.","OpenWorld does not contact an agency.","A candidate is not an identification.","No candidate is not a clearance.","This file is not authenticated.","On-device does not mean the file is real."],"warnings":["The file timestamps disagree."],"faces_seen_not_compared":1,"inventory":[{"kind":"face","label":"Possible candidate. Not an identification.","frame_index":0},{"kind":"face","label":"Not compared.","frame_index":0}],"candidates":[{"wording":"Possible candidate. Not an identification.","kind":"face","uncertainty":"Cosine 0.98 is above the locked cutoff 0.55 for Fast. Possible candidate. Not an identification.","poster_title":"Fixture subject A","poster_class":"missing","fbi_url":"https://www.fbi.gov/wanted","leaving":"You are leaving OpenWorld.","frame_index":0,"poster_id":"a"}],"comparisons":[]}
"""
#endif
