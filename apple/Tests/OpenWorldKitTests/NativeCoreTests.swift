// SPDX-License-Identifier: Apache-2.0
@testable import OpenWorldKit
import XCTest

/// The real C API, end to end. Runs when the kit was built with the core linked
/// (`OPENWORLD_LIB_DIR` on Linux, the xcframework in Xcode). Skipped otherwise.
@MainActor
final class NativeCoreTests: XCTestCase {
    // XCTest makes a new instance for every test.
    let core: (any Core)? = Linked.core()
    let places: Places = {
        // The repository's bundle catalog, next to apple/.
        let repo = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        var places = try! temporaryPlaces()
        places.bundles = repo.appendingPathComponent("bundles", isDirectory: true)
        return places
    }()

    /// The linked core, or a skip.
    func linked() throws -> any Core {
        guard let core else {
            throw XCTSkip("The core is not linked. Set OPENWORLD_LIB_DIR to run these.")
        }
        return core
    }

    func ask<Value: Decodable>(_ request: Request, _ type: Value.Type) -> Answer<Value> {
        Answers.decode(core?.call(request.json) ?? "", as: type)
    }

    func testUpdatePostersRefusesUntilACurveAllowsRealPosters() throws {
        let core = try linked()
        let request = Request.postersUpdate(bundles: places.bundles.path, bundle: "fast", posters: places.posters.path).json
        guard case .refused(let code, _) = Answers.decode(core.call(request) { _ in }, as: PostersUpdateAnswer.self) else {
            return XCTFail("Update posters ran without a curve")
        }
        XCTAssertEqual(code, "real_posters_off")
    }

    func testCallsAnswerWithTheContract() throws {
        _ = try linked()
        let version = try XCTUnwrap(ask(.version, VersionAnswer.self).value)
        XCTAssertEqual(version.api, 1)
        XCTAssertTrue(version.demoPosters, "the shells' test builds read demo posters")
        let copy = try XCTUnwrap(ask(.copy, CopyAnswer.self).value)
        XCTAssertEqual(copy.copy["no_clearance"], "No candidate is not a clearance.")
        XCTAssertFalse(copy.disclosure?.isEmpty ?? true)
        XCTAssertFalse(copy.disclosure?.contains { $0.code == "no_clearance" } ?? true)
        let rows = try XCTUnwrap(ask(.bundles(catalog: places.bundles.path), BundlesAnswer.self).value)
        XCTAssertEqual(rows.bundles.first?.id, "fast")
        XCTAssertEqual(ask(.postersCheck(posters: places.posters.path), PostersAnswer.self).value, nil)
        XCTAssertNotNil(ask(.postersDemo(out: places.posters.path), PostersAnswer.self).value)
        XCTAssertEqual(ask(.postersCheck(posters: places.posters.path), PostersAnswer.self).value?.posters.demo, true)
        XCTAssertEqual(ask(.leave(url: "https://www.fbi.gov/wanted/topten"), LeaveAnswer.self).value?.message.code, "leaving")
        guard case .refused(let code, _) = ask(.leave(url: "https://www.fbi.gov.evil.example/"), LeaveAnswer.self) else {
            return XCTFail("only an FBI page opens")
        }
        XCTAssertEqual(code, "only_fbi")
        XCTAssertNotNil(ask(.sweep(root: places.results.path), SweepAnswer.self).value)
    }

    func testLegalNamesTheVersionAndReadsTheTerms() throws {
        _ = try linked()
        let legal = try XCTUnwrap(ask(.legal, LegalAnswer.self).value)
        XCTAssertFalse(legal.version.isEmpty)
        XCTAssertEqual(legal.firstRun.first?.code, "welcome")
        XCTAssertEqual(legal.accept.code, "accept_terms")
        XCTAssertEqual(legal.changed.code, "terms_changed")
        XCTAssertEqual(legal.documents.map(\.id), ["terms", "privacy", "biometric", "health_data", "licenses"])
        let terms = try XCTUnwrap(ask(.legalDocument(id: "terms"), LegalDocument.self).value)
        XCTAssertEqual(terms.id, "terms")
        XCTAssertEqual(terms.title.code, "doc_terms")
        XCTAssertGreaterThan(terms.paragraphs.count, 3)
        XCTAssertTrue(terms.paragraphs.contains { $0.contains(legal.contact) })
        guard case .refused(let code, _) = ask(.legalDocument(id: "nope"), LegalDocument.self) else {
            return XCTFail("an unknown document reads")
        }
        XCTAssertEqual(code, "no_document")
    }

    func testTheDemoSceneIsACandidateThroughTheModel() async throws {
        let model = ScanModel(core: try linked(), media: SceneMedia(), places: places, defaults: temporaryDefaults()) { .computer }
        await model.bootstrap()
        XCTAssertEqual(model.terms, .firstRun(changed: false))
        await model.acceptTerms()
        await model.installDemoPosters()
        let source = places.imports.deletingLastPathComponent().appendingPathComponent("scene.png")
        try Data("a picture".utf8).write(to: source)
        await model.choose(try Importer.copy(source, into: places.imports, origin: .files))
        guard case .review(let review) = model.state, case .ready(let estimate) = review.estimate else {
            return XCTFail("estimate: \(model.state)")
        }
        XCTAssertTrue(estimate.canStart)
        XCTAssertEqual(estimate.framesAnalyzed, 1)
        // Review shows the core's disclosure, and every line has its sentence.
        XCTAssertEqual(review.disclosure, estimate.disclosure)
        XCTAssertTrue(estimate.disclosure.contains { $0.code == "no_clearance" })
        XCTAssertTrue(estimate.disclosure.allSatisfy { model.copy.texts[$0.code] == $0.text })
        await model.analyze()?.value
        guard case .result(let result) = model.state else { return XCTFail("result: \(model.state)") }
        XCTAssertEqual(result.report.status, .complete)
        XCTAssertEqual(result.report.headline.code, "possible_candidate")
        let card = try XCTUnwrap(result.report.candidates.first)
        XCTAssertEqual(card.poster.id, "fixture-missing-a")
        let crop = try XCTUnwrap(result.url(card.crop))
        XCTAssertTrue(FileManager.default.fileExists(atPath: crop.path))
        let prompt = await model.leave(card)
        XCTAssertEqual(prompt?.url.host, "www.fbi.gov")
        await model.deleteResult()
        XCTAssertEqual(model.state, .start)
        XCTAssertFalse(FileManager.default.fileExists(atPath: result.folder.path))
    }

    func testClosingTheWindowMidScanLeavesNothingOnDisk() async throws {
        let gate = DispatchSemaphore(value: 0)
        let model = ScanModel(core: try linked(), media: SceneMedia(frames: 3, gate: gate), places: places, defaults: temporaryDefaults()) { .computer }
        await model.bootstrap()
        await model.acceptTerms()
        await model.installDemoPosters()
        let source = places.imports.deletingLastPathComponent().appendingPathComponent("scene.mov")
        try Data("a video".utf8).write(to: source)
        await model.choose(try Importer.copy(source, into: places.imports, origin: .files))
        let task = try XCTUnwrap(model.analyze())
        let started = await eventually {
            guard case .scanning(let progress) = model.state else { return false }
            return !progress.faces.isEmpty
        }
        XCTAssertTrue(started)
        model.close()
        gate.signal()
        await task.value
        XCTAssertEqual(model.state, .start)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: places.results.path), [], "no partial result is left")
    }

    func testAStopIsIncompleteAndAShortDecodeIsIncomplete() async throws {
        let core = try linked()
        XCTAssertNotNil(ask(.postersDemo(out: places.posters.path), PostersAnswer.self).value)
        let facts = MediaFacts(width: 640, height: 480, fps: 30, frames: 3, durationSec: 0.1, video: true)
        for (stop, code) in [(StopReason.heat, "stopped_heat"), (.background, "stopped_background")] {
            let runner = ScanRunner()
            let gate = DispatchSemaphore(value: 0)
            let updates = runner.run(core: core, request: jobRequest(facts), plan: ScenePlan(frames: 3, gate: gate))
            var faces = 0
            var report: Report?
            var stopped = false
            for await update in updates {
                switch update {
                case .event(.face):
                    faces += 1
                    if !stopped {
                        stopped = true
                        runner.stop(stop)
                        gate.signal()
                    }
                case .finished(let finished): report = finished
                default: break
                }
            }
            XCTAssertGreaterThan(faces, 0)
            XCTAssertEqual(report?.status, .incomplete)
            XCTAssertEqual(report?.reason?.code, code)
        }

        // Fewer frames than declared is Incomplete, never a clearance.
        let short = ScanRunner()
        var last: Report?
        for await update in short.run(core: core, request: jobRequest(facts), plan: ScenePlan(frames: 1, gate: nil)) {
            if case .finished(let report) = update { last = report }
        }
        XCTAssertEqual(last?.status, .incomplete)
        XCTAssertEqual(last?.reason?.code, "not_fully_decoded")
        XCTAssertFalse(last?.disclosure.contains { $0.code == "no_clearance" } ?? true)
    }

    func testYUVPushesAndRefusals() async throws {
        let core = try linked()
        XCTAssertNotNil(ask(.postersDemo(out: places.posters.path), PostersAnswer.self).value)
        let facts = MediaFacts(width: 64, height: 48)
        var report: Report?
        for await update in ScanRunner().run(core: core, request: jobRequest(facts), plan: GreyNV12Plan()) {
            if case .finished(let finished) = update { report = finished }
        }
        XCTAssertEqual(report?.status, .complete)
        XCTAssertEqual(report?.headline.code, "no_clearance")
        XCTAssertEqual(report?.counts.framesAnalyzed, 1)

        // A pack that is not there refuses at the start.
        var refused: Report?
        var request = scanRequest(facts)
        request.posters = places.posters.appendingPathComponent("missing").path
        for await update in ScanRunner().run(core: core, request: Request.job(request).json, plan: GreyNV12Plan()) {
            if case .finished(let finished) = update { refused = finished }
        }
        XCTAssertEqual(refused?.status, .refused)
        XCTAssertEqual(refused?.refusal, "missing_pack")
    }

    private func scanRequest(_ facts: MediaFacts) -> ScanRequest {
        ScanRequest(
            input: "", bundles: places.bundles.path, bundle: "fast", posters: places.posters.path,
            out: places.newResultFolder().path, coverage: .complete, missing: true, wanted: true,
            device: .computer, media: facts, fileMtimeUnix: nil
        )
    }

    private func jobRequest(_ facts: MediaFacts) -> String {
        Request.job(scanRequest(facts)).json
    }
}

/// The core's demo still: marker 7 (a demo poster) 128 px across at (12, 12) on white.
/// Synthetic, never a face. Mirrors `fixture::render` and `fixture::demo_scene`.
enum DemoScene {
    static let width = 640
    static let height = 480

    static func rgb() -> [UInt8] {
        var pixels = [UInt8](repeating: 255, count: width * height * 3)
        paint(id: 7, module: 16, x: 12, y: 12, into: &pixels)
        return pixels
    }

    static func paint(id: UInt16, module: Int, x: Int, y: Int, into pixels: inout [UInt8]) {
        let cells: [(Int, Int)] = [(0, 1), (0, 2), (0, 3), (1, 0), (1, 1), (1, 2), (1, 3), (2, 0), (2, 1), (2, 2), (2, 3), (3, 0), (3, 1), (3, 2)]
        let masked = id & 0x03FF
        let packed = (masked << 4) | ((masked ^ (masked >> 4) ^ (masked >> 8)) & 0x0F)
        var bits = [Bool](repeating: false, count: 16)
        bits[0] = true
        for (i, (r, c)) in cells.enumerated() {
            bits[r * 4 + c] = (packed >> (13 - i)) & 1 == 1
        }
        for r in 0..<8 {
            for c in 0..<8 {
                let border = r == 0 || r == 7 || c == 0 || c == 7
                let ring = !border && (r == 1 || r == 6 || c == 1 || c == 6)
                guard border || (!ring && bits[(r - 2) * 4 + (c - 2)]) else { continue }
                for yy in 0..<module {
                    for xx in 0..<module {
                        let offset = ((y + r * module + yy) * width + (x + c * module + xx)) * 3
                        pixels[offset] = 0
                        pixels[offset + 1] = 0
                        pixels[offset + 2] = 0
                    }
                }
            }
        }
    }
}

struct ScenePlan: DecodePlan {
    var frames: UInt64
    var gate: DispatchSemaphore?

    func decode(into sink: FrameSink) -> Bool {
        let pixels = DemoScene.rgb()
        for index in 0..<frames {
            guard sink.wants(index) else {
                if !sink.skip(index) { return false }
                continue
            }
            let kept = pixels.withUnsafeBytes { raw in
                sink.push(index, pixels: PixelFrame(base: raw.baseAddress!, width: DemoScene.width, height: DemoScene.height, stride: DemoScene.width * 3, layout: .rgb, rotation: .up))
            }
            if index == 0 { gate?.wait() }
            if !kept { return false }
        }
        return true
    }
}

/// The demo still, or that many copies of it as a 30 fps video.
struct SceneMedia: MediaReader {
    var frames: UInt64 = 1
    var gate: DispatchSemaphore?

    func probe(_ file: ImportedFile) async throws -> ProbedMedia {
        let video = frames > 1
        let facts = video
            ? MediaFacts(width: DemoScene.width, height: DemoScene.height, fps: 30, frames: frames, durationSec: Double(frames) / 30, video: true)
            : MediaFacts(width: DemoScene.width, height: DemoScene.height)
        return ProbedMedia(facts: facts, kind: video ? .video : .photo, recorded: nil, plan: ScenePlan(frames: frames, gate: gate))
    }
}

/// One 64x48 mid-grey NV12 picture, pushed as Y and interleaved CbCr planes.
struct GreyNV12Plan: DecodePlan {
    func decode(into sink: FrameSink) -> Bool {
        let y = [UInt8](repeating: 128, count: 64 * 48)
        let cbcr = [UInt8](repeating: 128, count: 64 * 24)
        guard sink.wants(0) else { return sink.skip(0) }
        return y.withUnsafeBytes { yRaw in
            cbcr.withUnsafeBytes { uvRaw in
                let base = uvRaw.baseAddress!
                return sink.push(0, yuv: YUVFrame(
                    y: yRaw.baseAddress!, yStride: 64, u: base, v: base + 1, uvStride: 64, uvStep: 2,
                    width: 64, height: 48, fullRange: true, bt709: true, rotation: .up
                ))
            }
        }
    }
}
