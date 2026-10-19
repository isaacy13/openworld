// SPDX-License-Identifier: Apache-2.0
@testable import OpenWorldKit
import XCTest

@MainActor
final class ScanModelTests: XCTestCase {
    // XCTest makes a new instance for every test.
    let core = FakeCore()
    let places = try! temporaryPlaces()
    var deviceFacts = DeviceFacts(kind: "phone", thermal: "nominal", battery: 0.8, charging: false)

    func model(media: FakeMedia = FakeMedia()) -> ScanModel {
        ScanModel(core: core, media: media, places: places) { [unowned self] in self.deviceFacts }
    }

    func imported(_ name: String = "clip.mov", origin: ImportedFile.Origin = .files, modified: Date? = Date()) throws -> ImportedFile {
        let source = places.imports.deletingLastPathComponent().appendingPathComponent(name)
        try Data("not really a movie".utf8).write(to: source)
        return try Importer.copy(source, into: places.imports, origin: origin)
    }

    func testBootstrapReadsCopyBundlesAndPosters() async {
        let model = model()
        await model.bootstrap(interruptedScans: 1)
        XCTAssertEqual(model.state, .start)
        XCTAssertEqual(model.bundles.map(\.id), ["fast", "accurate"])
        XCTAssertEqual(model.options.bundle, "fast")
        XCTAssertTrue(model.demoBuild)
        XCTAssertEqual(model.interruptedScans, 1)
        guard case .ready(let pack) = model.posters else { return XCTFail("posters") }
        XCTAssertTrue(pack.demo)
        XCTAssertEqual(model.copy.line("incomplete").text, "Incomplete.")
    }

    func testMissingPostersAreTheCoresRefusalAndBlockAnalyze() async throws {
        core.answers["posters_check"] = Samples.postersMissing
        let model = model()
        await model.bootstrap()
        XCTAssertEqual(model.posters, .refused(Line(code: "missing_pack", text: "The poster pack is missing. Refusing.")))
        await model.choose(try imported())
        XCTAssertFalse(model.canAnalyze)
        XCTAssertNil(model.analyze())
        await model.installDemoPosters()
        guard case .ready = model.posters else { return XCTFail("demo posters") }
        await model.estimateTask?.value
        XCTAssertTrue(model.canAnalyze)
    }

    func testChooseShowsReviewWithEstimateAndSendsAVersionedRequest() async throws {
        let model = model()
        await model.bootstrap()
        let file = try imported()
        await model.choose(file)
        XCTAssertEqual(model.path, [.review])
        guard case .review(let review) = model.state, case .ready(let estimate) = review.estimate else {
            return XCTFail("review with an estimate")
        }
        XCTAssertEqual(review.file, file)
        XCTAssertEqual(review.media?.facts.frames, 3)
        XCTAssertEqual(estimate.framesAnalyzed, 300)
        XCTAssertTrue(model.canAnalyze)
        let sent = try XCTUnwrap(core.requests.last { $0.contains(#""op":"estimate""#) })
        let body = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(sent.utf8)) as? [String: Any])
        XCTAssertEqual(body["v"] as? Int, 1)
        XCTAssertEqual(body["bundle"] as? String, "fast")
        XCTAssertEqual(body["coverage"] as? String, "complete")
        XCTAssertEqual((body["device"] as? [String: Any])?["kind"] as? String, "phone")
        XCTAssertEqual((body["media"] as? [String: Any])?["frames"] as? Int, 3)
        XCTAssertNotNil(body["file_mtime_unix"])
    }

    func testOptionsEstimateAgainAndBothClassesOffCannotStart() async throws {
        let model = model()
        await model.bootstrap()
        await model.choose(try imported())
        core.answers["estimate"] = Samples.estimateNoClass
        model.update { $0.missing = false; $0.wanted = false }
        await model.estimateTask?.value
        guard case .review(let review) = model.state, case .ready(let estimate) = review.estimate else {
            return XCTFail("review")
        }
        XCTAssertFalse(estimate.canStart)
        XCTAssertEqual(estimate.classes.code, "choose_class")
        XCTAssertFalse(model.canAnalyze)
        let sent = try XCTUnwrap(core.requests.last)
        XCTAssertTrue(sent.contains(#""missing":false"#) && sent.contains(#""wanted":false"#))
    }

    func testAnUnreadableFileIsTheCoresRefusalSentence() async throws {
        let model = model(media: FakeMedia(failure: .badCodec))
        await model.bootstrap()
        await model.choose(try imported())
        guard case .review(let review) = model.state else { return XCTFail("review") }
        XCTAssertEqual(review.estimate, .refused(Line(code: "bad_codec", text: "Bad codec or unreadable file. Refusing.")))
        XCTAssertFalse(model.canAnalyze)
    }

    func testAScanShowsFacesThenTheResult() async throws {
        core.declined = [1]
        let model = model()
        await model.bootstrap()
        await model.choose(try imported())
        let task = try XCTUnwrap(model.analyze())
        XCTAssertTrue(model.isScanning)
        XCTAssertEqual(model.path, [.review, .scan])
        await task.value
        guard case .result(let result) = model.state else { return XCTFail("result, got \(model.state)") }
        XCTAssertEqual(result.report.headline.code, "possible_candidate")
        XCTAssertEqual(core.pushed, [0, 2])
        XCTAssertEqual(core.skipped, [1])
        // The job request has the version and no op, and names an empty result folder.
        let job = try XCTUnwrap(core.jobRequests.first)
        let body = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(job.utf8)) as? [String: Any])
        XCTAssertEqual(body["v"] as? Int, 1)
        XCTAssertNil(body["op"])
        XCTAssertEqual(body["out"] as? String, result.folder.path)
        XCTAssertTrue(result.folder.path.hasPrefix(places.results.path))
        XCTAssertEqual(result.url("crops/face_1.jpg")?.lastPathComponent, "face_1.jpg")
        // The journal marker is gone once the report is in.
        XCTAssertTrue(ScanJournal(folder: places.journal).interrupted().isEmpty)
    }

    func testFacesUpdateFromTrackEvents() async throws {
        let gate = DispatchSemaphore(value: 0)
        let model = model(media: FakeMedia(gate: gate))
        await model.bootstrap()
        await model.choose(try imported())
        let task = try XCTUnwrap(model.analyze())
        let seen = await eventually {
            guard case .scanning(let progress) = model.state else { return false }
            return progress.faces.count == 2 && progress.faces[1].track != nil
        }
        XCTAssertTrue(seen)
        if case .scanning(let progress) = model.state {
            XCTAssertNil(progress.faces[0].track, "a first look until its track is decided")
            XCTAssertEqual(progress.faces[1].track?.status, .notCompared)
            XCTAssertEqual(progress.faces[1].crop?.lastPathComponent, "face_2.jpg")
        }
        gate.signal()
        await task.value
    }

    func testStopIsIncompleteWithItsReason() async throws {
        let gate = DispatchSemaphore(value: 0)
        let model = model(media: FakeMedia(gate: gate))
        await model.bootstrap()
        await model.choose(try imported())
        let task = try XCTUnwrap(model.analyze())
        _ = await eventually { self.core.pushed.count == 1 }
        model.stop(.cancel)
        if case .scanning(let progress) = model.state {
            XCTAssertEqual(progress.stopping, .cancel)
        }
        gate.signal()
        await task.value
        guard case .result(let result) = model.state else { return XCTFail("result") }
        XCTAssertEqual(result.report.status, .incomplete)
        XCTAssertEqual(result.report.reason?.code, "cancelled")
        XCTAssertEqual(core.stopReason, .cancel)
        XCTAssertEqual(core.pushed, [0, 1], "the core refuses the next frame and the decoder ends")
    }

    func testHeatAndBatteryStopTheScan() async throws {
        let gate = DispatchSemaphore(value: 0)
        let model = model(media: FakeMedia(gate: gate))
        await model.bootstrap()
        await model.choose(try imported())
        let task = try XCTUnwrap(model.analyze())
        model.deviceChanged()
        XCTAssertNil(core.stopReason, "a cool, charged device keeps scanning")
        deviceFacts.thermal = "serious"
        model.deviceChanged()
        gate.signal()
        await task.value
        guard case .result(let result) = model.state else { return XCTFail("result") }
        XCTAssertEqual(result.report.reason?.code, "stopped_heat")
    }

    func testARefusedStartIsTheRefusalScreen() async throws {
        core.jobRefusal = Samples.refusalStart
        let model = model()
        await model.bootstrap()
        await model.choose(try imported())
        await model.analyze()?.value
        XCTAssertEqual(model.state, .refused(Line(code: "missing_pack", text: "The poster pack is missing. Refusing.")))
        XCTAssertEqual(model.path, [.review, .scan])
        // Back goes to the same file's review.
        model.navigate(to: [.review])
        guard case .review = model.state else { return XCTFail("review") }
    }

    func testBackDuringAScanIsIgnored() async throws {
        let gate = DispatchSemaphore(value: 0)
        let model = model(media: FakeMedia(gate: gate))
        await model.bootstrap()
        await model.choose(try imported())
        let task = try XCTUnwrap(model.analyze())
        model.navigate(to: [])
        XCTAssertTrue(model.isScanning)
        model.startOver()
        XCTAssertTrue(model.isScanning)
        gate.signal()
        await task.value
        model.navigate(to: [])
        XCTAssertEqual(model.state, .start)
    }

    func testLeaveOpensOnlyTheURLTheCoreApproved() async throws {
        let model = model()
        let report = Answers.report(Samples.reportCandidate)
        var card = try XCTUnwrap(report.candidates.first)
        let prompt = await model.leave(card)
        XCTAssertEqual(prompt?.url.absoluteString, "https://www.fbi.gov/wanted")
        XCTAssertEqual(prompt?.title.code, "leaving")
        XCTAssertEqual(prompt?.notes.first?.code, "not_sending")

        core.answers["leave"] = Samples.leaveBad
        card.poster.fbiUrl = "https://evil.example"
        let refused = await model.leave(card)
        XCTAssertNil(refused)
        XCTAssertEqual(model.alert?.code, "only_fbi")

        card.poster.fbiUrl = nil
        let none = await model.leave(card)
        XCTAssertNil(none)
    }

    func testDeleteGoesBackToStartAndRemovesTheImport() async throws {
        let model = model()
        await model.bootstrap()
        let file = try imported()
        await model.choose(file)
        await model.analyze()?.value
        await model.deleteResult()
        XCTAssertEqual(model.state, .start)
        XCTAssertTrue(core.requests.contains { $0.contains(#""op":"delete""#) })
        XCTAssertFalse(FileManager.default.fileExists(atPath: file.url.path))

        core.answers["delete"] = Samples.deleteForeign
        await model.choose(try imported())
        await model.analyze()?.value
        await model.deleteResult()
        XCTAssertEqual(model.alert?.code, "delete_foreign")
        guard case .result = model.state else { return XCTFail("the result stays when delete is refused") }
    }

    func testAPhotosPickUsesItsRecordingTimeForAge() async throws {
        let recorded = Date(timeIntervalSince1970: 1_600_000_000)
        let model = model(media: FakeMedia(recorded: recorded))
        await model.bootstrap()
        let source = places.imports.deletingLastPathComponent().appendingPathComponent("IMG_0001.HEIC")
        try Data("pixels".utf8).write(to: source)
        let file = try Importer.copy(source, into: places.imports, origin: .photos, keepModified: false)
        XCTAssertNil(file.modified)
        await model.choose(file)
        guard case .review(let review) = model.state else { return XCTFail("review") }
        XCTAssertEqual(review.warnings.map(\.code), ["old_file"])
        let sent = try XCTUnwrap(core.requests.last { $0.contains(#""op":"estimate""#) })
        XCTAssertTrue(sent.contains(#""file_mtime_unix":1600000000"#))
    }
}
