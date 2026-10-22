// SPDX-License-Identifier: Apache-2.0
@testable import OpenWorldKit
import XCTest

/// The kit reads the core's real answers, and reads codes, never English.
final class DecodingTests: XCTestCase {
    func testCandidateReport() throws {
        let report = Answers.report(Samples.reportCandidate)
        XCTAssertEqual(report.schema, "openworld.result.v2")
        XCTAssertEqual(report.status, .complete)
        XCTAssertEqual(report.headline.code, "possible_candidate")
        XCTAssertEqual(report.counts.facesSeen, 2)
        XCTAssertEqual(report.counts.framesDeclared, 1)
        let card = try XCTUnwrap(report.candidates.first)
        XCTAssertEqual(card.poster.posterClass, "missing")
        XCTAssertFalse(card.poster.isWanted)
        XCTAssertEqual(card.poster.fbiUrl, "https://www.fbi.gov/wanted")
        XCTAssertEqual(card.poster.image, "/tmp/openworld/posters/images/fixture-missing-a.png")
        XCTAssertEqual(card.band.code, "band_well")
        XCTAssertEqual(card.frameLabel.int("frame"), 1)
        XCTAssertEqual(card.crop, "crops/face_1.jpg")
        XCTAssertEqual(card.frame, "frames/frame_0.jpg")
        XCTAssertEqual(card.leaving.code, "leaving")
        XCTAssertEqual(card.leaveNotes.map(\.code), ["not_sending", "leave_missing", "unauthenticated"])
        XCTAssertEqual(report.tracks.map(\.status), [.candidate, .notCompared])
        XCTAssertEqual(report.tracks[1].reason?.code, "why_small")
        XCTAssertEqual(report.otherFaces.map(\.trackId), [2])
        XCTAssertEqual(report.facesLine?.int("seen"), 2)
        XCTAssertEqual(report.perceptionNote?.code, "fixture_markers")
        XCTAssertEqual(report.posters?.demo, true)
        XCTAssertEqual(report.posters?.count, 2)
        XCTAssertEqual(report.bundle?.name, "Fast")
        XCTAssertTrue(report.disclosure.contains { $0.code == "no_clearance" })
    }

    func testEmptyAndIncompleteReports() {
        let empty = Answers.report(Samples.reportEmpty)
        XCTAssertEqual(empty.status, .complete)
        XCTAssertEqual(empty.headline.code, "no_clearance")
        XCTAssertTrue(empty.candidates.isEmpty)
        XCTAssertEqual(empty.facesLine?.code, "faces_none")

        let stopped = Answers.report(Samples.reportIncomplete)
        XCTAssertEqual(stopped.status, .incomplete)
        XCTAssertEqual(stopped.headline.code, "incomplete")
        XCTAssertEqual(stopped.reason?.code, "stopped_heat")
        XCTAssertEqual(stopped.counts.framesDeclared, 3)
        XCTAssertEqual(stopped.warnings.map(\.code), ["old_file", "timestamps_disagree"])
        // An unfinished scan never carries the clearance sentence.
        XCTAssertFalse(stopped.disclosure.contains { $0.code == "no_clearance" })
        XCTAssertEqual(stopped.tracks.first?.status, .tooBrief)
    }

    func testRefusalsAreReportsAndAnswers() {
        let refused = Answers.report(Samples.refusalStart)
        XCTAssertEqual(refused.status, .refused)
        XCTAssertEqual(refused.refusal, "missing_pack")
        XCTAssertEqual(refused.headline.code, "missing_pack")
        XCTAssertTrue(refused.tracks.isEmpty)

        let answer = Answers.decode(Samples.postersMissing, as: PostersAnswer.self)
        XCTAssertEqual(answer, .refused(code: "missing_pack", headline: Line(code: "missing_pack", text: "The poster pack is missing. Refusing.")))
        XCTAssertNil(Answers.decode(Samples.leaveBad, as: LeaveAnswer.self).value)
        XCTAssertNil(Answers.decode(Samples.deleteForeign, as: DeleteAnswer.self).value)

        // Unreadable JSON is the core's internal refusal, never a success.
        let garbage = Answers.decode("not json", as: SweepAnswer.self)
        XCTAssertEqual(garbage, .refused(code: "internal", headline: .internalFailure))
        XCTAssertEqual(Answers.report("{").status, .refused)
        XCTAssertEqual(Report.refusal(.internalFailure).headline.code, "internal")
    }

    func testEstimates() throws {
        let video = try XCTUnwrap(Answers.decode(Samples.estimateVideo, as: Estimate.self).value)
        XCTAssertTrue(video.canStart)
        XCTAssertEqual(video.coverage, "measured")
        XCTAssertEqual(video.coverageBanner?.code, "brief_face")
        XCTAssertEqual(video.framesAnalyzed, 300)
        XCTAssertEqual(video.detectionLongSide, 1920)
        XCTAssertEqual(video.human.code, "duration_seconds")
        XCTAssertEqual(video.human.int("count"), 10)
        XCTAssertEqual(video.notes.map(\.code), ["cpu_note", "heat_note", "battery_note"])
        XCTAssertEqual(video.classes.code, "classes_missing")
        // The file's warnings and the disclosure come from the core, in its order.
        XCTAssertEqual(video.warnings, [])
        XCTAssertEqual(video.disclosure.count, 9)
        XCTAssertEqual(video.disclosure.first?.code, "on_device")
        XCTAssertTrue(video.disclosure.contains { $0.code == "no_clearance" })

        let none = try XCTUnwrap(Answers.decode(Samples.estimateNoClass, as: Estimate.self).value)
        XCTAssertFalse(none.canStart)
        XCTAssertEqual(none.classes.code, "choose_class")
        XCTAssertNil(none.coverageBanner)
    }

    func testCatalogPostersVersionLeaveAndCopy() throws {
        let rows = try XCTUnwrap(Answers.decode(Samples.bundles, as: BundlesAnswer.self).value)
        XCTAssertEqual(rows.bundles.map(\.id), ["fast", "accurate"])
        XCTAssertTrue(rows.bundles[0].preselected)
        XCTAssertEqual(rows.bundles[1].curveLine.code, "not_measured")
        XCTAssertEqual(rows.note.code, "bundle_note")

        let pack = try XCTUnwrap(Answers.decode(Samples.postersCheck, as: PostersAnswer.self).value).posters
        XCTAssertEqual(pack.missing + pack.wanted, 2)
        XCTAssertNotNil(pack.created)
        XCTAssertNotNil(Answers.decode(Samples.postersDemo, as: PostersAnswer.self).value)

        let version = try XCTUnwrap(Answers.decode(Samples.version, as: VersionAnswer.self).value)
        XCTAssertEqual(version.api, 1)
        XCTAssertTrue(version.demoPosters)

        XCTAssertEqual(Answers.decode(Samples.leaveOK, as: LeaveAnswer.self).value?.url, "https://www.fbi.gov/wanted")
        XCTAssertEqual(Answers.decode(Samples.delete, as: DeleteAnswer.self).value?.deleted, true)
        XCTAssertEqual(Answers.decode(Samples.sweep, as: SweepAnswer.self).value?.removed, 0)

        let copy = try XCTUnwrap(Answers.decode(Samples.copy, as: CopyAnswer.self).value)
        let table = CopyTable(texts: copy.copy)
        XCTAssertEqual(table.line("possible_candidate").text, "Possible candidate. Not an identification.")
    }

    func testEvents() throws {
        guard case .face(let id, let frame, let label, let crop)? = ScanEvent.parse(Samples.faceEvent) else {
            return XCTFail("face event")
        }
        XCTAssertEqual(id, 1)
        XCTAssertEqual(frame, 0)
        XCTAssertEqual(label.code, "frame")
        XCTAssertEqual(crop, "crops/face_1.jpg")
        guard case .track(let track)? = ScanEvent.parse(Samples.trackNotCompared) else {
            return XCTFail("track event")
        }
        XCTAssertEqual(track.status, .notCompared)
        XCTAssertEqual(track.label.code, "not_compared")
        XCTAssertEqual(ScanEvent.parse(Samples.progressEvent), .progress(framesAnalyzed: 450, frame: 899))
        XCTAssertEqual(ScanEvent.parse(#"{"event":"new_kind"}"#), .other)
        XCTAssertNil(ScanEvent.parse("{}"))
    }

    func testAnUnknownTrackStatusIsNeverACandidate() throws {
        let json = #"{"track_id":9,"first_frame":0,"last_frame":2,"frames":{"code":"frames_span","text":"Frames 1 to 3."},"face_px":40,"quality_frames":0,"status":"someday","label":{"code":"x","text":"X."},"crop":null}"#
        let track = try JSONDecoder().decode(TrackItem.self, from: Data(json.utf8))
        XCTAssertEqual(track.status, .unscored)
        XCTAssertNil(track.crop)
    }

    func testLinesLocalizeByCodeAndKeepValues() {
        let plain = Line(code: "no_clearance", text: "No candidate is not a clearance.")
        XCTAssertEqual(plain.localized(in: .main), "No candidate is not a clearance.")
        let valued = Line(code: "frame", text: "Frame 12.", args: ["frame": .number(12)])
        XCTAssertEqual(valued.localized(in: .main), "Frame 12.")
        XCTAssertEqual(valued.int("frame"), 12)
        XCTAssertNil(valued.int("nope"))
    }
}
