// SPDX-License-Identifier: Apache-2.0
import Foundation

/// A library answer: what was asked for, or a refusal with its line.
public enum Answer<Value> {
    case ok(Value)
    case refused(code: String, headline: Line)

    public var value: Value? {
        if case .ok(let value) = self { return value }
        return nil
    }
}

extension Answer: Sendable where Value: Sendable {}
extension Answer: Equatable where Value: Equatable {}

private struct Envelope: Decodable {
    var ok: Bool?
    var refusal: String?
    var headline: Line?
}

public enum Answers {
    /// Read `json` as `Value`, unless it is a refusal. Unreadable JSON is the core's internal refusal.
    public static func decode<Value: Decodable>(_ json: String, as type: Value.Type = Value.self) -> Answer<Value> {
        let data = Data(json.utf8)
        let decoder = JSONDecoder()
        guard let envelope = try? decoder.decode(Envelope.self, from: data) else {
            return .refused(code: Line.internalFailure.code, headline: .internalFailure)
        }
        if envelope.ok == false {
            let headline = envelope.headline ?? .internalFailure
            return .refused(code: envelope.refusal ?? headline.code, headline: headline)
        }
        guard let value = try? decoder.decode(Value.self, from: data) else {
            return .refused(code: Line.internalFailure.code, headline: .internalFailure)
        }
        return .ok(value)
    }

    /// A report from `ow_job_finish`. A refused scan is still a report.
    public static func report(_ json: String) -> Report {
        if let report = try? JSONDecoder().decode(Report.self, from: Data(json.utf8)) {
            return report
        }
        return .refusal(.internalFailure)
    }
}

extension Report {
    /// A refusal with no scan behind it.
    public static func refusal(_ line: Line) -> Report {
        let json = #"{"status":"refused","headline":{"code":"","text":""}}"#
        // Built from JSON so every field takes the same defaults a decoded refusal has.
        var report = try! JSONDecoder().decode(Report.self, from: Data(json.utf8))
        report.headline = line
        report.refusal = line.code
        return report
    }
}

/// `estimate`: the plan before Analyze.
public struct Estimate: Decodable, Equatable, Sendable {
    public var seconds: Double
    public var human: Line
    public var framesAnalyzed: UInt64
    public var detectionLongSide: Int
    public var detectionLine: Line
    public var coverage: String
    public var coverageLine: Line
    public var coverageBanner: Line?
    public var timed: Bool
    public var notes: [Line]
    public var suggestComputer: Bool
    public var caveat: Line
    /// About the file itself: old, or its timestamps disagree.
    public var warnings: [Line]
    /// What the scan promises, shown before Analyze.
    public var disclosure: [Line]
    public var classes: Line
    public var canStart: Bool

    enum CodingKeys: String, CodingKey {
        case seconds, human
        case framesAnalyzed = "frames_analyzed"
        case detectionLongSide = "detection_long_side"
        case detectionLine = "detection_line"
        case coverage
        case coverageLine = "coverage_line"
        case coverageBanner = "coverage_banner"
        case timed, notes
        case suggestComputer = "suggest_computer"
        case caveat, warnings, disclosure, classes
        case canStart = "can_start"
    }
}

/// One row of the bundle picker.
public struct BundleRow: Decodable, Equatable, Sendable, Identifiable {
    public var id: String
    public var name: String
    public var version: String?
    public var bestFor: String
    public var preselected: Bool
    public var curveExists: Bool
    public var curveLine: Line
    public var realPostersAllowed: Bool
    public var weightsReady: Bool
    public var official: Bool
    public var unofficialNotice: String?

    enum CodingKeys: String, CodingKey {
        case id, name, version
        case bestFor = "best_for"
        case preselected
        case curveExists = "curve_exists"
        case curveLine = "curve_line"
        case realPostersAllowed = "real_posters_allowed"
        case weightsReady = "weights_ready"
        case official
        case unofficialNotice = "unofficial_notice"
    }
}

public struct BundlesAnswer: Decodable, Equatable, Sendable {
    public var bundles: [BundleRow]
    public var note: Line
}

public struct VersionAnswer: Decodable, Equatable, Sendable {
    public var api: Int
    public var library: String
    public var demoPosters: Bool
    public var providers: [String]

    enum CodingKeys: String, CodingKey {
        case api, library
        case demoPosters = "demo_posters"
        case providers
    }
}

public struct PostersAnswer: Decodable, Equatable, Sendable {
    public var posters: PackInfo
}

/// `posters_update`: the new pack, how many posters were kept, and how many were left out, by reason.
public struct PostersUpdateAnswer: Decodable, Equatable, Sendable {
    public var posters: PackInfo
    public var kept: Int
    public var skipped: [SkippedPosters]
    public var listSha256: String

    enum CodingKeys: String, CodingKey {
        case posters, kept, skipped
        case listSha256 = "list_sha256"
    }
}

/// Posters left out for one reason.
public struct SkippedPosters: Decodable, Equatable, Sendable {
    public var reason: Line
    public var count: Int
}

/// What Update posters sends while it runs.
public enum PostersEvent: Decodable, Equatable, Sendable {
    /// A page of the FBI list was read. `pages` is nil until the list gives a total.
    case page(page: Int, pages: Int?)
    /// This many of the poster photos were checked.
    case checked(checked: Int, posters: Int)
    case other

    enum CodingKeys: String, CodingKey {
        case event, page, pages, checked, posters
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .event) {
        case "posters_page":
            self = .page(page: try c.decode(Int.self, forKey: .page), pages: try c.decodeIfPresent(Int.self, forKey: .pages))
        case "posters_checked":
            self = .checked(checked: try c.decode(Int.self, forKey: .checked), posters: try c.decode(Int.self, forKey: .posters))
        default:
            self = .other
        }
    }

    public static func parse(_ json: String) -> PostersEvent? {
        try? JSONDecoder().decode(PostersEvent.self, from: Data(json.utf8))
    }
}

public struct LeaveAnswer: Decodable, Equatable, Sendable {
    public var message: Line
    public var url: String
}

public struct DeleteAnswer: Decodable, Equatable, Sendable {
    public var deleted: Bool
    public var headline: Line
}

public struct SweepAnswer: Decodable, Equatable, Sendable {
    public var removed: Int
}

public struct CopyAnswer: Decodable, Equatable, Sendable {
    public var copy: [String: String]
    /// For a report the shell makes itself. It has no clearance line.
    public var disclosure: [Line]?
}

/// What a scan sends while it runs.
public enum ScanEvent: Decodable, Equatable, Sendable {
    case progress(framesAnalyzed: UInt64, frame: UInt64)
    /// A first look at a new face. Its `track` event replaces it.
    case face(trackId: UInt64, frame: UInt64, frameLabel: Line, crop: String?)
    case track(TrackItem)
    case other

    enum CodingKeys: String, CodingKey {
        case event
        case framesAnalyzed = "frames_analyzed"
        case frame
        case trackId = "track_id"
        case frameLabel = "frame_label"
        case crop, track
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .event) {
        case "progress":
            self = .progress(
                framesAnalyzed: try c.decode(UInt64.self, forKey: .framesAnalyzed),
                frame: try c.decode(UInt64.self, forKey: .frame)
            )
        case "face":
            self = .face(
                trackId: try c.decode(UInt64.self, forKey: .trackId),
                frame: try c.decode(UInt64.self, forKey: .frame),
                frameLabel: try c.decode(Line.self, forKey: .frameLabel),
                crop: try c.decodeIfPresent(String.self, forKey: .crop)
            )
        case "track":
            self = .track(try c.decode(TrackItem.self, forKey: .track))
        default:
            self = .other
        }
    }

    public static func parse(_ json: String) -> ScanEvent? {
        try? JSONDecoder().decode(ScanEvent.self, from: Data(json.utf8))
    }
}
