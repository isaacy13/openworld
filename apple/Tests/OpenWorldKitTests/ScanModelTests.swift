// SPDX-License-Identifier: Apache-2.0
@testable import OpenWorldKit
import XCTest

@MainActor
final class ScanModelTests: XCTestCase {
    // XCTest makes a new instance for every test.
    let core = FakeCore()
    let places = try! temporaryPlaces()
    /// The current terms are accepted, so Start opens. The first-run tests make their own.
    let defaults = temporaryDefaults(accepted: "2026-10-19")
    var deviceFacts = DeviceFacts(kind: "phone", thermal: "nominal", battery: 0.8, charging: false)

    func model(media: FakeMedia = FakeMedia(), defaults: UserDefaults? = nil) -> ScanModel {
        ScanModel(core: core, media: media, places: places, defaults: defaults ?? self.defaults) { [unowned self] in self.deviceFacts }
    }

    func imported(_ name: String = "clip.mov", origin: ImportedFile.Origin = .files, modified: Date? = Date()) throws -> ImportedFile {
        let source = places.imports.deletingLastPathComponent().appendingPathComponent(name)
        try Data("not really a movie".utf8).write(to: source)
        return try Importer.copy(source, into: places.imports, origin: origin)
    }

    func testBootstrapReadsCopyBundlesAndPosters() async {
        let model = model()
        XCTAssertNil(model.notAffiliated)
        await model.bootstrap(interruptedScans: 1)
        XCTAssertEqual(model.state, .start)
        XCTAssertEqual(model.bundles.map(\.id), ["fast", "accurate"])
        XCTAssertEqual(model.options.bundle, "fast")
        XCTAssertTrue(model.demoBuild)
        XCTAssertEqual(model.interruptedScans, 1)
        guard case .ready(let pack) = model.posters else { return XCTFail("posters") }
        XCTAssertTrue(pack.demo)
        XCTAssertEqual(model.copy.line("incomplete").text, "Incomplete.")
        // The interrupted-scan card shows the core's disclosure, without the clearance line.
        XCTAssertEqual(model.copy.disclosure.count, 13)
        XCTAssertFalse(model.copy.disclosure.contains { $0.code == "no_clearance" })
        // The start screen's non-affiliation line, from the core by code.
        XCTAssertEqual(model.notAffiliated, Line(code: "not_affiliated", text: "OpenWorld is not part of, or endorsed by, the FBI or any government agency."))
    }

    // MARK: First run

    func testNothingAcceptedShowsFirstRunAndHoldsAFile() async throws {
        let model = model(defaults: temporaryDefaults())
        XCTAssertEqual(model.terms, .loading)
        await model.bootstrap()
        XCTAssertEqual(model.terms, .firstRun(changed: false))
        let legal = try XCTUnwrap(model.legal)
        XCTAssertEqual(legal.version, "2026-10-19")
        XCTAssertEqual(legal.contact, "support@onexengineering.com")
        XCTAssertEqual(legal.firstRun.first?.code, "welcome")
        XCTAssertEqual(legal.accept.code, "accept_terms")
        XCTAssertEqual(legal.documents.map(\.id), ["terms", "privacy", "biometric", "health_data", "licenses"])
        XCTAssertEqual(legal.documents.first?.title, Line(code: "doc_terms", text: "Terms of Use"))
        // A file that arrives before Continue waits: it is not read or estimated.
        await model.choose(try imported())
        XCTAssertEqual(model.state, .start)
        XCTAssertFalse(core.requests.contains { $0.contains(#""op":"estimate""#) })
        XCTAssertFalse(model.canAnalyze)
        XCTAssertNil(model.analyze())
    }

    func testTheAcceptedVersionOpensStart() async {
        let model = model()
        await model.bootstrap()
        XCTAssertEqual(model.terms, .accepted)
        XCTAssertTrue(model.termsAccepted)
        XCTAssertEqual(model.state, .start)
        XCTAssertTrue(core.requests.contains(Request.legal.json))
    }

    func testAnOlderVersionShowsFirstRunWithTheChangedLine() async {
        let model = model(defaults: temporaryDefaults(accepted: "2025-01-01"))
        await model.bootstrap()
        XCTAssertEqual(model.terms, .firstRun(changed: true))
        XCTAssertEqual(model.legal?.changed.code, "terms_changed")
        XCTAssertFalse(model.termsAccepted)
    }

    func testContinueStoresTheVersionAndOpensTheWaitingFile() async throws {
        let defaults = temporaryDefaults(accepted: "2025-01-01")
        let model = model(defaults: defaults)
        await model.bootstrap()
        let file = try imported()
        await model.choose(file)
        XCTAssertEqual(model.state, .start)
        await model.acceptTerms()
        XCTAssertEqual(defaults.string(forKey: ScanModel.acceptedTermsKey), "2026-10-19")
        XCTAssertEqual(model.terms, .accepted)
        guard case .review(let review) = model.state else { return XCTFail("review, got \(model.state)") }
        XCTAssertEqual(review.file, file)
        XCTAssertTrue(model.canAnalyze)
        // The next launch opens Start.
        let next = self.model(defaults: defaults)
        await next.bootstrap()
        XCTAssertEqual(next.terms, .accepted)
    }

    func testContinueInAnotherWindowCountsHere() async {
        let defaults = temporaryDefaults()
        let first = model(defaults: defaults)
        let second = model(defaults: defaults)
        await first.bootstrap()
        await second.bootstrap()
        await first.acceptTerms()
        XCTAssertEqual(second.terms, .firstRun(changed: false))
        await second.refreshTerms()
        XCTAssertEqual(second.terms, .accepted)
    }

    func testARefusedLegalIsShownAndNothingIsAccepted() async throws {
        core.answers["legal"] = nil
        let defaults = temporaryDefaults()
        let model = model(defaults: defaults)
        await model.bootstrap()
        XCTAssertEqual(model.terms, .refused(.internalFailure))
        XCTAssertNil(model.legal)
        await model.acceptTerms()
        XCTAssertEqual(model.terms, .refused(.internalFailure))
        XCTAssertNil(defaults.string(forKey: ScanModel.acceptedTermsKey))
        await model.choose(try imported())
        XCTAssertEqual(model.state, .start)
    }

    func testADocumentIsReadOrItsRefusalHeadline() async {
        let model = model()
        guard case .ok(let terms) = await model.document("terms") else { return XCTFail("terms") }
        XCTAssertEqual(terms.title.code, "doc_terms")
        XCTAssertEqual(terms.paragraphs.first, "OpenWorld Terms of Use")
        XCTAssertEqual(core.requests.last, #"{"id":"terms","op":"legal_document","v":1}"#)
        core.answers["legal_document"] = Samples.noDocument
        let missing = await model.document("nope")
        XCTAssertEqual(missing, .refused(code: "no_document", headline: Line(code: "no_document", text: "That document is not in this build. Refusing.")))
    }

    func testUpdatePostersIsTheCoresRefusalAndKeepsThePack() async {
        let model = model()
        await model.bootstrap()
        await model.updatePosters()
        XCTAssertEqual(model.update, .refused(Line(code: "real_posters_off", text: "Real FBI photos stay off. No bundle has a curve that allows them.")))
        XCTAssertEqual(
            core.requests.last { $0.contains("posters_update") },
            Request.postersUpdate(bundles: places.bundles.path, bundle: "fast", posters: places.posters.path).json
        )
        // The demo pack stays, and the refusal is not an alert.
        guard case .ready(let pack) = model.posters else { return XCTFail("posters") }
        XCTAssertTrue(pack.demo)
        XCTAssertNil(model.alert)
        // Installing the demo pack clears the old answer.
        await model.installDemoPosters()
        XCTAssertEqual(model.update, .idle)
    }

    func testAnUpdateSaysWhatItKeptAndLeftOutThenReadsThePack() async {
        core.answers["posters_update"] = Samples.postersUpdated
        core.updateEvents = [Samples.postersPageEvent, Samples.postersCheckedEvent]
        let model = model()
        await model.bootstrap()
        let checks = core.requests.filter { $0.contains("posters_check") }.count
        await model.updatePosters()
        guard case .done(let kept, let skipped) = model.update else { return XCTFail("done") }
        XCTAssertEqual(kept, 880)
        XCTAssertEqual(skipped.map(\.reason.code), ["skip_no_face", "skip_several_faces"])
        XCTAssertEqual(skipped.map(\.count), [20, 4])
        XCTAssertEqual(core.requests.filter { $0.contains("posters_check") }.count, checks + 1)
        // Progress that arrives after the answer is dropped.
        for _ in 0..<10 { await Task.yield() }
        guard case .done = model.update else { return XCTFail("still done") }
    }

    func testPostersEventsMoveTheProgress() throws {
        var progress = UpdateProgress()
        progress.apply(try XCTUnwrap(PostersEvent.parse(Samples.postersPageEvent)))
        XCTAssertEqual(progress.page, 3)
        XCTAssertEqual(progress.pages, 26)
        progress.apply(try XCTUnwrap(PostersEvent.parse(Samples.postersPageOpenEvent)))
        XCTAssertEqual(progress.page, 1)
        XCTAssertNil(progress.pages)
        progress.apply(try XCTUnwrap(PostersEvent.parse(Samples.postersCheckedEvent)))
        XCTAssertEqual(progress.checked, 120)
        XCTAssertEqual(progress.posters, 900)
        XCTAssertEqual(PostersEvent.parse(Samples.progressEvent), .other)
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

    func testAScanStartedHotOrOnALowBatteryStopsAtOnce() async throws {
        let cases = [
            (DeviceFacts(kind: "phone", thermal: "serious", battery: 0.8, charging: false), "stopped_heat"),
            (DeviceFacts(kind: "phone", thermal: "nominal", battery: 0.05, charging: false), "stopped_battery"),
        ]
        for (facts, code) in cases {
            // No change notice comes: the device was already like this.
            let core = FakeCore()
            let gate = DispatchSemaphore(value: 0)
            let model = ScanModel(core: core, media: FakeMedia(gate: gate), places: places, defaults: defaults) { facts }
            await model.bootstrap()
            await model.choose(try imported())
            let task = try XCTUnwrap(model.analyze())
            gate.signal()
            await task.value
            guard case .result(let result) = model.state else { return XCTFail("result") }
            XCTAssertEqual(result.report.status, .incomplete)
            XCTAssertEqual(result.report.reason?.code, code)
        }
    }

    func testClosingTheWindowMidScanDeletesTheResultAndShowsNothing() async throws {
        let gate = DispatchSemaphore(value: 0)
        let model = model(media: FakeMedia(gate: gate))
        await model.bootstrap()
        let file = try imported()
        await model.choose(file)
        let task = try XCTUnwrap(model.analyze())
        _ = await eventually { self.core.pushed.count == 1 }
        model.close()
        XCTAssertTrue(model.isScanning, "the scan ends on its own queue")
        gate.signal()
        await task.value
        XCTAssertEqual(core.stopReason, .cancel)
        XCTAssertEqual(model.state, .start, "a closed window's scan is never shown")
        XCTAssertNil(model.review)
        XCTAssertFalse(FileManager.default.fileExists(atPath: file.url.path))
        let job = try XCTUnwrap(core.jobRequests.first)
        let out = try XCTUnwrap((JSONSerialization.jsonObject(with: Data(job.utf8)) as? [String: Any])?["out"] as? String)
        XCTAssertTrue(core.requests.contains { $0.contains(#""op":"delete""#) && $0.contains(out) }, "the result folder is deleted")
        // Deleted before the journal marker ended.
        XCTAssertTrue(ScanJournal(folder: places.journal).interrupted().isEmpty)
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
        core.answers["estimate"] = Samples.estimateOldFile
        let model = model(media: FakeMedia(recorded: recorded))
        await model.bootstrap()
        let source = places.imports.deletingLastPathComponent().appendingPathComponent("IMG_0001.HEIC")
        try Data("pixels".utf8).write(to: source)
        let file = try Importer.copy(source, into: places.imports, origin: .photos, keepModified: false)
        XCTAssertNil(file.modified)
        await model.choose(file)
        guard case .review(let review) = model.state else { return XCTFail("review") }
        // The warnings are the core's, from the estimate.
        XCTAssertEqual(review.warnings.map(\.code), ["old_file"])
        XCTAssertEqual(review.disclosure.count, 9)
        let sent = try XCTUnwrap(core.requests.last { $0.contains(#""op":"estimate""#) })
        XCTAssertTrue(sent.contains(#""file_mtime_unix":1600000000"#))
    }
}
