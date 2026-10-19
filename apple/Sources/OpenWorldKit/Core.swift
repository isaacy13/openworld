// SPDX-License-Identifier: Apache-2.0
import Foundation
#if canImport(Glibc)
import Glibc
#endif

/// The scan library (`include/openworld.h`). The shell never detects, tracks, or compares.
/// `NativeCore` calls the linked C API; tests use a fake.
public protocol Core: Sendable {
    /// `ow_call`: one JSON request, one JSON answer.
    func call(_ request: String) -> String
    /// `ow_job_start`. `onEvent` runs on the thread that pushes frames.
    func startJob(_ request: String, onEvent: @escaping @Sendable (String) -> Void) -> JobStart
}

public enum JobStart {
    case started(any CoreJob)
    /// The refusal JSON: `{ok: false, refusal, headline}`.
    case refused(String)
}

/// A pushed scan. Use it from one thread, in frame order.
public protocol CoreJob: AnyObject {
    /// `ow_job_wants`: false means skip this frame without converting it.
    func wants(_ index: UInt64) -> Bool
    /// `ow_job_skip`. False when the scan stopped.
    func skip(_ index: UInt64) -> Bool
    /// `ow_job_push_pixels`. False when the scan stopped.
    func push(_ index: UInt64, pixels: PixelFrame) -> Bool
    /// `ow_job_push_yuv420`. False when the scan stopped.
    func push(_ index: UInt64, yuv: YUVFrame) -> Bool
    /// `ow_job_stop_token`: stops this job from any thread.
    func stopToken() -> (any StopToken)?
    /// `ow_job_finish`: the report JSON. The job is closed after this.
    func finish(clean: Bool) -> String
}

public protocol StopToken: AnyObject, Sendable {
    /// `ow_stop`. The first reason wins.
    func stop(_ reason: StopReason)
}

public enum StopReason: String, Sendable {
    case cancel, heat, battery, background
}

/// Clockwise degrees that turn the stored picture upright.
public enum Rotation: UInt32, Sendable {
    case up = 0
    case right = 90
    case down = 180
    case left = 270

    /// The rotation in a display transform `[a b; c d]` (CGAffineTransform's a, b, c, d),
    /// in a y-down picture. The second column is where "up" goes, so a mirrored transform
    /// keeps the picture upright (mirrored, which the core does not undo). Angles between
    /// quarter turns snap to the nearest one.
    public static func of(a: Double, b: Double, c: Double, d: Double) -> Rotation {
        guard a.isFinite, b.isFinite, c.isFinite, d.isFinite, (c != 0 || d != 0) else { return .up }
        var degrees = atan2(-c, d) * 180 / .pi
        if degrees < 0 { degrees += 360 }
        switch Int((degrees / 90).rounded()) % 4 {
        case 1: return .right
        case 2: return .down
        case 3: return .left
        default: return .up
        }
    }

    /// True when the transform flips the picture.
    public static func isMirrored(a: Double, b: Double, c: Double, d: Double) -> Bool {
        a * d - b * c < 0
    }

    /// Width and height once turned upright.
    public func upright(width: Int, height: Int) -> (width: Int, height: Int) {
        switch self {
        case .right, .left: return (height, width)
        case .up, .down: return (width, height)
        }
    }
}

public enum PixelLayout: UInt32, Sendable {
    case rgba = 0
    case bgra = 1
    case rgb = 2
}

/// Packed pixels the caller keeps alive for the push.
public struct PixelFrame {
    public var base: UnsafeRawPointer
    public var width: Int
    public var height: Int
    public var stride: Int
    public var layout: PixelLayout
    public var rotation: Rotation

    public init(base: UnsafeRawPointer, width: Int, height: Int, stride: Int, layout: PixelLayout, rotation: Rotation) {
        self.base = base
        self.width = width
        self.height = height
        self.stride = stride
        self.layout = layout
        self.rotation = rotation
    }
}

/// 8-bit 4:2:0 planes, as a locked CVPixelBuffer hands them out. For NV12, `u` is the
/// CbCr plane, `v` is one byte after it, and `uvStep` is 2.
public struct YUVFrame {
    public var y: UnsafeRawPointer
    public var yStride: Int
    public var u: UnsafeRawPointer
    public var v: UnsafeRawPointer
    public var uvStride: Int
    public var uvStep: Int
    public var width: Int
    public var height: Int
    public var fullRange: Bool
    public var bt709: Bool
    public var rotation: Rotation

    public init(y: UnsafeRawPointer, yStride: Int, u: UnsafeRawPointer, v: UnsafeRawPointer, uvStride: Int, uvStep: Int, width: Int, height: Int, fullRange: Bool, bt709: Bool, rotation: Rotation) {
        self.y = y
        self.yStride = yStride
        self.u = u
        self.v = v
        self.uvStride = uvStride
        self.uvStep = uvStep
        self.width = width
        self.height = height
        self.fullRange = fullRange
        self.bt709 = bt709
        self.rotation = rotation
    }
}

/// The linked core, when this build has one.
public enum Linked {
    public static func core() -> (any Core)? {
        #if canImport(OpenWorldCore)
        return NativeCore()
        #else
        return nil
        #endif
    }
}
