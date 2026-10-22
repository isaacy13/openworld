// SPDX-License-Identifier: Apache-2.0
import Foundation

/// `openworld.result.v2`, as `ow_job_finish` returns it (docs/api.md).
public struct Report: Decodable, Equatable, Sendable {
    public enum Status: String, Decodable, Sendable {
        case complete, incomplete, refused
    }

    public var schema: String
    public var status: Status
    public var headline: Line
    public var reason: Line?
    public var refusal: String?
    public var bundle: BundleInfo?
    public var posters: PackInfo?
    public var perception: String?
    public var perceptionNote: Line?
    public var coverage: String?
    public var coverageLine: Line?
    public var coverageBanner: Line?
    public var detectionLongSide: Int?
    public var detectionLine: Line?
    public var provider: String?
    public var execution: String?
    public var deviceNote: Line?
    public var classes: Line?
    public var framesLine: Line?
    public var facesLine: Line?
    public var chance: Line?
    public var warnings: [Line]
    public var disclosure: [Line]
    public var counts: Counts
    public var tracks: [TrackItem]
    public var candidates: [Candidate]
    public var createdAt: String?
    public var expiresAt: String?

    enum CodingKeys: String, CodingKey {
        case schema, status, headline, reason, refusal, bundle, posters, perception
        case perceptionNote = "perception_note"
        case coverage
        case coverageLine = "coverage_line"
        case coverageBanner = "coverage_banner"
        case detectionLongSide = "detection_long_side"
        case detectionLine = "detection_line"
        case provider, execution
        case deviceNote = "device_note"
        case classes
        case framesLine = "frames_line"
        case facesLine = "faces_line"
        case chance, warnings, disclosure, counts, tracks, candidates
        case createdAt = "created_at"
        case expiresAt = "expires_at"
    }

    /// A refusal from `ow_job_start` is `{ok, status, refusal, headline}`, so most fields default.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        schema = try c.decodeIfPresent(String.self, forKey: .schema) ?? ""
        status = try c.decode(Status.self, forKey: .status)
        headline = try c.decode(Line.self, forKey: .headline)
        reason = try c.decodeIfPresent(Line.self, forKey: .reason)
        refusal = try c.decodeIfPresent(String.self, forKey: .refusal)
        bundle = try c.decodeIfPresent(BundleInfo.self, forKey: .bundle)
        posters = try c.decodeIfPresent(PackInfo.self, forKey: .posters)
        perception = try c.decodeIfPresent(String.self, forKey: .perception)
        perceptionNote = try c.decodeIfPresent(Line.self, forKey: .perceptionNote)
        coverage = try c.decodeIfPresent(String.self, forKey: .coverage)
        coverageLine = try c.decodeIfPresent(Line.self, forKey: .coverageLine)
        coverageBanner = try c.decodeIfPresent(Line.self, forKey: .coverageBanner)
        detectionLongSide = try c.decodeIfPresent(Int.self, forKey: .detectionLongSide)
        detectionLine = try c.decodeIfPresent(Line.self, forKey: .detectionLine)
        provider = try c.decodeIfPresent(String.self, forKey: .provider)
        execution = try c.decodeIfPresent(String.self, forKey: .execution)
        deviceNote = try c.decodeIfPresent(Line.self, forKey: .deviceNote)
        classes = try c.decodeIfPresent(Line.self, forKey: .classes)
        framesLine = try c.decodeIfPresent(Line.self, forKey: .framesLine)
        facesLine = try c.decodeIfPresent(Line.self, forKey: .facesLine)
        chance = try c.decodeIfPresent(Line.self, forKey: .chance)
        warnings = try c.decodeIfPresent([Line].self, forKey: .warnings) ?? []
        disclosure = try c.decodeIfPresent([Line].self, forKey: .disclosure) ?? []
        counts = try c.decodeIfPresent(Counts.self, forKey: .counts) ?? Counts()
        tracks = try c.decodeIfPresent([TrackItem].self, forKey: .tracks) ?? []
        candidates = try c.decodeIfPresent([Candidate].self, forKey: .candidates) ?? []
        createdAt = try c.decodeIfPresent(String.self, forKey: .createdAt)
        expiresAt = try c.decodeIfPresent(String.self, forKey: .expiresAt)
    }

    /// Faces that are not on a candidate card, for the "Other faces" grid.
    public var otherFaces: [TrackItem] {
        let carded = Set(candidates.map(\.trackId))
        return tracks.filter { !carded.contains($0.trackId) }
    }
}

public struct Counts: Decodable, Equatable, Sendable {
    public var framesDecoded: UInt64 = 0
    public var framesAnalyzed: UInt64 = 0
    public var framesDeclared: UInt64?
    public var facesSeen: UInt64 = 0
    public var facesCompared: UInt64 = 0
    public var facesNotCompared: UInt64 = 0
    public var candidates: UInt64 = 0

    public init() {}

    enum CodingKeys: String, CodingKey {
        case framesDecoded = "frames_decoded"
        case framesAnalyzed = "frames_analyzed"
        case framesDeclared = "frames_declared"
        case facesSeen = "faces_seen"
        case facesCompared = "faces_compared"
        case facesNotCompared = "faces_not_compared"
        case candidates
    }
}

/// One face across the frames it was on screen.
public struct TrackItem: Decodable, Equatable, Sendable, Identifiable {
    public enum Status: String, Sendable {
        case candidate
        case belowCutoff = "below_cutoff"
        case tooBrief = "too_brief"
        case notCompared = "not_compared"
        case unscored
    }

    public var trackId: UInt64
    public var firstFrame: UInt64
    public var lastFrame: UInt64
    public var frames: Line
    public var facePx: Int
    public var qualityFrames: Int
    public var status: Status
    public var label: Line
    public var reason: Line?
    public var crop: String?

    public var id: UInt64 { trackId }

    enum CodingKeys: String, CodingKey {
        case trackId = "track_id"
        case firstFrame = "first_frame"
        case lastFrame = "last_frame"
        case frames
        case facePx = "face_px"
        case qualityFrames = "quality_frames"
        case status, label, reason, crop
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        trackId = try c.decode(UInt64.self, forKey: .trackId)
        firstFrame = try c.decode(UInt64.self, forKey: .firstFrame)
        lastFrame = try c.decode(UInt64.self, forKey: .lastFrame)
        frames = try c.decode(Line.self, forKey: .frames)
        facePx = try c.decode(Int.self, forKey: .facePx)
        qualityFrames = try c.decode(Int.self, forKey: .qualityFrames)
        // A status this shell does not know is shown as unscored, never as a candidate.
        status = Status(rawValue: try c.decode(String.self, forKey: .status)) ?? .unscored
        label = try c.decode(Line.self, forKey: .label)
        reason = try c.decodeIfPresent(Line.self, forKey: .reason)
        crop = try c.decodeIfPresent(String.self, forKey: .crop)
    }
}

public struct PosterRef: Decodable, Equatable, Sendable {
    public var id: String
    /// `missing` or `wanted`.
    public var posterClass: String
    public var classLabel: String
    public var title: String
    public var fbiUrl: String?
    /// An absolute path to the poster photo.
    public var image: String?

    public var isWanted: Bool { posterClass == "wanted" }

    enum CodingKeys: String, CodingKey {
        case id
        case posterClass = "class"
        case classLabel = "class_label"
        case title
        case fbiUrl = "fbi_url"
        case image
    }
}

public struct Candidate: Decodable, Equatable, Sendable, Identifiable {
    public var headline: Line
    public var trackId: UInt64
    public var frameIndex: UInt64
    public var frameLabel: Line
    public var band: Line
    public var poster: PosterRef
    public var crop: String?
    public var frame: String?
    public var leaving: Line
    public var leaveNotes: [Line]

    public var id: String { "\(trackId)-\(poster.id)" }

    enum CodingKeys: String, CodingKey {
        case headline
        case trackId = "track_id"
        case frameIndex = "frame_index"
        case frameLabel = "frame_label"
        case band, poster, crop, frame, leaving
        case leaveNotes = "leave_notes"
    }
}

public struct BundleInfo: Decodable, Equatable, Sendable {
    public var id: String
    public var name: String
    public var version: String
    public var measured: Bool
    public var weights: [WeightInfo]
}

public struct WeightInfo: Decodable, Equatable, Sendable {
    public var role: String
    public var name: String
    public var sha256: String
}

/// A poster pack's provenance.
public struct PackInfo: Decodable, Equatable, Sendable {
    public var id: String
    public var source: String
    public var sha256: String
    public var createdAt: String
    public var expiresAt: String
    public var missing: Int
    public var wanted: Int
    public var demo: Bool

    enum CodingKeys: String, CodingKey {
        case id, source, sha256
        case createdAt = "created_at"
        case expiresAt = "expires_at"
        case missing, wanted, demo
    }

    public var count: Int { missing + wanted }
    public var created: Date? { Timestamp.parse(createdAt) }
}

/// RFC 3339 times as the core writes them ("2026-10-19T03:41:33Z").
public enum Timestamp {
    public static func parse(_ text: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        if let date = formatter.date(from: text) {
            return date
        }
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter.date(from: text)
    }
}
