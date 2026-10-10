// SPDX-License-Identifier: Apache-2.0
import Foundation

/// Arguments shared by the iPhone and Mac shells. This file does not detect or compare.
public struct MediaArguments: Equatable {
    public var width: Int
    public var height: Int
    public var fps: Double
    public var frames: Int
    public var duration: Double
    public var video: Bool
    public var containerUnix: Int?

    public init(width: Int, height: Int, fps: Double, frames: Int, duration: Double, video: Bool, containerUnix: Int?) {
        self.width = width
        self.height = height
        self.fps = fps
        self.frames = frames
        self.duration = duration
        self.video = video
        self.containerUnix = containerUnix
    }

    public func arguments(framesDirectory: String?) -> [String] {
        var args = [
            "--width", String(width),
            "--height", String(height),
            "--fps", String(fps),
            "--frame-count", String(frames),
            "--duration", String(duration),
        ]
        if video {
            args.append("--video")
        }
        if let containerUnix {
            args.append(contentsOf: ["--container-unix", String(containerUnix)])
        }
        if let framesDirectory {
            args.append(contentsOf: ["--frames", framesDirectory])
        }
        return args
    }
}

public enum PhoneArguments {
    public static func bundles(catalog: String) -> [String] {
        ["--json", "--bundles", catalog, "bundles"]
    }

    public static func estimate(catalog: String, input: String, bundle: String, longSide: String, coverage: String, phone: Bool, media: MediaArguments) -> [String] {
        var args = [
            "--json", "--bundles", catalog, "estimate",
            "--input", input,
            "--bundle", bundle,
            "--long-side", longSide,
            "--coverage", coverage,
            "--form-factor", phone ? "phone" : "computer",
            "--provider", "cpu",
        ]
        args.append(contentsOf: media.arguments(framesDirectory: nil))
        return args
    }

    public static func scan(catalog: String, input: String, bundle: String, longSide: String, coverage: String, posters: String, frames: String, out: String, phone: Bool, media: MediaArguments, missing: Bool = true, wanted: Bool = true) -> [String] {
        var args = [
            "--json", "--bundles", catalog, "scan",
            "--input", input,
            "--bundle", bundle,
            "--long-side", longSide,
            "--coverage", coverage,
            "--posters", posters,
            "--out", out,
            "--form-factor", phone ? "phone" : "computer",
            "--provider", "cpu",
        ]
        if !missing { args.append("--no-missing") }
        if !wanted { args.append("--no-wanted") }
        args.append(contentsOf: media.arguments(framesDirectory: frames))
        return args
    }

    public static func writeFixture(out: String) -> [String] {
        ["posters", "write-fixture", "--out", out]
    }

    public static func leave(url: String) -> [String] {
        ["--json", "leave", "--url", url]
    }

    public static func delete(out: String) -> [String] {
        ["--json", "delete", "--out", out]
    }
}

public struct BundleList: Decodable {
    public var bundles: [BundleRow]
}

public struct BundleRow: Decodable, Identifiable {
    public var id: String
    public var name: String
    public var bestFor: String
    public var curveLine: String
    public var preselected: Bool
    enum CodingKeys: String, CodingKey {
        case id, name, preselected
        case bestFor = "best_for"
        case curveLine = "curve_line"
    }
}

public struct Estimate: Decodable {
    public var human: String
    public var caveat: String
    public var deviceNote: String?
    public var heatNote: String?
    public var batteryNote: String?
    public var suggestComputerText: String?
    enum CodingKeys: String, CodingKey {
        case human, caveat
        case deviceNote = "device_note"
        case heatNote = "heat_note"
        case batteryNote = "battery_note"
        case suggestComputerText = "suggest_computer_text"
    }
}

public struct ScanReport: Decodable {
    public var status: String
    public var summary: String
    public var message: String?
    public var bundleName: String?
    public var disclosure: [String]
    public var coverageBanner: String?
    public var classNote: String?
    public var perceptionNote: String?
    public var warnings: [String]
    public var inventory: [InventoryItem]
    public var candidates: [Candidate]
    public var comparisons: [Comparison]
    public var facesSeenNotCompared: Int
    enum CodingKeys: String, CodingKey {
        case status, summary, message, disclosure, warnings, inventory, candidates, comparisons
        case bundleName = "bundle_name"
        case coverageBanner = "coverage_banner"
        case classNote = "class_note"
        case perceptionNote = "perception_note"
        case facesSeenNotCompared = "faces_seen_not_compared"
    }
}

public struct InventoryItem: Decodable, Identifiable {
    public var id: String { "\(kind)-\(frameIndex)-\(label)" }
    public var kind: String
    public var label: String
    public var crop: String?
    public var frameIndex: Int
    enum CodingKeys: String, CodingKey {
        case kind, label, crop
        case frameIndex = "frame_index"
    }
}

public struct Candidate: Decodable, Identifiable {
    public var id: String { "\(posterId)-\(frameIndex)-\(kind)" }
    public var wording: String
    public var kind: String
    public var uncertainty: String
    public var posterTitle: String
    public var posterClass: String
    public var posterClassLabel: String
    public var posterLine: String {
        posterClassLabel.isEmpty ? posterTitle : "\(posterTitle) (\(posterClassLabel))"
    }
    public var fbiUrl: String
    public var crop: String?
    public var frame: String?
    public var leaving: String
    public var frameIndex: Int
    public var posterId: String
    enum CodingKeys: String, CodingKey {
        case wording, kind, uncertainty, crop, frame, leaving
        case posterTitle = "poster_title"
        case posterClass = "poster_class"
        case posterClassLabel = "poster_class_label"
        case fbiUrl = "fbi_url"
        case frameIndex = "frame_index"
        case posterId = "poster_id"
    }
}

public struct Comparison: Decodable {
    public var fiducialId: Int
    public var posterFiducialId: Int?
    public var passed: Bool
    enum CodingKeys: String, CodingKey {
        case passed
        case fiducialId = "fiducial_id"
        case posterFiducialId = "poster_fiducial_id"
    }
}
