// SPDX-License-Identifier: Apache-2.0
import Foundation

/// Size and timing from the platform decoder. The core refuses a different picture count.
public struct MediaFacts: Codable, Equatable, Sendable {
    public var width: Int
    public var height: Int
    public var fps: Double
    /// Pictures the decoder will deliver.
    public var frames: UInt64
    public var durationSec: Double
    public var video: Bool
    /// Container creation time, seconds since 1970, when the file has one.
    public var containerUnix: Int64?

    public init(width: Int, height: Int, fps: Double = 0, frames: UInt64 = 1, durationSec: Double = 0, video: Bool = false, containerUnix: Int64? = nil) {
        self.width = width
        self.height = height
        self.fps = fps
        self.frames = frames
        self.durationSec = durationSec
        self.video = video
        self.containerUnix = containerUnix
    }

    enum CodingKeys: String, CodingKey {
        case width, height, fps, frames
        case durationSec = "duration_sec"
        case video
        case containerUnix = "container_unix"
    }
}

/// What the device says about itself. A computer sends only its kind.
public struct DeviceFacts: Codable, Equatable, Sendable {
    /// `phone` or `computer`.
    public var kind: String
    /// nominal, fair, serious, critical.
    public var thermal: String?
    /// 0 to 1.
    public var battery: Double?
    public var charging: Bool?

    public init(kind: String, thermal: String? = nil, battery: Double? = nil, charging: Bool? = nil) {
        self.kind = kind
        self.thermal = thermal
        self.battery = battery
        self.charging = charging
    }

    public static let computer = DeviceFacts(kind: "computer")
}

public enum Coverage: String, Codable, Sendable, CaseIterable {
    /// Every decoded frame.
    case complete
    /// 5 frames a second, plus the tracker.
    case measured
}

/// A scan request (docs/api.md). The core refuses unknown fields, so only these are sent.
public struct ScanRequest: Encodable, Equatable, Sendable {
    public var input: String
    public var bundles: String
    public var bundle: String
    public var posters: String
    public var out: String
    public var coverage: Coverage
    public var missing: Bool
    public var wanted: Bool
    public var accelerate: Bool
    public var device: DeviceFacts
    public var media: MediaFacts?
    public var fileMtimeUnix: Int64?

    public init(input: String, bundles: String, bundle: String, posters: String, out: String, coverage: Coverage, missing: Bool, wanted: Bool, accelerate: Bool = true, device: DeviceFacts, media: MediaFacts?, fileMtimeUnix: Int64?) {
        self.input = input
        self.bundles = bundles
        self.bundle = bundle
        self.posters = posters
        self.out = out
        self.coverage = coverage
        self.missing = missing
        self.wanted = wanted
        self.accelerate = accelerate
        self.device = device
        self.media = media
        self.fileMtimeUnix = fileMtimeUnix
    }

    enum CodingKeys: String, CodingKey {
        case input, bundles, bundle, posters, out, coverage, missing, wanted, accelerate, device, media
        case fileMtimeUnix = "file_mtime_unix"
    }
}

/// One `ow_call` request, or the body of `ow_job_start`.
public enum Request: Sendable {
    case version
    case copy
    case bundles(catalog: String)
    case estimate(ScanRequest)
    case postersCheck(posters: String)
    case postersDemo(out: String)
    /// Fetch real posters into `posters` with `bundle`'s models.
    case postersUpdate(bundles: String, bundle: String, posters: String)
    case leave(url: String)
    case delete(out: String)
    case sweep(root: String)
    /// The terms version to accept, the first-run lines, and the documents' titles.
    case legal
    /// One built-in document: `terms`, `privacy`, `biometric`, `health_data`, or `licenses`.
    case legalDocument(id: String)
    /// `ow_job_start` takes a scan request with `v` and without `op`.
    case job(ScanRequest)

    public var json: String {
        switch self {
        case .version: return Self.encode(op: "version", [String: String]())
        case .copy: return Self.encode(op: "copy", [String: String]())
        case .bundles(let catalog): return Self.encode(op: "bundles", ["bundles": catalog])
        case .estimate(let scan): return Self.encode(op: "estimate", scan)
        case .postersCheck(let posters): return Self.encode(op: "posters_check", ["posters": posters])
        case .postersDemo(let out): return Self.encode(op: "posters_demo", ["out": out])
        case .postersUpdate(let bundles, let bundle, let posters):
            return Self.encode(op: "posters_update", ["bundles": bundles, "bundle": bundle, "posters": posters])
        case .leave(let url): return Self.encode(op: "leave", ["url": url])
        case .delete(let out): return Self.encode(op: "delete", ["out": out])
        case .sweep(let root): return Self.encode(op: "sweep", ["root": root])
        case .legal: return Self.encode(op: "legal", [String: String]())
        case .legalDocument(let id): return Self.encode(op: "legal_document", ["id": id])
        case .job(let scan): return Self.encode(op: nil, scan)
        }
    }

    static func encode<Body: Encodable>(op: String?, _ body: Body) -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        guard let data = try? encoder.encode(Versioned(op: op, body: body)) else { return "{}" }
        return String(decoding: data, as: UTF8.self)
    }
}

/// `{"v": 1, "op": ..., ...body}` in one JSON object.
private struct Versioned<Body: Encodable>: Encodable {
    let op: String?
    let body: Body

    enum Keys: String, CodingKey {
        case v, op
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(1, forKey: .v)
        try c.encodeIfPresent(op, forKey: .op)
        // The body writes its own keys into the same object.
        try body.encode(to: encoder)
    }
}
