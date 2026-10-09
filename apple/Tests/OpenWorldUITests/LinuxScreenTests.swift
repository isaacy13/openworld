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
            if ProcessInfo.processInfo.environment["OPENWORLD_LIB"] != nil {
                XCTAssertTrue(LinkedCore.isLinked)
            }
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
            model.loadEstimate()
            XCTAssertEqual(model.step, .estimate)
            XCTAssertNil(model.estimate)
            XCTAssertFalse(model.canAnalyze)
            XCTAssertTrue(model.error?.contains("Refusing") == true)
        }
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
        let webp = FileManager.default.temporaryDirectory.appendingPathComponent("ow-webp-\(UUID().uuidString)")
        try Data(webpWithOrientation(6, prefix: false)).write(to: webp)
        XCTAssertEqual(JpegOrientation.tag(webp), 6)
        XCTAssertEqual(JpegOrientation.tag(Data(webpWithOrientation(6, prefix: true))), 6)
        XCTAssertEqual(JpegOrientation.tag(Data(webpWithOrientation(1, prefix: false))), 1)
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

private let incompleteJSON = """
{"status":"incomplete","summary":"Incomplete.","message":"The file was not fully decoded.","disclosure":["Nothing is uploaded."],"warnings":[],"faces_seen_not_compared":0,"inventory":[],"candidates":[],"comparisons":[]}
"""

private let estimateJSON = """
{"human":"Less than a second","caveat":"This is a planning estimate, not a thermal measurement.","device_note":"This scan runs on the CPU. It will be slower, warmer, and use more battery.","heat_note":"This phone may get hot. If heat or the system stops the scan, the result is Incomplete.","battery_note":"A long scan uses a lot of battery."}
"""

private let reportJSON = """
{"status":"complete","summary":"Possible candidate. Not an identification.","bundle_name":"Fast","perception_note":"Fixture markers were read.","disclosure":["Nothing is uploaded.","Nobody is enrolled.","OpenWorld does not train on this file.","OpenWorld does not contact an agency.","A candidate is not an identification.","No candidate is not a clearance.","This file is not authenticated.","On-device does not mean the file is real."],"warnings":["The file timestamps disagree."],"faces_seen_not_compared":1,"inventory":[{"kind":"face","label":"Possible candidate. Not an identification.","frame_index":0},{"kind":"face","label":"Not compared.","frame_index":0}],"candidates":[{"wording":"Possible candidate. Not an identification.","kind":"face","uncertainty":"Cosine 0.98 is above the locked cutoff 0.55 for Fast. Possible candidate. Not an identification.","poster_title":"Fixture subject A","poster_class":"missing","fbi_url":"https://www.fbi.gov/wanted","leaving":"You are leaving OpenWorld.","frame_index":0,"poster_id":"a"}],"comparisons":[]}
"""
#endif
