// SPDX-License-Identifier: Apache-2.0
import AVFoundation
@testable import OpenWorldKit
import XCTest

/// The app's own decoders (App/Platform, compiled into this bundle) on media made here with
/// AVAssetWriter, AVAssetExportSession, and ImageIO, scanned by the linked core. Runs on the
/// iOS simulator and the Mac.
final class DecoderTests: XCTestCase {
    private var work: URL!
    private var places: Places!
    private var core: (any Core)!

    override func setUpWithError() throws {
        guard let linked = Linked.core() else { throw XCTSkip("The core is not linked.") }
        core = linked
        work = FileManager.default.temporaryDirectory.appendingPathComponent("openworld-decoders-\(UUID().uuidString)", isDirectory: true)
        // The repository's bundle catalog, next to apple/.
        let repo = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        places = Places(
            results: work.appendingPathComponent("results"),
            posters: work.appendingPathComponent("posters"),
            journal: work.appendingPathComponent("journal"),
            imports: work.appendingPathComponent("imports"),
            bundles: repo.appendingPathComponent("bundles", isDirectory: true)
        )
        try places.prepare()
        let pack = Answers.decode(core.call(Request.postersDemo(out: places.posters.path).json), as: PostersAnswer.self)
        XCTAssertNotNil(pack.value, "the demo pack was not written")
    }

    override func tearDownWithError() throws {
        if let work { try? FileManager.default.removeItem(at: work) }
    }

    // MARK: Tests

    func testAVideoFindsTheMarker() async throws {
        let url = try movie("upright.mov")
        let probed = try await MediaProbe().probe(file(url))
        XCTAssertEqual(probed.kind, .video)
        XCTAssertEqual(probed.facts.frames, UInt64(Scene.frames))
        XCTAssertFalse(probed.countIsEstimate)
        XCTAssertEqual(probed.facts.width, Scene.width)
        XCTAssertEqual(probed.facts.height, Scene.height)
        XCTAssertEqual(decodedCount(probed), UInt64(Scene.frames))
        let report = await scan(url, probed)
        try assertCandidate(report, frames: Scene.frames)
    }

    func testARotatedVideoIsScannedUpright() async throws {
        // Stored portrait; the display transform turns it 90 degrees clockwise, like an iPhone.
        let url = try movie("turned.mov", turned: true)
        let probed = try await MediaProbe().probe(file(url))
        XCTAssertEqual((probed.plan as? VideoPlan)?.rotation, .right)
        XCTAssertEqual(probed.facts.width, Scene.width, "the probe reports the upright size")
        XCTAssertEqual(probed.facts.height, Scene.height)
        let report = await scan(url, probed)
        try assertCandidate(report, frames: Scene.frames)
    }

    /// A trimmed clip has an edit list: passthrough keeps the samples from the key frame
    /// before the cut, and only the edited range is shown. The probe's count must be the
    /// decoder's, or the scan would be Incomplete for a file that decoded cleanly.
    func testATrimmedClipIsCountedAsItDecodes() async throws {
        // 30 frames, key frames every 8. The marker is on source frames 15 to 26.
        let source = work.appendingPathComponent("source.mov")
        let marked = Scene.gray(markerAt: Scene.spot(frames: Scene.shown))
        let blank = Scene.gray(markerAt: nil)
        let offset = 5
        let codec = try MediaMaker.movie(at: source, width: Scene.width, height: Scene.height, count: 30, keyFrameInterval: 8) { frame in
            Scene.shown.contains(frame - offset) ? marked : blank
        }
        // Cut at frame 5, between key frames 0 and 8, and before the last frame: 24 shown.
        let trimmed = work.appendingPathComponent("trimmed.mov")
        try await MediaMaker.trim(source, frames: offset..<(offset + 24), to: trimmed)

        let probed = try await MediaProbe().probe(file(trimmed))
        let asset = AVURLAsset(url: trimmed)
        let tracks = try await asset.loadTracks(withMediaType: .video)
        let track = try XCTUnwrap(tracks.first)
        let stored = VideoDecoder.countFrames(asset: asset, track: track)
        let decoded = decodedCount(probed)
        let note = "codec \(codec.rawValue): probe \(probed.facts.frames), decoded \(decoded), counted without the timeline check \(stored.map { "\($0)" } ?? "nil")"
        print(note)
        XCTAssertFalse(probed.countIsEstimate, note)
        XCTAssertEqual(decoded, 24, "the decoder delivers the edited range: \(note)")
        XCTAssertEqual(probed.facts.frames, decoded, note)
        let report = await scan(trimmed, probed)
        try assertCandidate(report, frames: 24)
    }

    func testAnAnimatedGIFScansEveryFrame() async throws {
        let url = work.appendingPathComponent("marker.gif")
        let marked = Scene.gray(markerAt: Scene.spot(frames: Scene.shown))
        let blank = Scene.gray(markerAt: nil)
        try MediaMaker.gif(at: url, count: Scene.frames) { Scene.shown.contains($0) ? marked : blank }
        let probed = try await MediaProbe().probe(file(url))
        XCTAssertEqual(probed.kind, .animation)
        XCTAssertEqual(probed.facts.frames, UInt64(Scene.frames))
        XCTAssertEqual(probed.facts.fps, 10, accuracy: 0.01)
        XCTAssertEqual(decodedCount(probed), UInt64(Scene.frames))
        let report = await scan(url, probed)
        try assertCandidate(report, frames: Scene.frames)
    }

    // MARK: Helpers

    /// 2.5 s at 10 fps, the marker on frames 10 to 21.
    private func movie(_ name: String, turned: Bool = false) throws -> URL {
        let url = work.appendingPathComponent(name)
        let marked = Scene.gray(markerAt: Scene.spot(frames: Scene.shown))
        let blank = Scene.gray(markerAt: nil)
        let pictures = turned ? [Scene.storedForQuarterTurn(blank), Scene.storedForQuarterTurn(marked)] : [blank, marked]
        let (width, height) = turned ? (Scene.height, Scene.width) : (Scene.width, Scene.height)
        try MediaMaker.movie(
            at: url, width: width, height: height, count: Scene.frames,
            transform: turned ? CGAffineTransform(rotationAngle: .pi / 2) : .identity
        ) { Scene.shown.contains($0) ? pictures[1] : pictures[0] }
        return url
    }

    private func file(_ url: URL) -> ImportedFile {
        ImportedFile(url: url, name: url.lastPathComponent, modified: nil, origin: .files)
    }

    /// Pictures the app's decode plan delivers, without the core.
    private func decodedCount(_ probed: ProbedMedia) -> UInt64 {
        let sink = FrameSink(job: CountingJob())
        let reachedEnd = probed.plan.decode(into: sink)
        XCTAssertTrue(reachedEnd, "the decoder stopped early")
        return sink.delivered
    }

    /// The scan as the app runs it: ScanRunner, the probe's facts and plan, the linked core.
    private func scan(_ url: URL, _ probed: ProbedMedia) async -> Report? {
        let request = ScanRequest(
            input: url.path, bundles: places.bundles.path, bundle: "fast", posters: places.posters.path,
            out: places.newResultFolder().path, coverage: .complete, missing: true, wanted: true,
            device: .computer, media: probed.facts, fileMtimeUnix: nil
        )
        var report: Report?
        for await update in ScanRunner().run(core: core, request: Request.job(request).json, plan: probed.plan) {
            if case .finished(let finished) = update { report = finished }
        }
        return report
    }

    private func assertCandidate(_ report: Report?, frames: Int, line: UInt = #line) throws {
        let report = try XCTUnwrap(report, line: line)
        let note = "\(report.status) \(report.headline.code) reason \(report.reason?.code ?? "none") decoded \(report.counts.framesDecoded) declared \(report.counts.framesDeclared.map { "\($0)" } ?? "none") tracks \(report.tracks.map(\.status))"
        XCTAssertEqual(report.status, .complete, note, line: line)
        XCTAssertEqual(report.headline.code, "possible_candidate", note, line: line)
        XCTAssertEqual(report.counts.framesDecoded, UInt64(frames), note, line: line)
        XCTAssertEqual(report.counts.framesDeclared, UInt64(frames), note, line: line)
        XCTAssertEqual(report.candidates.first?.poster.id, "fixture-missing-a", note, line: line)
    }
}

/// A job that takes every picture and only counts. No core.
private final class CountingJob: CoreJob {
    func wants(_ index: UInt64) -> Bool { true }
    func skip(_ index: UInt64) -> Bool { true }
    func push(_ index: UInt64, pixels: PixelFrame) -> Bool { true }
    func push(_ index: UInt64, yuv: YUVFrame) -> Bool { true }
    func stopToken() -> (any StopToken)? { nil }
    func finish(clean: Bool) -> String { "" }
}
