// SPDX-License-Identifier: Apache-2.0
import Foundation

/// Decodes a file into a job. Built by the platform probe (AVFoundation, ImageIO) and
/// run on the scan queue. Returns true when the decoder reached the end without an error.
public protocol DecodePlan: Sendable {
    func decode(into sink: FrameSink) -> Bool
}

/// The pushed job as a decoder sees it. Used on the scan queue only.
/// A stop is read by the core on the next push, which then returns false. The decoder
/// keeps going until then, so the report names the stop and not a short decode.
public final class FrameSink {
    private let job: any CoreJob
    /// Frames delivered so far, pushed or skipped.
    public private(set) var delivered: UInt64 = 0
    /// The core stopped taking frames.
    public private(set) var stopped = false

    init(job: any CoreJob) {
        self.job = job
    }

    /// Ask before converting frame `index`. False means call `skip`.
    public func wants(_ index: UInt64) -> Bool {
        !stopped && job.wants(index)
    }

    /// Count a frame the core does not want. False when the scan stopped.
    @discardableResult
    public func skip(_ index: UInt64) -> Bool {
        keep(job.skip(index))
    }

    @discardableResult
    public func push(_ index: UInt64, pixels: PixelFrame) -> Bool {
        keep(job.push(index, pixels: pixels))
    }

    @discardableResult
    public func push(_ index: UInt64, yuv: YUVFrame) -> Bool {
        keep(job.push(index, yuv: yuv))
    }

    private func keep(_ accepted: Bool) -> Bool {
        if accepted {
            delivered += 1
        } else {
            stopped = true
        }
        return accepted
    }
}

/// A stop asked for before the job has a token is applied when it gets one.
public final class StopSwitch: @unchecked Sendable {
    private let lock = NSLock()
    private var token: (any StopToken)?
    private var reason: StopReason?

    public init() {}

    public func stop(_ reason: StopReason) {
        lock.lock()
        let first = self.reason == nil
        if first { self.reason = reason }
        let token = self.token
        lock.unlock()
        if first { token?.stop(reason) }
    }

    func attach(_ token: (any StopToken)?) {
        lock.lock()
        self.token = token
        let reason = self.reason
        lock.unlock()
        if let reason { token?.stop(reason) }
    }

    func detach() {
        lock.lock()
        token = nil
        lock.unlock()
    }
}

public enum ScanUpdate: Sendable {
    case event(ScanEvent)
    /// The job ended. A refused start is a refused report.
    case finished(Report)
}

/// Runs one pushed scan on its own serial queue. Events and the report arrive in order.
public final class ScanRunner: Sendable {
    public let stopSwitch = StopSwitch()
    private let queue = DispatchQueue(label: "app.openworld.scan", qos: .userInitiated)

    public init() {}

    public func stop(_ reason: StopReason) {
        stopSwitch.stop(reason)
    }

    public func run(core: any Core, request: String, plan: any DecodePlan) -> AsyncStream<ScanUpdate> {
        let (stream, continuation) = AsyncStream.makeStream(of: ScanUpdate.self)
        let stop = stopSwitch
        queue.async {
            let started = core.startJob(request) { json in
                if let event = ScanEvent.parse(json) {
                    continuation.yield(.event(event))
                }
            }
            switch started {
            case .refused(let json):
                continuation.yield(.finished(Answers.report(json)))
            case .started(let job):
                stop.attach(job.stopToken())
                let sink = FrameSink(job: job)
                let reachedEnd = plan.decode(into: sink)
                let report = job.finish(clean: reachedEnd && !sink.stopped)
                stop.detach()
                continuation.yield(.finished(Answers.report(report)))
            }
            continuation.finish()
        }
        return stream
    }
}
