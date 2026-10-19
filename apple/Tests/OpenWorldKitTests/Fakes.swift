// SPDX-License-Identifier: Apache-2.0
import Foundation
@testable import OpenWorldKit

/// A core that answers with the real samples and records what it was asked.
final class FakeCore: Core, @unchecked Sendable {
    private let lock = NSLock()
    private var _requests: [String] = []
    private var _pushed: [UInt64] = []
    private var _skipped: [UInt64] = []
    private var _stopReason: StopReason?
    private var _jobRequests: [String] = []

    /// Answers by op. Missing ops answer with the internal refusal.
    var answers: [String: String] = [
        "copy": Samples.copy,
        "version": Samples.version,
        "bundles": Samples.bundles,
        "posters_check": Samples.postersCheck,
        "posters_demo": Samples.postersDemo,
        "estimate": Samples.estimateVideo,
        "leave": Samples.leaveOK,
        "delete": Samples.delete,
        "sweep": Samples.sweep,
    ]
    /// Set to refuse `ow_job_start`.
    var jobRefusal: String?
    /// Events sent on the first push.
    var events = [Samples.faceEvent, Samples.faceEvent2, Samples.trackNotCompared]
    /// The report when the job ends without a stop.
    var report = Samples.reportCandidate
    /// Frames the job declines, as `ow_job_wants` would under measured coverage.
    var declined: Set<UInt64> = []

    var requests: [String] { lock.withLock { _requests } }
    var jobRequests: [String] { lock.withLock { _jobRequests } }
    var pushed: [UInt64] { lock.withLock { _pushed } }
    var skipped: [UInt64] { lock.withLock { _skipped } }
    var stopReason: StopReason? { lock.withLock { _stopReason } }

    func call(_ request: String) -> String {
        lock.withLock { _requests.append(request) }
        let op = (try? JSONSerialization.jsonObject(with: Data(request.utf8)) as? [String: Any])?["op"] as? String ?? ""
        return answers[op] ?? #"{"ok":false,"refusal":"internal","headline":{"code":"internal","text":"The scan library failed. Refusing."}}"#
    }

    func startJob(_ request: String, onEvent: @escaping @Sendable (String) -> Void) -> JobStart {
        lock.withLock { _jobRequests.append(request) }
        if let jobRefusal {
            return .refused(jobRefusal)
        }
        return .started(FakeJob(core: self, onEvent: onEvent))
    }

    fileprivate func record(push index: UInt64) -> Bool {
        lock.withLock {
            _pushed.append(index)
            return _stopReason == nil
        }
    }

    fileprivate func record(skip index: UInt64) -> Bool {
        lock.withLock {
            _skipped.append(index)
            return _stopReason == nil
        }
    }

    fileprivate func stop(_ reason: StopReason) {
        lock.withLock {
            if _stopReason == nil { _stopReason = reason }
        }
    }

    fileprivate func finishReport() -> String {
        guard let reason = stopReason else { return report }
        let codes: [StopReason: (String, String)] = [
            .cancel: ("cancelled", "You stopped the scan before every frame was analyzed."),
            .heat: ("stopped_heat", "The device got too hot, so the scan stopped."),
            .battery: ("stopped_battery", "The battery ran low, so the scan stopped."),
            .background: ("stopped_background", "The app left the screen, so the scan stopped."),
        ]
        let (code, text) = codes[reason]!
        return Samples.reportIncomplete.replacingOccurrences(
            of: #"{"code":"stopped_heat","text":"The device got too hot, so the scan stopped."}"#,
            with: #"{"code":"\#(code)","text":"\#(text)"}"#
        )
    }
}

private final class FakeJob: CoreJob {
    let core: FakeCore
    let onEvent: @Sendable (String) -> Void
    var sentEvents = false

    init(core: FakeCore, onEvent: @escaping @Sendable (String) -> Void) {
        self.core = core
        self.onEvent = onEvent
    }

    func wants(_ index: UInt64) -> Bool { !core.declined.contains(index) }

    func skip(_ index: UInt64) -> Bool { core.record(skip: index) }

    func push(_ index: UInt64, pixels: PixelFrame) -> Bool { take(index) }

    func push(_ index: UInt64, yuv: YUVFrame) -> Bool { take(index) }

    private func take(_ index: UInt64) -> Bool {
        let keep = core.record(push: index)
        if !sentEvents {
            sentEvents = true
            core.events.forEach(onEvent)
        }
        return keep
    }

    func stopToken() -> (any StopToken)? { FakeStop(core: core) }

    func finish(clean: Bool) -> String { core.finishReport() }
}

private final class FakeStop: StopToken, @unchecked Sendable {
    let core: FakeCore
    init(core: FakeCore) { self.core = core }
    func stop(_ reason: StopReason) { core.stop(reason) }
}

/// Pushes `frames` small grey pictures. A gate holds it after the first frame.
struct FakePlan: DecodePlan {
    var frames: UInt64
    var gate: DispatchSemaphore?
    var reachesEnd = true

    func decode(into sink: FrameSink) -> Bool {
        let pixels = [UInt8](repeating: 128, count: 4 * 4 * 4)
        for index in 0..<frames {
            if sink.wants(index) {
                let kept = pixels.withUnsafeBytes { raw in
                    sink.push(index, pixels: PixelFrame(base: raw.baseAddress!, width: 4, height: 4, stride: 16, layout: .rgba, rotation: .up))
                }
                if !kept { return false }
            } else if !sink.skip(index) {
                return false
            }
            if index == 0 { gate?.wait() }
        }
        return reachesEnd
    }
}

struct FakeMedia: MediaReader {
    var facts = MediaFacts(width: 1920, height: 1080, fps: 30, frames: 3, durationSec: 0.1, video: true)
    var failure: ProbeFailure?
    var recorded: Date?
    var gate: DispatchSemaphore?

    func probe(_ file: ImportedFile) async throws -> ProbedMedia {
        if let failure { throw failure }
        return ProbedMedia(facts: facts, kind: .video, recorded: recorded, plan: FakePlan(frames: facts.frames, gate: gate))
    }
}

func temporaryPlaces() throws -> Places {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent("openworld-tests-\(UUID().uuidString)", isDirectory: true)
    let places = Places(
        results: root.appendingPathComponent("results"),
        posters: root.appendingPathComponent("posters"),
        journal: root.appendingPathComponent("journal"),
        imports: root.appendingPathComponent("imports"),
        bundles: root.appendingPathComponent("bundles")
    )
    try places.prepare()
    return places
}

/// Wait for the main actor to reach a state, or fail after two seconds.
@MainActor
func eventually(_ condition: () -> Bool, file: StaticString = #filePath, line: UInt = #line) async -> Bool {
    for _ in 0..<200 {
        if condition() { return true }
        try? await Task.sleep(nanoseconds: 10_000_000)
    }
    return condition()
}
