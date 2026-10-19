// SPDX-License-Identifier: Apache-2.0
#if canImport(OpenWorldCore)
import Foundation
import OpenWorldCore

/// The C API linked into this binary: `OpenWorldCore.xcframework` in the app, or
/// `libopenworld_core` on Linux when `OPENWORLD_LIB_DIR` is set.
public final class NativeCore: Core {
    public init() {}

    public func call(_ request: String) -> String {
        let answer = request.withCString { ow_call($0, nil, nil) }
        return NativeCore.take(answer) ?? NativeCore.internalRefusal
    }

    public func call(_ request: String, onEvent: @escaping @Sendable (String) -> Void) -> String {
        // The call is synchronous, so the box lives exactly as long as its events can arrive.
        let events = Unmanaged.passRetained(EventBox(onEvent))
        defer { events.release() }
        let answer = request.withCString { text in
            ow_call(text, { json, user in
                guard let json, let user else { return }
                Unmanaged<EventBox>.fromOpaque(user).takeUnretainedValue().send(String(cString: json))
            }, events.toOpaque())
        }
        return NativeCore.take(answer) ?? NativeCore.internalRefusal
    }

    public func startJob(_ request: String, onEvent: @escaping @Sendable (String) -> Void) -> JobStart {
        // Retained until the job is finished or freed: events arrive during pushes and finish.
        let events = Unmanaged.passRetained(EventBox(onEvent))
        var refusal: UnsafeMutablePointer<CChar>?
        let job = request.withCString { text in
            ow_job_start(text, { json, user in
                guard let json, let user else { return }
                Unmanaged<EventBox>.fromOpaque(user).takeUnretainedValue().send(String(cString: json))
            }, events.toOpaque(), &refusal)
        }
        guard let job else {
            events.release()
            return .refused(NativeCore.take(refusal) ?? NativeCore.internalRefusal)
        }
        return .started(NativeJob(handle: job, events: events))
    }

    /// Copy a returned string and free it.
    static func take(_ pointer: UnsafeMutablePointer<CChar>?) -> String? {
        guard let pointer else { return nil }
        defer { ow_string_free(pointer) }
        return String(cString: pointer)
    }

    static let internalRefusal = #"{"ok":false,"status":"refused","refusal":"internal","headline":{"code":"internal","text":"The scan library failed. Refusing."}}"#
}

private final class EventBox: Sendable {
    let send: @Sendable (String) -> Void

    init(_ send: @escaping @Sendable (String) -> Void) {
        self.send = send
    }
}

private final class NativeJob: CoreJob {
    private var handle: OpaquePointer?
    private var events: Unmanaged<EventBox>?

    init(handle: OpaquePointer, events: Unmanaged<EventBox>) {
        self.handle = handle
        self.events = events
    }

    deinit {
        // Abandoned without a report. Its result folder stays until delete or sweep.
        if let handle {
            ow_job_free(handle)
        }
        events?.release()
    }

    func wants(_ index: UInt64) -> Bool {
        guard let handle else { return false }
        return ow_job_wants(handle, index) != 0
    }

    func skip(_ index: UInt64) -> Bool {
        guard let handle else { return false }
        return ow_job_skip(handle, index) != 0
    }

    func push(_ index: UInt64, pixels f: PixelFrame) -> Bool {
        guard let handle else { return false }
        return ow_job_push_pixels(
            handle, index,
            f.base.assumingMemoryBound(to: UInt8.self),
            UInt32(clamping: f.width), UInt32(clamping: f.height), UInt32(clamping: f.stride),
            f.layout.rawValue, f.rotation.rawValue
        ) != 0
    }

    func push(_ index: UInt64, yuv f: YUVFrame) -> Bool {
        guard let handle else { return false }
        return ow_job_push_yuv420(
            handle, index,
            f.y.assumingMemoryBound(to: UInt8.self), UInt32(clamping: f.yStride),
            f.u.assumingMemoryBound(to: UInt8.self), f.v.assumingMemoryBound(to: UInt8.self),
            UInt32(clamping: f.uvStride), UInt32(clamping: f.uvStep),
            UInt32(clamping: f.width), UInt32(clamping: f.height),
            f.fullRange ? 1 : 0, f.bt709 ? 1 : 0,
            f.rotation.rawValue
        ) != 0
    }

    func stopToken() -> (any StopToken)? {
        guard let handle, let token = ow_job_stop_token(handle) else { return nil }
        return NativeStop(token)
    }

    func finish(clean: Bool) -> String {
        guard let handle else { return NativeCore.internalRefusal }
        self.handle = nil
        let end = clean ? #"{"clean":true}"# : #"{"clean":false}"#
        let report = end.withCString { ow_job_finish(handle, $0) }
        events?.release()
        events = nil
        return NativeCore.take(report) ?? NativeCore.internalRefusal
    }
}

/// `ow_stop` is safe from any thread; the token outlives its job.
private final class NativeStop: StopToken, @unchecked Sendable {
    private let token: OpaquePointer

    init(_ token: OpaquePointer) {
        self.token = token
    }

    deinit {
        ow_stop_free(token)
    }

    func stop(_ reason: StopReason) {
        reason.rawValue.withCString { ow_stop(token, $0) }
    }
}
#endif
