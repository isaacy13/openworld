// SPDX-License-Identifier: Apache-2.0
#if os(Linux)
import Foundation
import Glibc
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
            if ProcessInfo.processInfo.environment["OPENWORLD_LIB"] != nil {
                XCTAssertTrue(LinkedCore.isLinked)
            }
            let model = try self.scan("scene")
            XCTAssertEqual(model.step, .results)
            XCTAssertEqual(model.report?.status, "complete")
            XCTAssertEqual(model.report?.summary, Copy.possible)
            XCTAssertEqual(model.report?.framesNote, "1 frame analyzed.")
            XCTAssertEqual(model.report?.detectionNote, "640 px on the long side.")
            XCTAssertEqual(model.report?.coverageNote, "Every decoded frame.")
            let labels = model.report?.inventory.map(\.label) ?? []
            XCTAssertTrue(labels.contains(Copy.possible))
            XCTAssertTrue(labels.contains(Copy.notCompared))
            XCTAssertTrue(labels.contains("A vehicle is not a person."))
            XCTAssertGreaterThanOrEqual(model.report?.facesSeenNotCompared ?? 0, 1)
            XCTAssertFalse(model.report?.candidates.isEmpty ?? true)
            XCTAssertTrue(model.report?.candidates.allSatisfy { $0.frameLabel == "Frame 1." } == true)
            XCTAssertEqual(model.report?.candidates.first?.leaving, Copy.leaving)
            XCTAssertTrue(model.report?.candidates.first?.fbiUrl.hasPrefix("https://www.fbi.gov") == true)
            XCTAssertTrue(model.report?.candidates.contains { $0.posterLine == "Fixture subject A (Missing)" } == true)
            XCTAssertTrue(model.report?.candidates.contains { $0.posterLine == "Fixture vehicle C (Wanted)" } == true)
            XCTAssertTrue(model.report?.candidates.contains { $0.kind == "face" && $0.uncertainty.contains("Score ") && $0.uncertainty.contains("keeps a candidate at") && !$0.uncertainty.contains(Copy.possible) } == true)
            XCTAssertTrue(model.report?.candidates.contains { $0.kind == "plate" && $0.uncertainty.contains("FIX123") && !$0.uncertainty.contains(Copy.possible) } == true)
            let preview = PhonePreview.lines(screen: "results", model: model)
            XCTAssertEqual(preview.first, "Back")
            XCTAssertEqual(preview.last, "Choose another file")
            let secondCard = try XCTUnwrap(preview.firstIndex(of: "Fixture vehicle C (Wanted)"))
            let choose = try XCTUnwrap(preview.firstIndex(of: "Choose another file"))
            XCTAssertLessThan(secondCard, choose)
            XCTAssertEqual(preview.filter { $0 == "Open FBI page" }.count, model.report?.candidates.count)
            XCTAssertTrue(preview.contains { $0.contains("keeps a candidate at") && !$0.contains(Copy.possible) })
            XCTAssertTrue(preview.contains("Nothing is uploaded."))
            XCTAssertTrue(preview.contains(Copy.clearance))
            XCTAssertTrue(preview.contains("Crops from this file."))
            let frameCaptions = (model.report?.candidates.count ?? 0) + (model.report?.inventory.count ?? 0)
            XCTAssertEqual(preview.filter { $0 == "Frame 1." }.count, frameCaptions)
            XCTAssertTrue(model.report?.inventory.allSatisfy { $0.frameLabel == "Frame 1." } == true)
            XCTAssertTrue(preview.contains("1 frame analyzed."))
            let framesAt = try XCTUnwrap(preview.firstIndex(of: "1 frame analyzed."))
            let classAt = try XCTUnwrap(preview.firstIndex(of: "Missing and wanted."))
            XCTAssertLessThan(framesAt, classAt)
            let bundleAt = try XCTUnwrap(preview.firstIndex(of: "Bundle: Fast"))
            let sizeAt = try XCTUnwrap(preview.firstIndex(of: "640 px on the long side."))
            let coverageAt = try XCTUnwrap(preview.firstIndex(of: "Every decoded frame."))
            XCTAssertLessThan(bundleAt, sizeAt)
            XCTAssertLessThan(sizeAt, coverageAt)
            XCTAssertTrue(model.report?.candidates.allSatisfy { $0.posterClassLabel == "Missing" || $0.posterClassLabel == "Wanted" } == true)
            var untitled = try XCTUnwrap(model.report?.candidates.first { $0.posterClassLabel == "Missing" })
            untitled.posterTitle = ""
            XCTAssertEqual(untitled.posterLine, "Missing")
            untitled.posterTitle = "Fixture subject A"
            untitled.posterClassLabel = ""
            XCTAssertEqual(untitled.posterLine, "Fixture subject A")
            untitled.posterTitle = ""
            XCTAssertEqual(untitled.posterLine, "")
            let saved = model.report
            if var report = model.report, var card = report.candidates.first {
                card.posterTitle = ""
                card.posterClassLabel = "Missing"
                report.candidates = [card]
                model.report = report
                let unnamed = PhonePreview.lines(screen: "results", model: model)
                XCTAssertTrue(unnamed.contains("Missing"))
                XCTAssertFalse(unnamed.contains(" (Missing)"))
            }
            model.report = saved
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
            model.requestLeave("https://www.fbi.gov.evil.com/wanted")
            XCTAssertNil(model.leavingURL)
            XCTAssertTrue(model.leaveError?.contains("FBI page") == true)
            _ = FlowView(model: model, importControl: self.control).body
            if let page = model.report?.candidates.first?.fbiUrl {
                model.requestLeave(page)
            }
            _ = FlowView(model: model, importControl: self.control).body
            XCTAssertEqual(model.leavingURL?.host, "www.fbi.gov")
            XCTAssertNil(model.leaveError)
            model.leavingURL = nil
            XCTAssertNil(model.leavingURL)
            _ = FlowView(model: model, importControl: self.control).body
        }
    }

    func testThePhonePreviewKeepsTheSizeRulesAndAnalyze() throws {
        try MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            model.choose(try self.still("blank"))
            model.loadBundles()
            model.longSide = "640"
            model.coverage = "measured"
            let sizeLines = PhonePreview.lines(screen: "size", model: model)
            XCTAssertTrue(sizeLines.contains(Copy.sizeHint))
            let coverAt = try XCTUnwrap(sizeLines.firstIndex(of: "Coverage"))
            let completeAt = try XCTUnwrap(sizeLines.firstIndex(of: "Complete. Every decoded frame."))
            let measuredAt = try XCTUnwrap(sizeLines.firstIndex(of: "Measured. 5 frames a second, plus the tracker. Selected."))
            XCTAssertLessThan(coverAt, completeAt)
            XCTAssertLessThan(completeAt, measuredAt)
            XCTAssertEqual(sizeLines.last, "Continue")
            let size = PhonePreview.wrapped(screen: "size", model: model)
            let sizeText = size.joined(separator: "\n")
            XCTAssertTrue(sizeText.contains("face under 64 px on that image is"))
            XCTAssertTrue(sizeText.contains("the label is \"Not compared.\""))
            XCTAssertTrue(size.contains("640 px on the long side. Selected."))
            XCTAssertTrue(size.contains(Copy.brief))
            XCTAssertEqual(size.last, "Continue")
            model.loadEstimate()
            let estimate = PhonePreview.wrapped(screen: "estimate", model: model)
            XCTAssertTrue(estimate.contains("Missing. Selected."))
            XCTAssertTrue(estimate.contains("Wanted. Selected."))
            XCTAssertTrue(estimate.contains("Missing and wanted."))
            XCTAssertTrue(estimate.contains(Copy.brief))
            let plain = PhonePreview.lines(screen: "estimate", model: model)
            let choiceAt = try XCTUnwrap(plain.firstIndex(of: "Fast. 640 px on the long side. 5 frames a second, plus the tracker."))
            let briefAt = try XCTUnwrap(plain.firstIndex(of: Copy.brief))
            let classAt = try XCTUnwrap(plain.firstIndex(of: "Missing and wanted."))
            XCTAssertLessThan(choiceAt, briefAt)
            XCTAssertLessThan(briefAt, classAt)
            XCTAssertEqual(estimate.last, "Analyze")
            let file = try XCTUnwrap(model.file)
            let old = Date().addingTimeInterval(-40 * 24 * 3600)
            try FileManager.default.setAttributes([.modificationDate: old], ofItemAtPath: file.path)
            model.choose(file)
            model.loadBundles()
            model.coverage = "measured"
            model.loadEstimate()
            let warned = PhonePreview.lines(screen: "estimate", model: model)
            XCTAssertTrue(warned.contains(Copy.oldFile))
            XCTAssertEqual(warned.last, "Analyze")
            model.includeMissing = false
            model.includeWanted = false
            model.applyClassGate()
            let blocked = PhonePreview.lines(screen: "estimate", model: model)
            XCTAssertTrue(blocked.contains("Choose missing, wanted, or both."))
            XCTAssertEqual(blocked.last, "Analyze")
        }
    }

    func testAnEmptyCatalogNamesTheMissingProgram() throws {
        let empty = FileManager.default.temporaryDirectory
            .appendingPathComponent("openworld-empty-bundles-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: empty, withIntermediateDirectories: true)
        let previous = getenv("OPENWORLD_BUNDLES").map { String(cString: $0) }
        defer {
            if let previous {
                setenv("OPENWORLD_BUNDLES", previous, 1)
            } else {
                unsetenv("OPENWORLD_BUNDLES")
            }
            try? FileManager.default.removeItem(at: empty)
        }
        setenv("OPENWORLD_BUNDLES", empty.path, 1)
        try MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            model.choose(try self.still("blank"))
            model.loadBundles()
            XCTAssertTrue(model.bundles.isEmpty)
            XCTAssertEqual(model.step, .bundle)
            XCTAssertEqual(model.error, "The scan program is not on this device. Refusing.")
            XCTAssertFalse(model.showsTopError)
            let preview = PhonePreview.lines(screen: "bundle", model: model)
            XCTAssertTrue(preview.contains("Model bundle"))
            XCTAssertEqual(preview.filter { $0 == "The scan program is not on this device. Refusing." }.count, 1)
            XCTAssertFalse(preview.contains("Fast. Selected."))
            XCTAssertEqual(preview.last, "Continue")
            model.continueFromBundles()
            XCTAssertEqual(model.step, .bundle)
            _ = FlowView(model: model, importControl: self.control).body
        }
        if let previous {
            setenv("OPENWORLD_BUNDLES", previous, 1)
        } else {
            unsetenv("OPENWORLD_BUNDLES")
        }
        MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            model.error = "The scan program is not on this device. Refusing."
            model.loadBundles()
            XCTAssertFalse(model.bundles.isEmpty)
            XCTAssertNil(model.error)
            XCTAssertEqual(model.bundles.first?.id, "fast")
            let preview = PhonePreview.lines(screen: "bundle", model: model)
            XCTAssertFalse(preview.contains("The scan program is not on this device. Refusing."))
            XCTAssertTrue(preview.contains("Fast. Selected."))
        }
    }

    func testAnUnreadableProgramAnswerUsesTheCommandRefusal() throws {
        defer { ProgramAnswer.stdoutForTest = nil }
        try MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            ProgramAnswer.stdoutForTest = { _ in Data() }
            model.loadBundles()
            XCTAssertTrue(model.bundles.isEmpty)
            XCTAssertEqual(model.error, "The bundle catalog could not be read. Refusing.")
            ProgramAnswer.stdoutForTest = { _ in Data("not json".utf8) }
            model.loadBundles()
            let bundle = PhonePreview.lines(screen: "bundle", model: model)
            XCTAssertEqual(bundle.filter { $0 == "The bundle catalog could not be read. Refusing." }.count, 1)
            XCTAssertFalse(bundle.contains("The scan program is not on this device. Refusing."))

            model.choose(try self.still("blank"))
            model.loadEstimate()
            XCTAssertNil(model.estimate)
            XCTAssertEqual(model.error, "The scan could not be read. Refusing.")
            XCTAssertFalse(model.showsAnalyze)
            let estimate = PhonePreview.lines(screen: "estimate", model: model)
            XCTAssertEqual(estimate.filter { $0 == "The scan could not be read. Refusing." }.count, 1)
            XCTAssertFalse(estimate.contains("Analyze"))

            model.canAnalyze = true
            ProgramAnswer.stdoutForTest = { args in
                if args.contains("posters") {
                    return Data(#"{"id":"fixture-v0"}"#.utf8)
                }
                return Data("not json".utf8)
            }
            model.analyze()
            XCTAssertEqual(model.step, .results)
            XCTAssertNil(model.report)
            XCTAssertNil(model.resultDirectory)
            XCTAssertEqual(model.error, "The scan could not be read. Refusing.")
            let results = PhonePreview.lines(screen: "results", model: model)
            XCTAssertEqual(results.filter { $0 == "The scan could not be read. Refusing." }.count, 1)
            XCTAssertTrue(results.contains("Nothing is uploaded."))
            XCTAssertFalse(results.contains("No candidate is not a clearance."))
            XCTAssertFalse(results.contains("Delete"))
            _ = FlowView(model: model, importControl: self.control).body
        }
    }

    func testAnUnreadableCatalogNamesTheCatalog() throws {
        let missing = FileManager.default.temporaryDirectory
            .appendingPathComponent("openworld-missing-bundles-\(UUID().uuidString)", isDirectory: true)
        let previous = getenv("OPENWORLD_BUNDLES").map { String(cString: $0) }
        defer {
            if let previous {
                setenv("OPENWORLD_BUNDLES", previous, 1)
            } else {
                unsetenv("OPENWORLD_BUNDLES")
            }
        }
        setenv("OPENWORLD_BUNDLES", missing.path, 1)
        try MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            model.choose(try self.still("blank"))
            model.loadBundles()
            XCTAssertTrue(model.bundles.isEmpty)
            XCTAssertEqual(model.step, .bundle)
            XCTAssertEqual(model.error, "The bundle catalog could not be read. Refusing.")
            XCTAssertFalse(model.showsTopError)
            let preview = PhonePreview.lines(screen: "bundle", model: model)
            XCTAssertTrue(preview.contains("Model bundle"))
            XCTAssertEqual(preview.filter { $0 == "The bundle catalog could not be read. Refusing." }.count, 1)
            XCTAssertFalse(preview.contains("The scan program is not on this device. Refusing."))
            XCTAssertEqual(preview.last, "Continue")
            model.continueFromBundles()
            XCTAssertEqual(model.step, .bundle)
            _ = FlowView(model: model, importControl: self.control).body
        }
    }

    func testTheChosenBundleIsMarked() throws {
        XCTAssertTrue(Copy.sizeHint.contains("A face under 64 px on that image is left out."))
        XCTAssertTrue(Copy.sizeHint.contains("the label is \"Not compared.\""))
        XCTAssertFalse(Copy.sizeHint.contains("about 112"))
        XCTAssertEqual(markedChoice("Fast", selected: true), "Fast. Selected.")
        XCTAssertEqual(markedChoice("Accurate", selected: false), "Accurate")
        XCTAssertEqual(markedChoice("Complete. Every decoded frame.", selected: true), "Complete. Every decoded frame. Selected.")
        XCTAssertEqual(detectionChoice("640", selected: "640"), "640 px on the long side. Selected.")
        XCTAssertEqual(detectionChoice("full", selected: "640"), "Full resolution")
        XCTAssertEqual(detectionChoice("full", selected: "full"), "Full resolution. Selected.")
        XCTAssertEqual(coverageChoice("complete", selected: "complete"), "Complete. Every decoded frame. Selected.")
        XCTAssertEqual(coverageChoice("measured", selected: "complete"), "Measured. 5 frames a second, plus the tracker.")
        XCTAssertEqual(coverageChoice("measured", selected: "measured"), "Measured. 5 frames a second, plus the tracker. Selected.")
        try MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            model.choose(try self.still("blank"))
            model.loadBundles()
            XCTAssertEqual(model.bundles.first?.id, "fast")
            XCTAssertEqual(markedChoice("Fast", selected: model.bundleID == "fast"), "Fast. Selected.")
            model.bundleID = "accurate"
            XCTAssertEqual(markedChoice("Accurate", selected: model.bundleID == "accurate"), "Accurate. Selected.")
            XCTAssertEqual(markedChoice("Fast", selected: model.bundleID == "fast"), "Fast")
            _ = FlowView(model: model, importControl: self.control).body
        }
    }

    func testReturningFromTheFilePageKeepsTheChosenBundle() throws {
        try MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            model.choose(try self.still("blank"))
            model.loadBundles()
            XCTAssertEqual(model.bundleID, "fast")
            model.bundleID = "accurate"
            model.goBack()
            XCTAssertEqual(model.step, .device)
            model.loadBundles()
            XCTAssertEqual(model.bundleID, "accurate")
            XCTAssertEqual(model.step, .bundle)
            model.bundleID = "not-in-the-catalog"
            model.loadBundles()
            XCTAssertEqual(model.bundleID, "fast")
            model.bundleID = "accurate"
            model.chooseAnother()
            XCTAssertEqual(model.bundleID, "fast")
            XCTAssertEqual(model.step, .choose)
            model.loadBundles()
            XCTAssertEqual(model.bundleID, "fast")
        }
    }

    func testACandidateWithoutAPageKeepsTheCard() throws {
        try MainActor.assumeIsolated {
            var report = try JSONDecoder().decode(ScanReport.self, from: Data(reportJSON.utf8))
            report.candidates[0].fbiUrl = ""
            let model = FlowModel(phone: true)
            model.report = report
            model.step = .results
            let lines = PhonePreview.lines(screen: "results", model: model)
            XCTAssertTrue(lines.contains("Possible candidate. Not an identification."))
            XCTAssertTrue(lines.contains("Fixture subject A (Missing)"))
            XCTAssertFalse(lines.contains("Open FBI page"))
            _ = FlowView(model: model, importControl: self.control).body
        }
    }

    func testARefusedScanWithoutAResultHidesDelete() throws {
        try MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            model.choose(try self.still("blank"))
            model.bundleID = "not-in-the-catalog"
            model.analyze()
            XCTAssertEqual(model.step, .results)
            XCTAssertEqual(model.report?.summary, "That bundle is not in the catalog. Refusing.")
            XCTAssertNil(model.resultDirectory)
            let lines = PhonePreview.lines(screen: "results", model: model)
            XCTAssertEqual(lines.filter { $0 == "That bundle is not in the catalog. Refusing." }.count, 1)
            XCTAssertFalse(lines.contains("Delete"))
            XCTAssertFalse(lines.contains("No candidate is not a clearance."))
            _ = FlowView(model: model, importControl: self.control).body
            model.chooseAnother()
            XCTAssertEqual(model.step, .choose)
            XCTAssertNil(model.report)
        }
    }

    func testAPosterPackFileRefusesBeforeTheScan() throws {
        let blocker = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try Data("keep".utf8).write(to: blocker)
        defer { try? FileManager.default.removeItem(at: blocker) }
        try MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            model.choose(try self.still("blank"))
            model.fixturePackDirectory = blocker
            model.analyze()
            XCTAssertEqual(model.step, .results)
            XCTAssertNil(model.report)
            XCTAssertEqual(model.error, "The poster pack could not be read. Refusing.")
            let lines = PhonePreview.lines(screen: "results", model: model)
            XCTAssertEqual(lines.filter { $0 == "The poster pack could not be read. Refusing." }.count, 1)
            XCTAssertFalse(lines.contains("The poster pack is missing. Refusing."))
            XCTAssertTrue(lines.contains("Nothing is uploaded."))
            XCTAssertTrue(lines.contains("On-device does not mean the file is real."))
            XCTAssertFalse(lines.contains("No candidate is not a clearance."))
            XCTAssertFalse(lines.contains("Delete"))
            XCTAssertTrue(lines.contains("Choose another file"))
            _ = FlowView(model: model, importControl: self.control).body
        }
        XCTAssertEqual(try Data(contentsOf: blocker), Data("keep".utf8))
    }

    func testAnalyzeSaysScanningUntilTheResultIsReady() async throws {
        let file = try self.still("blank")
        let model = await MainActor.run { () -> FlowModel in
            let model = FlowModel(phone: true)
            model.choose(file)
            model.loadBundles()
            model.loadEstimate()
            XCTAssertTrue(model.canAnalyze)
            model.startScan()
            XCTAssertTrue(model.scanning)
            XCTAssertFalse(model.backEnabled)
            XCTAssertEqual(model.step, .estimate)
            _ = FlowView(model: model, importControl: self.control).body
            model.goBack()
            XCTAssertEqual(model.step, .estimate)
            return model
        }
        let deadline = Date().addingTimeInterval(30)
        while await MainActor.run(body: { model.scanning }) && Date() < deadline {
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        await MainActor.run {
            XCTAssertFalse(model.scanning)
            XCTAssertTrue(model.backEnabled)
            XCTAssertEqual(model.step, .results)
            XCTAssertEqual(model.report?.summary, Copy.clearance)
        }
    }

    func testACropAppearsOnTheEstimateWhileScanning() async throws {
        let file = try self.still("scene")
        let seen = DispatchSemaphore(value: 0)
        let hold = DispatchSemaphore(value: 0)
        let model = await MainActor.run { () -> FlowModel in
            let model = FlowModel(phone: true)
            model.choose(file)
            model.loadBundles()
            model.loadEstimate()
            XCTAssertTrue(model.canAnalyze)
            model.progressSeen = seen
            model.progressHold = hold
            model.startScan()
            XCTAssertTrue(model.scanning)
            XCTAssertEqual(model.step, .estimate)
            XCTAssertFalse(model.backEnabled)
            return model
        }
        let appeared = await withCheckedContinuation { (continuation: CheckedContinuation<Bool, Never>) in
            DispatchQueue.global().async {
                continuation.resume(returning: seen.wait(timeout: .now() + 30) == .success)
            }
        }
        XCTAssertTrue(appeared)
        await MainActor.run {
            XCTAssertFalse(model.liveCrops.isEmpty)
            XCTAssertTrue(model.scanning)
            XCTAssertEqual(model.step, .estimate)
            let label = model.liveCrops[0].label
            let known = [
                Copy.possible,
                Copy.notCompared,
                "A vehicle is not a person.",
                "This plate text is not published on a poster.",
                "The plate could not be read.",
                "No poster publishes a plate.",
                "Below the locked cutoff. Not a candidate.",
                "This face could not be scored.",
            ]
            XCTAssertTrue(known.contains(label), label)
            XCTAssertTrue(model.liveCrops.allSatisfy { $0.frameLabel == "Frame 1." })
            let lines = PhonePreview.lines(screen: "estimate", model: model)
            XCTAssertEqual(lines.first, "Back")
            XCTAssertTrue(lines.contains("Scanning"))
            XCTAssertTrue(lines.contains("Crops from this file."))
            XCTAssertTrue(lines.contains(label))
            XCTAssertTrue(lines.contains("Frame 1."))
            let frameAt = lines.firstIndex(of: "Frame 1.")
            let labelAt = lines.firstIndex(of: label)
            XCTAssertNotNil(frameAt)
            XCTAssertNotNil(labelAt)
            if let frameAt, let labelAt {
                XCTAssertLessThan(frameAt, labelAt)
            }
            let crops = lines.firstIndex(of: "Crops from this file.")
            let title = lines.firstIndex(of: "Scanning")
            XCTAssertEqual(title, 1)
            XCTAssertEqual(crops, 2)
            _ = FlowView(model: model, importControl: self.control).body
            model.goBack()
            XCTAssertEqual(model.step, .estimate)
            hold.signal()
        }
        let deadline = Date().addingTimeInterval(30)
        while await MainActor.run(body: { model.scanning }) && Date() < deadline {
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        await MainActor.run {
            XCTAssertFalse(model.scanning)
            XCTAssertEqual(model.step, .results)
            XCTAssertEqual(model.report?.summary, Copy.possible)
        }
    }

    func testAnUnreadableFileStaysOffTheNextPage() throws {
        try MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            model.choose(URL(fileURLWithPath: "/tmp/openworld-no-such-photo.png"))
            XCTAssertEqual(model.step, .choose)
            XCTAssertNil(model.file)
            XCTAssertEqual(model.error, "The file could not be read. Refusing.")
            XCTAssertTrue(model.showsTopError)
            let choose = PhonePreview.lines(screen: "choose", model: model)
            XCTAssertEqual(choose.first, "The file could not be read. Refusing.")
            XCTAssertEqual(choose.dropFirst().first, "Choose a photo or video")
            XCTAssertEqual(choose.last, "Choose File")
            XCTAssertEqual(choose.filter { $0 == "The file could not be read. Refusing." }.count, 1)
            let file = try self.still("blank")
            model.choose(file)
            XCTAssertEqual(model.step, .device)
            XCTAssertNil(model.error)
            XCTAssertFalse(model.showsTopError)
            let opened = PhonePreview.lines(screen: "device", model: model)
            XCTAssertEqual(opened.first, "Back")
            XCTAssertFalse(opened.contains("The file could not be read. Refusing."))
            let folder = file.deletingLastPathComponent().appendingPathComponent("not-a-file-dir", isDirectory: true)
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            model.choose(folder)
            XCTAssertEqual(model.step, .device)
            XCTAssertEqual(model.file?.path, file.path)
            XCTAssertEqual(model.error, "The file could not be read. Refusing.")
            XCTAssertTrue(model.showsTopError)
            let kept = PhonePreview.lines(screen: "device", model: model)
            XCTAssertEqual(kept.first, "The file could not be read. Refusing.")
            XCTAssertEqual(kept.dropFirst().first, "Back")
            XCTAssertTrue(kept.contains(file.lastPathComponent))
            XCTAssertEqual(kept.filter { $0 == "The file could not be read. Refusing." }.count, 1)
            model.choose(file)
            model.loadBundles()
            model.step = .size
            model.choose(folder)
            XCTAssertEqual(model.step, .size)
            XCTAssertEqual(model.file?.path, file.path)
            let size = PhonePreview.lines(screen: "size", model: model)
            XCTAssertEqual(size.first, "The file could not be read. Refusing.")
            XCTAssertEqual(size.dropFirst().first, "Back")
            XCTAssertEqual(size.dropFirst(2).first, "Detection size")
            XCTAssertEqual(size.filter { $0 == "The file could not be read. Refusing." }.count, 1)
            _ = FlowView(model: model, importControl: self.control).body
        }
    }

    func testAnUnreadableFileKeepsThePageThatAlreadyHasAFile() throws {
        try MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            let file = try self.still("blank")
            model.choose(file)
            model.loadBundles()
            model.loadEstimate()
            XCTAssertNotNil(model.estimate)
            XCTAssertEqual(model.step, .estimate)
            model.choose(URL(fileURLWithPath: "/tmp/openworld-no-such-photo-on-estimate.png"))
            XCTAssertEqual(model.step, .estimate)
            XCTAssertEqual(model.file?.path, file.path)
            XCTAssertNotNil(model.estimate)
            XCTAssertEqual(model.error, "The file could not be read. Refusing.")
            XCTAssertTrue(model.showsTopError)
            let estimate = PhonePreview.lines(screen: "estimate", model: model)
            XCTAssertEqual(estimate.first, "The file could not be read. Refusing.")
            XCTAssertEqual(estimate.dropFirst().first, "Back")
            XCTAssertEqual(estimate.dropFirst(2).first, "Estimate")
            XCTAssertEqual(estimate.filter { $0 == "The file could not be read. Refusing." }.count, 1)
            XCTAssertTrue(estimate.contains("Analyze"))
            model.analyze()
            XCTAssertEqual(model.step, .results)
            let result = try XCTUnwrap(model.resultDirectory)
            XCTAssertTrue(FileManager.default.fileExists(atPath: result.appendingPathComponent("result.json").path))
            model.choose(URL(fileURLWithPath: "/tmp/openworld-no-such-photo-on-result.png"))
            XCTAssertEqual(model.step, .results)
            XCTAssertEqual(model.file?.path, file.path)
            XCTAssertEqual(model.error, "The file could not be read. Refusing.")
            XCTAssertTrue(model.showsTopError)
            XCTAssertTrue(FileManager.default.fileExists(atPath: result.appendingPathComponent("result.json").path))
            let lines = PhonePreview.lines(screen: "results", model: model)
            XCTAssertEqual(lines.first, "The file could not be read. Refusing.")
            XCTAssertEqual(lines.dropFirst().first, "Back")
            XCTAssertEqual(lines.filter { $0 == "The file could not be read. Refusing." }.count, 1)
            XCTAssertTrue(lines.contains("Delete"))
            let next = try self.still("blank")
            model.choose(next)
            XCTAssertEqual(model.step, .device)
            XCTAssertEqual(model.file?.path, next.path)
            XCTAssertNil(model.error)
            XCTAssertNil(model.report)
            XCTAssertNil(model.resultDirectory)
            XCTAssertFalse(FileManager.default.fileExists(atPath: result.path))
            XCTAssertFalse(model.showsTopError)
            let device = PhonePreview.lines(screen: "device", model: model)
            XCTAssertEqual(device.first, "Back")
            XCTAssertFalse(device.contains("The file could not be read. Refusing."))
            XCTAssertFalse(device.contains("Delete"))
            _ = FlowView(model: model, importControl: self.control).body
        }
    }

    func testWantedOffLeavesThatClassOutAndBothOffKeepsAnalyzeOff() throws {
        try MainActor.assumeIsolated {
            let model = try self.scan("scene", wanted: false)
            XCTAssertEqual(model.classArguments(), ["--no-wanted"])
            XCTAssertEqual(model.classLine, "Missing.")
            XCTAssertEqual(markedChoice("Missing", selected: model.includeMissing), "Missing. Selected.")
            XCTAssertEqual(markedChoice("Wanted", selected: model.includeWanted), "Wanted")
            XCTAssertNotEqual(markedChoice("Wanted", selected: false), "Wanted.")
            XCTAssertTrue(model.showsAnalyze)
            XCTAssertEqual(model.report?.classNote, "Missing.")
            XCTAssertTrue(model.report?.candidates.allSatisfy { $0.posterClass == "missing" } == true)
            XCTAssertTrue(model.report?.candidates.allSatisfy { $0.posterClassLabel == "Missing" } == true)
            XCTAssertTrue(model.report?.candidates.allSatisfy { $0.posterLine == "\($0.posterTitle) (Missing)" } == true)
            XCTAssertTrue(model.report?.candidates.contains { $0.kind == "face" } == true)
            XCTAssertFalse(model.report?.candidates.contains { $0.kind == "plate" } == true)
            model.includeMissing = false
            model.applyClassGate()
            XCTAssertEqual(model.classLine, "Choose missing, wanted, or both.")
            XCTAssertFalse(model.canAnalyze)
            XCTAssertTrue(model.showsAnalyze)
            XCTAssertEqual(markedChoice("Missing", selected: false), "Missing")
            XCTAssertEqual(markedChoice("Wanted", selected: false), "Wanted")
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
            XCTAssertEqual(model.estimateWarning, Copy.oldFile)
            XCTAssertTrue(model.showBriefOnEstimate)
            XCTAssertEqual(model.choiceLine, "Fast. 640 px on the long side. 5 frames a second, plus the tracker.")
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

    func testAnAnimatedGifKeepsAFaceThatIsNotOnTheFirstFrame() throws {
        let gif = try laterFrameGif()
        let frames = FileManager.default.temporaryDirectory.appendingPathComponent("ow-gif-\(UUID().uuidString)")
        let reel = try GifFrames.write(gif, directory: frames)
        XCTAssertEqual(reel.frames.count, 3)
        XCTAssertTrue(reel.video)
        XCTAssertEqual(reel.fps, 5, accuracy: 0.05)
        MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            model.choose(gif)
            model.loadBundles()
            model.longSide = "640"
            model.coverage = "complete"
            model.loadEstimate()
            XCTAssertTrue(model.canAnalyze)
            XCTAssertNil(model.error)
            model.analyze()
            XCTAssertEqual(model.report?.summary, Copy.possible)
            XCTAssertFalse(model.report?.candidates.isEmpty == true)
            XCTAssertTrue(model.report?.candidates.allSatisfy { $0.frameIndex == 1 } == true)
            XCTAssertTrue(model.report?.candidates.allSatisfy { $0.frameLabel == "Frame 2." } == true)
            XCTAssertTrue(model.report?.inventory.allSatisfy { $0.frameLabel == "Frame \($0.frameIndex + 1)." } == true)
            XCTAssertTrue(model.report?.inventory.contains { $0.frameLabel == "Frame 2." } == true)
        }
    }

    func testAOneFrameGifStaysOneFrame() throws {
        let gif = FileManager.default.temporaryDirectory.appendingPathComponent("ow-one-\(UUID().uuidString).gif")
        try ffmpeg(["-f", "lavfi", "-i", "color=c=blue:s=64x64", "-frames:v", "1", gif.path])
        let reel = try GifFrames.read(gif)
        XCTAssertEqual(reel.frames.count, 1)
        XCTAssertFalse(reel.video)
        MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            model.choose(gif)
            model.loadBundles()
            model.longSide = "640"
            model.coverage = "complete"
            model.loadEstimate()
            XCTAssertTrue(model.canAnalyze)
            model.analyze()
            XCTAssertEqual(model.report?.summary, Copy.clearance)
            XCTAssertTrue(model.report?.candidates.isEmpty == true)
        }
    }

    func testABrokenGifIsRefused() throws {
        let gif = FileManager.default.temporaryDirectory.appendingPathComponent("ow-bad-\(UUID().uuidString).gif")
        var bytes = Array("GIF89a".utf8)
        bytes.append(contentsOf: [UInt8](repeating: 0, count: 24))
        try Data(bytes).write(to: gif)
        MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            model.choose(gif)
            model.coverage = "measured"
            model.loadEstimate()
            XCTAssertEqual(model.step, .estimate)
            XCTAssertNil(model.estimate)
            XCTAssertFalse(model.canAnalyze)
            XCTAssertFalse(model.showsAnalyze)
            XCTAssertFalse(model.showBriefOnEstimate)
            XCTAssertTrue(model.error?.contains("Refusing") == true)
            XCTAssertFalse(model.error?.contains("Choose missing, wanted, or both.") == true)
            _ = FlowView(model: model, importControl: self.control).body
        }
    }

    func testAVideoWithoutAnExtensionIsAMovieContainer() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("ow-bare-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let named = root.appendingPathComponent("clip.mp4")
        try ffmpeg(["-f", "lavfi", "-i", "color=c=black:s=32x32:r=10:d=0.2", "-an", "-c:v", "libx264", "-pix_fmt", "yuv420p", named.path])
        let bare = root.appendingPathComponent("clip")
        try FileManager.default.copyItem(at: named, to: bare)
        XCTAssertTrue(StillMotion.movieContainer(bare))
        XCTAssertTrue(StillMotion.movieContainer(named))
        let webm = root.appendingPathComponent("clip.webm")
        try ffmpeg(["-f", "lavfi", "-i", "color=c=black:s=32x32:r=10:d=0.2", "-an", "-c:v", "libvpx", webm.path])
        let bareWebm = root.appendingPathComponent("webm")
        try FileManager.default.copyItem(at: webm, to: bareWebm)
        XCTAssertTrue(StillMotion.movieContainer(bareWebm))
        let avi = root.appendingPathComponent("avi")
        let aviBytes = Array("RIFF".utf8) + [UInt8](repeating: 0, count: 4) + Array("AVI ".utf8)
        try Data(aviBytes).write(to: avi)
        XCTAssertTrue(StillMotion.movieContainer(avi))
        let heic = root.appendingPathComponent("still")
        let heicBytes = [UInt8](repeating: 0, count: 4) + Array("ftypheic".utf8)
        try Data(heicBytes).write(to: heic)
        XCTAssertFalse(StillMotion.movieContainer(heic))
        let png = try still("blank")
        XCTAssertFalse(StillMotion.movieContainer(png))
        let jpeg = root.appendingPathComponent("photo")
        try Data([0xFF, 0xD8, 0xFF, 0xD9] + [UInt8](repeating: 0, count: 8)).write(to: jpeg)
        XCTAssertFalse(StillMotion.movieContainer(jpeg))
    }

    func testAJpegNamedPngIsNotAPngHeader() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("ow-pnghdr-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let jpeg = root.appendingPathComponent("photo.png")
        try Data([0xFF, 0xD8, 0xFF, 0xD9] + [UInt8](repeating: 0, count: 8)).write(to: jpeg)
        XCTAssertFalse(StillMotion.png(jpeg))
        let png = try still("blank")
        let pngNamedJpeg = root.appendingPathComponent("scene.jpg")
        try FileManager.default.copyItem(at: png, to: pngNamedJpeg)
        XCTAssertTrue(StillMotion.png(pngNamedJpeg))
        XCTAssertTrue(StillMotion.png(png))
    }

    func testAnAnimatedPngIsRefusedInsteadOfClearingTheFirstFrame() throws {
        let apng = try movingPicture("apng")
        XCTAssertTrue(StillMotion.animatedPng(apng))
        XCTAssertFalse(StillMotion.animatedPng(try still("blank")))
        MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            model.choose(apng)
            model.loadEstimate()
            XCTAssertEqual(model.step, .estimate)
            XCTAssertNil(model.estimate)
            XCTAssertFalse(model.canAnalyze)
            XCTAssertTrue(model.error?.contains("not fully decoded") == true)
            XCTAssertTrue(model.error?.contains("Refusing") == true)
        }
    }

    func testAnAnimatedWebpIsRefusedInsteadOfClearingTheFirstFrame() throws {
        let webp = try movingPicture("webp")
        let stillWebp = FileManager.default.temporaryDirectory.appendingPathComponent("ow-still-\(UUID().uuidString).webp")
        try ffmpeg(["-f", "lavfi", "-i", "color=c=blue:s=16x16", "-frames:v", "1", "-c:v", "libwebp", stillWebp.path])
        XCTAssertTrue(StillMotion.animatedWebp(webp))
        XCTAssertFalse(StillMotion.animatedWebp(stillWebp))
        MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            model.choose(webp)
            model.loadEstimate()
            XCTAssertFalse(model.canAnalyze)
            XCTAssertTrue(model.error?.contains("not fully decoded") == true)
            XCTAssertTrue(model.error?.contains("Refusing") == true)
        }
    }

    func testOrientationSixTurnsTheStoredPixelsUpright() {
        let rgb: [UInt8] = [
            1, 0, 0, 2, 0, 0, 3, 0, 0,
            4, 0, 0, 5, 0, 0, 6, 0, 0,
        ]
        let shown = JpegOrientation.apply(rgb: rgb, width: 3, height: 2, tag: 6)
        XCTAssertEqual(shown.width, 2)
        XCTAssertEqual(shown.height, 3)
        XCTAssertEqual(shown.rgb, [
            4, 0, 0, 1, 0, 0,
            5, 0, 0, 2, 0, 0,
            6, 0, 0, 3, 0, 0,
        ])
        let same = JpegOrientation.apply(rgb: rgb, width: 3, height: 2, tag: 1)
        XCTAssertEqual(same.rgb, rgb)
        XCTAssertEqual(same.width, 3)
        let mirrored = JpegOrientation.apply(rgb: rgb, width: 3, height: 2, tag: 2)
        XCTAssertEqual(mirrored.rgb, [
            3, 0, 0, 2, 0, 0, 1, 0, 0,
            6, 0, 0, 5, 0, 0, 4, 0, 0,
        ])
    }

    func testATurnedVideoWithNonSquarePixelsUsesTheShownSize() {
        let upright = VideoDisplay.shownSize(codedWidth: 480, codedHeight: 320, sarNum: 1, sarDen: 2, quarterTurn: true)
        XCTAssertEqual(upright.0, 640)
        XCTAssertEqual(upright.1, 480)
        let squeezed = VideoDisplay.shownSize(codedWidth: 320, codedHeight: 480, sarNum: 2, sarDen: 1, quarterTurn: true)
        XCTAssertEqual(squeezed.0, 240)
        XCTAssertEqual(squeezed.1, 320)
        let wide = VideoDisplay.shownSize(codedWidth: 320, codedHeight: 480, sarNum: 2, sarDen: 1, quarterTurn: false)
        XCTAssertEqual(wide.0, 640)
        XCTAssertEqual(wide.1, 480)
        let turned = VideoDisplay.shownSize(codedWidth: 480, codedHeight: 640, sarNum: 1, sarDen: 1, quarterTurn: true)
        XCTAssertEqual(turned.0, 640)
        XCTAssertEqual(turned.1, 480)
        XCTAssertEqual(VideoDisplay.squareWidth(320, num: 2, den: 1), 640)
        XCTAssertEqual(VideoDisplay.frameCount(pictureSeconds: 0.8, rate: 10), 8)
        XCTAssertEqual(VideoDisplay.frameCount(pictureSeconds: 30, rate: 10), 300)
        XCTAssertEqual(VideoDisplay.frameCount(pictureSeconds: 0, rate: 10), 1)
    }

    func testAJpegOrientationTagIsReadFromTheFile() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("ow-orient-\(UUID().uuidString)")
        try Data(jpegWithOrientation(6)).write(to: url)
        XCTAssertEqual(JpegOrientation.tag(url), 6)
        let size = JpegOrientation.displaySize(width: 480, height: 640, tag: 6)
        XCTAssertEqual(size.0, 640)
        XCTAssertEqual(size.1, 480)
        XCTAssertEqual(JpegOrientation.tag(Data([0x89, 0x50, 0x4E, 0x47])), 1)
        XCTAssertEqual(JpegOrientation.tag(Data(pngWithOrientation(6))), 6)
        XCTAssertEqual(JpegOrientation.tag(Data(pngWithOrientation(1))), 1)
        let webp = FileManager.default.temporaryDirectory.appendingPathComponent("ow-webp-\(UUID().uuidString)")
        try Data(webpWithOrientation(6, prefix: false)).write(to: webp)
        XCTAssertEqual(JpegOrientation.tag(webp), 6)
        XCTAssertEqual(JpegOrientation.tag(Data(webpWithOrientation(6, prefix: true))), 6)
        XCTAssertEqual(JpegOrientation.tag(Data(webpWithOrientation(1, prefix: false))), 1)
        let pages = rgbTiff([(1, 1, [1, 2, 3], 6), (1, 1, [4, 5, 6], 8)])
        XCTAssertEqual(JpegOrientation.tiffPageTags(Data(pages)), [6, 8])
        XCTAssertEqual(JpegOrientation.tag(Data(pages)), 6)
        let tiff = FileManager.default.temporaryDirectory.appendingPathComponent("ow-tif-\(UUID().uuidString)")
        try Data(pages).write(to: tiff)
        XCTAssertEqual(JpegOrientation.tag(tiff), 6)
        XCTAssertEqual(JpegOrientation.tag(Data(rgbTiff([(1, 1, [9, 9, 9], 1)]))), 1)
        let big = Data([
            0x4D, 0x4D, 0x00, 0x2A,
            0x00, 0x00, 0x00, 0x08,
            0x00, 0x01,
            0x01, 0x12,
            0x00, 0x03,
            0x00, 0x00, 0x00, 0x01,
            0x00, 0x06,
            0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ])
        XCTAssertEqual(JpegOrientation.tiffPageTags(big), [6])
        XCTAssertNil(JpegOrientation.tiffPageTags(Data([0x89, 0x50, 0x4E, 0x47])))
    }

    func testAnInterlacedGifKeepsThePixelsOfThatFrame() throws {
        let gif = FileManager.default.temporaryDirectory.appendingPathComponent("ow-inter-\(UUID().uuidString).gif")
        try Data(interlacedGif).write(to: gif)
        let reel = try GifFrames.read(gif)
        XCTAssertEqual(reel.width, 16)
        XCTAssertEqual(reel.height, 16)
        XCTAssertEqual(reel.frames.count, 1)
        let rgb = reel.frames[0]
        for y in 0..<16 {
            for x in 0..<16 {
                let pixel = (y * 16 + x) * 3
                let expected: UInt8 = (x < 8 && y < 8) ? 0 : 255
                XCTAssertEqual(rgb[pixel], expected)
                XCTAssertEqual(rgb[pixel + 1], expected)
                XCTAssertEqual(rgb[pixel + 2], expected)
            }
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
            XCTAssertFalse(model.canAnalyze)
            XCTAssertTrue(model.error?.contains("AVFoundation decodes on macOS and iOS. Refusing.") == true)
            model.goBack()
            XCTAssertEqual(model.step, .size)
            model.chooseAnother()
            XCTAssertEqual(model.step, .choose)
            XCTAssertNil(model.file)
            XCTAssertNil(model.error)
            XCTAssertTrue(model.canAnalyze)
            _ = FlowView(model: model, importControl: self.control).body
        }
    }

    func testChooseAnotherClearsACandidate() throws {
        try MainActor.assumeIsolated {
            let model = try self.scan("scene")
            model.leavingURL = URL(string: "https://www.fbi.gov/wanted")
            model.leaveError = "OpenWorld only opens an FBI page."
            model.goBack()
            XCTAssertEqual(model.step, .estimate)
            XCTAssertNil(model.leavingURL)
            XCTAssertNil(model.leaveError)
            let result = model.resultDirectory
            model.chooseAnother()
            XCTAssertEqual(model.step, .choose)
            XCTAssertNil(model.report)
            XCTAssertNil(model.file)
            XCTAssertEqual(model.coverage, "complete")
            XCTAssertEqual(model.longSide, "640")
            if let result {
                XCTAssertFalse(FileManager.default.fileExists(atPath: result.path))
            }
            _ = FlowView(model: model, importControl: self.control).body
        }
    }

    func testDeleteRemovesTheResultAndLeavesAForeignDirectory() throws {
        try MainActor.assumeIsolated {
            let model = try self.scan("scene")
            let result = try XCTUnwrap(model.resultDirectory)
            let frames = result.deletingLastPathComponent().appendingPathComponent("frames")
            XCTAssertTrue(FileManager.default.fileExists(atPath: result.appendingPathComponent("result.json").path))
            XCTAssertTrue(FileManager.default.fileExists(atPath: frames.path))
            _ = FlowView(model: model, importControl: self.control).body
            model.deleteResult()
            XCTAssertEqual(model.error, "Deleted.")
            XCTAssertNil(model.report)
            XCTAssertNil(model.resultDirectory)
            XCTAssertFalse(FileManager.default.fileExists(atPath: result.path))
            XCTAssertFalse(FileManager.default.fileExists(atPath: frames.path))
            _ = FlowView(model: model, importControl: self.control).body

            let foreign = FileManager.default.temporaryDirectory.appendingPathComponent("ow-foreign-\(UUID().uuidString)")
            try FileManager.default.createDirectory(at: foreign, withIntermediateDirectories: true)
            try Data("keep".utf8).write(to: foreign.appendingPathComponent("notes.txt"))
            let process = Process()
            process.executableURL = URL(fileURLWithPath: self.binary())
            process.arguments = PhoneArguments.delete(out: foreign.path)
            let pipe = Pipe()
            process.standardOutput = pipe
            try process.run()
            process.waitUntilExit()
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            let object = try XCTUnwrap(try JSONSerialization.jsonObject(with: data) as? [String: Any])
            XCTAssertEqual(object["deleted"] as? Bool, false)
            XCTAssertTrue((object["message"] as? String)?.contains("not an OpenWorld result") == true)
            XCTAssertTrue(FileManager.default.fileExists(atPath: foreign.appendingPathComponent("notes.txt").path))
            try FileManager.default.removeItem(at: foreign)
        }
    }

    func testARefusedDeleteKeepsTheResultOnScreen() throws {
        try MainActor.assumeIsolated {
            let model = try self.scan("scene")
            let result = try XCTUnwrap(model.resultDirectory)
            try FileManager.default.removeItem(at: result.appendingPathComponent("result.json"))
            model.deleteResult()
            XCTAssertEqual(model.report?.summary, "Possible candidate. Not an identification.")
            XCTAssertFalse(model.report?.candidates.isEmpty ?? true)
            XCTAssertEqual(model.deleteNotice, "Refusing to delete a directory that is not an OpenWorld result.")
            XCTAssertTrue(FileManager.default.fileExists(atPath: result.path))
            XCTAssertNil(model.error)
            _ = FlowView(model: model, importControl: self.control).body
            model.goBack()
            XCTAssertEqual(model.step, .estimate)
            model.deleteNotice = "The result could not be deleted."
            _ = FlowView(model: model, importControl: self.control).body
            model.deleteNotice = nil
            model.analyze()
            XCTAssertEqual(model.step, .results)
            XCTAssertEqual(model.report?.summary, "Possible candidate. Not an identification.")
            XCTAssertTrue(FileManager.default.fileExists(atPath: result.path))
            XCTAssertNotEqual(model.resultDirectory?.path, result.path)
        }
    }

    func testARefusalLeavesTheBundleUnnamed() throws {
        try MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            model.report = try JSONDecoder().decode(ScanReport.self, from: Data(refusedJSON.utf8))
            model.step = .results
            let preview = PhonePreview.lines(screen: "results", model: model)
            XCTAssertEqual(preview.first, "Back")
            XCTAssertEqual(preview.dropFirst().first, "The file could not be read. Refusing.")
            XCTAssertFalse(preview.contains { $0.hasPrefix("Bundle:") })
            XCTAssertFalse(preview.contains("Fixture markers were read."))
            XCTAssertFalse(preview.contains(""))
            XCTAssertFalse(preview.contains(Copy.clearance))
            XCTAssertTrue(preview.contains("Nothing is uploaded."))
            XCTAssertEqual(preview.last, "Choose another file")
            XCTAssertEqual(PhonePreview.contextLines(model.report), [])
            _ = FlowView(model: model, importControl: self.control).body
        }
    }

    func testAnIncompleteReportKeepsTheReason() throws {
        try MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            model.report = try JSONDecoder().decode(ScanReport.self, from: Data(incompleteJSON.utf8))
            model.step = .results
            XCTAssertEqual(model.report?.summary, Copy.incomplete)
            XCTAssertEqual(model.report?.status, "incomplete")
            XCTAssertEqual(model.report?.message, "The file was not fully decoded.")
            XCTAssertNotEqual(model.report?.message, model.report?.summary)
            _ = FlowView(model: model, importControl: self.control).body
        }
    }

    func testLinuxSizeAndCoverageButtonsChangeTheSelection() {
        MainActor.assumeIsolated {
            let model = FlowModel(phone: true)
            model.step = .size
            model.longSide = "640"
            model.coverage = "complete"
            _ = FlowView(model: model, importControl: self.control).body
            model.longSide = "320"
            model.coverage = "measured"
            XCTAssertEqual(model.longSide, "320")
            XCTAssertEqual(model.coverage, "measured")
            _ = FlowView(model: model, importControl: self.control).body
        }
    }

    @MainActor
    private var control: AnyView {
        AnyView(Button(action: {}) { Text("Choose File") })
    }

    @MainActor
    private func scan(_ kind: String, coverage: String = "complete", wanted: Bool = true) throws -> FlowModel {
        let model = FlowModel(phone: true)
        model.choose(try still(kind))
        model.loadBundles()
        model.longSide = "640"
        model.coverage = coverage
        model.includeWanted = wanted
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

    private func laterFrameGif() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("ow-later-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let scene = try still("scene")
        let blank = try still("blank")
        let size = try pngSize(scene)
        let wide = root.appendingPathComponent("wide.png")
        try ffmpeg(["-i", blank.path, "-vf", "scale=\(size.0):\(size.1):flags=neighbor", "-frames:v", "1", wide.path])
        try ffmpeg(["-i", wide.path, "-vf", "drawbox=x=0:y=0:w=1:h=1:color=black:t=fill", root.appendingPathComponent("f0.png").path])
        try FileManager.default.copyItem(at: scene, to: root.appendingPathComponent("f1.png"))
        try ffmpeg(["-i", wide.path, "-vf", "drawbox=x=20:y=20:w=1:h=1:color=black:t=fill", root.appendingPathComponent("f2.png").path])
        let palette = root.appendingPathComponent("pal.png")
        try ffmpeg([
            "-framerate", "5", "-start_number", "0", "-i", root.appendingPathComponent("f%d.png").path,
            "-frames:v", "3", "-vf", "palettegen=stats_mode=full:max_colors=8", palette.path,
        ])
        let gif = root.appendingPathComponent("later.gif")
        try ffmpeg([
            "-framerate", "5", "-start_number", "0", "-i", root.appendingPathComponent("f%d.png").path,
            "-i", palette.path, "-frames:v", "3", "-lavfi", "paletteuse=dither=none", "-loop", "0", gif.path,
        ])
        return gif
    }

    private func pngSize(_ url: URL) throws -> (Int, Int) {
        let data = try Data(contentsOf: url)
        let bytes = [UInt8](data.prefix(24))
        XCTAssertEqual(bytes.count, 24)
        let width = (Int(bytes[16]) << 24) | (Int(bytes[17]) << 16) | (Int(bytes[18]) << 8) | Int(bytes[19])
        let height = (Int(bytes[20]) << 24) | (Int(bytes[21]) << 16) | (Int(bytes[22]) << 8) | Int(bytes[23])
        return (width, height)
    }

    private func movingPicture(_ kind: String) throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("ow-move-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try ffmpeg([
            "-f", "lavfi", "-i", "testsrc2=size=32x32:rate=5:duration=0.6",
            "-start_number", "0", root.appendingPathComponent("f%d.png").path,
        ])
        let out = root.appendingPathComponent("move.\(kind)")
        if kind == "apng" {
            try ffmpeg([
                "-framerate", "5", "-start_number", "0", "-i", root.appendingPathComponent("f%d.png").path,
                "-frames:v", "3", "-plays", "1", "-f", "apng", out.path,
            ])
        } else {
            try ffmpeg([
                "-framerate", "5", "-start_number", "0", "-i", root.appendingPathComponent("f%d.png").path,
                "-frames:v", "3", "-loop", "0", "-c:v", "libwebp", out.path,
            ])
        }
        return out
    }

    private func ffmpeg(_ args: [String]) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/ffmpeg")
        process.arguments = ["-y", "-v", "error"] + args
        let pipe = Pipe()
        process.standardError = pipe
        process.standardOutput = pipe
        try process.run()
        process.waitUntilExit()
        if process.terminationStatus != 0 {
            let text = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
            XCTFail(text)
        }
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

        model.choose(try self.still("blank"))
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
        let leaving = PhonePreview.lines(screen: "leaving", model: model)
        let pageAt = try XCTUnwrap(leaving.firstIndex(of: "https://www.fbi.gov/wanted"))
        let openAt = try XCTUnwrap(leaving.firstIndex(of: "Open"))
        let stayAt = try XCTUnwrap(leaving.firstIndex(of: "Stay"))
        XCTAssertLessThan(pageAt, openAt)
        XCTAssertLessThan(openAt, stayAt)
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
        let score = "Score 0.98. Fast keeps a candidate at 0.55 and above."
        let preview = PhonePreview.lines(screen: "results", model: model)
        XCTAssertTrue(preview.contains(score))
        XCTAssertFalse(preview.contains { $0.contains("Cosine") || $0.contains("locked cutoff") })
        XCTAssertFalse(preview.contains { $0.contains("keeps a candidate") && $0.contains(Copy.possible) })
        XCTAssertTrue(preview.contains("1 frame analyzed."))
        XCTAssertTrue(preview.contains("Missing and wanted."))
        XCTAssertTrue(preview.contains("640 px on the long side."))
        XCTAssertTrue(preview.contains("Every decoded frame."))
        if let scoreAt = preview.firstIndex(of: score),
           let posterAt = preview.firstIndex(of: "Fixture subject A (Missing)") {
            XCTAssertLessThan(scoreAt, posterAt)
        } else {
            XCTFail("the score line or the poster line was missing")
        }
    }

    private func row() throws -> BundleRow {
        try JSONDecoder().decode(BundleRow.self, from: Data("""
        {"id":"fast","name":"Fast","best_for":"Phones and long video.","curve_line":"Fixture curve measured. Real FBI photos stay off.","preselected":true}
        """.utf8))
    }
}

private func webpWithOrientation(_ tag: UInt8, prefix: Bool) -> [UInt8] {
    let tiff: [UInt8] = [
        0x4D, 0x4D, 0x00, 0x2A,
        0x00, 0x00, 0x00, 0x08,
        0x00, 0x01,
        0x01, 0x12,
        0x00, 0x03,
        0x00, 0x00, 0x00, 0x01,
        0x00, tag,
        0x00, 0x00,
    ]
    let exif = (prefix ? [0x45, 0x78, 0x69, 0x66, 0x00, 0x00] : []) + tiff
    let vp8x: [UInt8] = [0x08, 0, 0, 0, 0xDF, 0x01, 0x00, 0x7F, 0x02, 0x00]
    let body = Array("WEBP".utf8) + riffChunk("VP8X", vp8x) + riffChunk("EXIF", exif)
    return Array("RIFF".utf8) + le32(body.count) + body
}

private func riffChunk(_ tag: String, _ payload: [UInt8]) -> [UInt8] {
    var out = Array(tag.utf8) + le32(payload.count) + payload
    if payload.count % 2 == 1 { out.append(0) }
    return out
}

private func le32(_ value: Int) -> [UInt8] {
    [
        UInt8(value & 0xFF),
        UInt8((value >> 8) & 0xFF),
        UInt8((value >> 16) & 0xFF),
        UInt8((value >> 24) & 0xFF),
    ]
}

private func pngWithOrientation(_ tag: UInt8) -> [UInt8] {
    let tiff: [UInt8] = [
        0x49, 0x49, 0x2A, 0x00,
        0x08, 0x00, 0x00, 0x00,
        0x01, 0x00,
        0x12, 0x01,
        0x03, 0x00,
        0x01, 0x00, 0x00, 0x00,
        tag, 0x00,
        0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ]
    let signature: [UInt8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
    let chunk = be32(tiff.count) + Array("eXIf".utf8) + tiff + be32(0)
    let end = be32(0) + Array("IEND".utf8) + be32(0)
    return signature + chunk + end
}

private func be32(_ value: Int) -> [UInt8] {
    [
        UInt8((value >> 24) & 0xFF),
        UInt8((value >> 16) & 0xFF),
        UInt8((value >> 8) & 0xFF),
        UInt8(value & 0xFF),
    ]
}

private func rgbTiff(_ pages: [(Int, Int, [UInt8], UInt8)]) -> [UInt8] {
    let entryCount = 10
    let ifdLen = 2 + entryCount * 12 + 4
    var cursor = 8
    var layout: [(Int, Int, Int)] = []
    for (width, height, rgb, _) in pages {
        if rgb.count != width * height * 3 { return [] }
        let ifd = cursor
        let bits = ifd + ifdLen
        let pixels = bits + 6
        cursor = pixels + rgb.count
        layout.append((ifd, bits, pixels))
    }
    var out = [UInt8](repeating: 0, count: cursor)
    out[0] = 0x49
    out[1] = 0x49
    out[2] = 0x2A
    out[4] = 8
    for index in pages.indices {
        let (width, height, rgb, tag) = pages[index]
        let (ifd, bits, pixels) = layout[index]
        let next = index + 1 < layout.count ? layout[index + 1].0 : 0
        put16(&out, ifd, entryCount)
        let entries: [(Int, Int, Int, Int)] = [
            (256, 4, 1, width),
            (257, 4, 1, height),
            (258, 3, 3, bits),
            (259, 3, 1, 1),
            (262, 3, 1, 2),
            (273, 4, 1, pixels),
            (274, 3, 1, Int(tag)),
            (277, 3, 1, 3),
            (278, 4, 1, height),
            (279, 4, 1, rgb.count),
        ]
        var at = ifd + 2
        for (entryTag, kind, count, value) in entries {
            put16(&out, at, entryTag)
            put16(&out, at + 2, kind)
            put32(&out, at + 4, count)
            put32(&out, at + 8, value)
            at += 12
        }
        put32(&out, at, next)
        put16(&out, bits, 8)
        put16(&out, bits + 2, 8)
        put16(&out, bits + 4, 8)
        for (offset, byte) in rgb.enumerated() {
            out[pixels + offset] = byte
        }
    }
    return out
}

private func put16(_ out: inout [UInt8], _ offset: Int, _ value: Int) {
    out[offset] = UInt8(value & 0xFF)
    out[offset + 1] = UInt8((value >> 8) & 0xFF)
}

private func put32(_ out: inout [UInt8], _ offset: Int, _ value: Int) {
    put16(&out, offset, value & 0xFFFF)
    put16(&out, offset + 2, (value >> 16) & 0xFFFF)
}

private func jpegWithOrientation(_ tag: UInt8) -> [UInt8] {
    let tiff: [UInt8] = [
        0x49, 0x49, 0x2A, 0x00,
        0x08, 0x00, 0x00, 0x00,
        0x01, 0x00,
        0x12, 0x01,
        0x03, 0x00,
        0x01, 0x00, 0x00, 0x00,
        tag, 0x00,
        0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ]
    let payload: [UInt8] = [0x45, 0x78, 0x69, 0x66, 0x00, 0x00] + tiff
    let length = payload.count + 2
    return [0xFF, 0xD8, 0xFF, 0xE1, UInt8(length >> 8), UInt8(length & 0xFF)] + payload + [0xFF, 0xD9]
}

private let interlacedGif = Data(hex: "47494638376110001000810000ffffff0000000000000000002c000000001000100040082f0003081c281080c18308132a3c4890e0c287101b0e7c28b120c48b182356a4b87161c5001c25661c49b2a449830101003b")

private extension Data {
    init(hex: String) {
        var bytes: [UInt8] = []
        var index = hex.startIndex
        while index < hex.endIndex {
            let next = hex.index(index, offsetBy: 2)
            bytes.append(UInt8(hex[index..<next], radix: 16) ?? 0)
            index = next
        }
        self.init(bytes)
    }
}

private let refusedJSON = """
{"status":"refused","summary":"The file could not be read. Refusing.","message":"The file could not be read. Refusing.","bundle_name":"","perception_note":"","disclosure":["Nothing is uploaded.","Nobody is enrolled.","OpenWorld does not train on this file.","OpenWorld does not contact an agency.","A candidate is not an identification.","This file is not authenticated.","On-device does not mean the file is real."],"warnings":[],"faces_seen_not_compared":0,"inventory":[],"candidates":[],"comparisons":[]}
"""

private let incompleteJSON = """
{"status":"incomplete","summary":"Incomplete.","message":"The file was not fully decoded.","disclosure":["Nothing is uploaded."],"warnings":[],"faces_seen_not_compared":0,"inventory":[],"candidates":[],"comparisons":[]}
"""

private let estimateJSON = """
{"human":"Less than a second","caveat":"This is a planning estimate, not a thermal measurement.","device_note":"This scan runs on the CPU. It will be slower, warmer, and use more battery.","heat_note":"This phone may get hot. If heat or the system stops the scan, the result is Incomplete.","battery_note":"A long scan uses a lot of battery."}
"""

private let reportJSON = """
{"status":"complete","summary":"Possible candidate. Not an identification.","bundle_name":"Fast","frames_note":"1 frame analyzed.","class_note":"Missing and wanted.","detection_note":"640 px on the long side.","coverage_note":"Every decoded frame.","perception_note":"Fixture markers were read.","disclosure":["Nothing is uploaded.","Nobody is enrolled.","OpenWorld does not train on this file.","OpenWorld does not contact an agency.","A candidate is not an identification.","No candidate is not a clearance.","This file is not authenticated.","On-device does not mean the file is real."],"warnings":["The file timestamps disagree."],"faces_seen_not_compared":1,"inventory":[{"kind":"face","label":"Possible candidate. Not an identification.","frame_index":0,"frame_label":"Frame 1."},{"kind":"face","label":"Not compared.","frame_index":0,"frame_label":"Frame 1."}],"candidates":[{"wording":"Possible candidate. Not an identification.","kind":"face","uncertainty":"Score 0.98. Fast keeps a candidate at 0.55 and above.","poster_title":"Fixture subject A","poster_class":"missing","poster_class_label":"Missing","fbi_url":"https://www.fbi.gov/wanted","leaving":"You are leaving OpenWorld.","frame_index":0,"frame_label":"Frame 1.","poster_id":"a"}],"comparisons":[]}
"""
#endif
