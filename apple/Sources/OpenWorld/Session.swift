// SPDX-License-Identifier: Apache-2.0
import Foundation

/// Talks to the Rust library when it is linked, and otherwise to the `openworld` program. This file does not detect or compare.
struct CoreClient {
    var binary: String = CoreClient.findBinary()
    var bundles: String = CoreClient.findBundles()

    /// The built `openworld` program, or the name on PATH when this is a packaged app.
    static func findBinary() -> String {
        if let env = ProcessInfo.processInfo.environment["OPENWORLD_BIN"], !env.isEmpty {
            return env
        }
        var url = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        for _ in 0..<8 {
            for name in ["debug", "release"] {
                let candidate = url.appendingPathComponent("core/target/\(name)/openworld")
                if FileManager.default.isExecutableFile(atPath: candidate.path) {
                    return candidate.path
                }
            }
            if url.path == "/" { break }
            url.deleteLastPathComponent()
        }
        return "openworld"
    }

    /// Walk up from the working directory, the same way the CLI finds `bundles/`.
    static func findBundles() -> String {
        if let env = ProcessInfo.processInfo.environment["OPENWORLD_BUNDLES"], !env.isEmpty {
            return env
        }
        var url = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        for _ in 0..<8 {
            let manifest = url.appendingPathComponent("bundles/fast/manifest.toml")
            if FileManager.default.fileExists(atPath: manifest.path) {
                return url.appendingPathComponent("bundles").path
            }
            if url.path == "/" { break }
            url.deleteLastPathComponent()
        }
        return "bundles"
    }

    func bundlesJSON() throws -> [BundleRow] {
        let data = try run(["--json", "--bundles", bundles, "bundles"])
        let decoded = try JSONDecoder().decode(BundleList.self, from: data)
        return decoded.bundles
    }

    func estimate(input: URL, bundle: String, longSide: String, coverage: String, phone: Bool) throws -> Estimate {
        let facts = try PlatformDecoder.facts(url: input)
        var args = [
            "--json", "--bundles", bundles, "estimate",
            "--input", input.path,
            "--bundle", bundle,
            "--long-side", longSide,
            "--coverage", coverage,
            "--form-factor", phone ? "phone" : "computer",
            "--provider", "cpu",
        ]
        args.append(contentsOf: facts.arguments(framesDirectory: nil))
        let data = try run(args)
        return try JSONDecoder().decode(Estimate.self, from: data)
    }

    func scan(input: URL, bundle: String, longSide: String, coverage: String, posters: URL, frames: URL, facts: PlatformDecoder.Facts, out: URL, phone: Bool) throws -> ScanReport {
        var args = [
            "--json", "--bundles", bundles, "scan",
            "--input", input.path,
            "--bundle", bundle,
            "--long-side", longSide,
            "--coverage", coverage,
            "--posters", posters.path,
            "--out", out.path,
            "--form-factor", phone ? "phone" : "computer",
            "--provider", "cpu",
        ]
        args.append(contentsOf: facts.arguments(framesDirectory: frames))
        let data = try run(args)
        return try JSONDecoder().decode(ScanReport.self, from: data)
    }

    func run(_ args: [String]) throws -> Data {
        if let linked = LinkedCore.invoke(args) {
            return linked
        }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: binary)
        process.arguments = args
        let pipe = Pipe()
        process.standardOutput = pipe
        try process.run()
        process.waitUntilExit()
        return pipe.fileHandleForReading.readDataToEndOfFile()
    }
}

struct BundleList: Decodable { var bundles: [BundleRow] }

struct BundleRow: Decodable, Identifiable {
    var id: String
    var name: String
    var bestFor: String
    var curveLine: String
    var preselected: Bool
    enum CodingKeys: String, CodingKey {
        case id, name, preselected
        case bestFor = "best_for"
        case curveLine = "curve_line"
    }
}

struct Estimate: Decodable {
    var human: String
    var caveat: String
    var deviceNote: String?
    var heatNote: String?
    var batteryNote: String?
    var suggestComputerText: String?
    enum CodingKeys: String, CodingKey {
        case human, caveat
        case deviceNote = "device_note"
        case heatNote = "heat_note"
        case batteryNote = "battery_note"
        case suggestComputerText = "suggest_computer_text"
    }
}

struct ScanReport: Decodable {
    var status: String
    var summary: String
    var bundleName: String?
    var disclosure: [String]
    var coverageBanner: String?
    var perceptionNote: String?
    var warnings: [String]
    var inventory: [InventoryItem]
    var candidates: [Candidate]
    enum CodingKeys: String, CodingKey {
        case status, summary, disclosure, warnings, inventory, candidates
        case bundleName = "bundle_name"
        case coverageBanner = "coverage_banner"
        case perceptionNote = "perception_note"
    }
}

struct InventoryItem: Decodable, Identifiable {
    var id: String { "\(kind)-\(frameIndex)-\(label)" }
    var kind: String
    var label: String
    var crop: String?
    var frameIndex: Int
    enum CodingKeys: String, CodingKey {
        case kind, label, crop
        case frameIndex = "frame_index"
    }
}

struct Candidate: Decodable, Identifiable {
    var id: String { "\(posterId)-\(frameIndex)-\(kind)" }
    var wording: String
    var kind: String
    var uncertainty: String
    var posterTitle: String
    var posterClass: String
    var fbiUrl: String
    var crop: String?
    var frame: String?
    var leaving: String
    var frameIndex: Int
    var posterId: String
    enum CodingKeys: String, CodingKey {
        case wording, kind, uncertainty, crop, frame, leaving
        case posterTitle = "poster_title"
        case posterClass = "poster_class"
        case fbiUrl = "fbi_url"
        case frameIndex = "frame_index"
        case posterId = "poster_id"
    }
}
