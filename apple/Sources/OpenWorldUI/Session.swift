// SPDX-License-Identifier: Apache-2.0
import Foundation
import OpenWorldContract

/// Talks to the Rust library when it is linked, and otherwise to the `openworld` program. This file does not detect or compare.
struct CoreFailure: LocalizedError {
    var message: String
    var errorDescription: String? { message }
}

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
        let data = try run(PhoneArguments.bundles(catalog: bundles))
        let decoded = try JSONDecoder().decode(BundleList.self, from: data)
        return decoded.bundles
    }

    func estimate(input: URL, bundle: String, longSide: String, coverage: String, phone: Bool) throws -> Estimate {
        let facts = try PlatformDecoder.facts(url: input)
        let data = try run(PhoneArguments.estimate(
            catalog: bundles,
            input: input.path,
            bundle: bundle,
            longSide: longSide,
            coverage: coverage,
            phone: phone,
            media: facts.media
        ))
        if let message = Self.refusalMessage(data) {
            throw CoreFailure(message: message)
        }
        return try JSONDecoder().decode(Estimate.self, from: data)
    }

    static func refusalMessage(_ data: Data) -> String? {
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              (object["status"] as? String) == "refused" else {
            return nil
        }
        if let message = object["message"] as? String, !message.isEmpty {
            return message
        }
        if let summary = object["summary"] as? String, !summary.isEmpty {
            return summary
        }
        return "Refusing."
    }

    func scan(input: URL, bundle: String, longSide: String, coverage: String, posters: URL, frames: URL, facts: PlatformDecoder.Facts, out: URL, phone: Bool) throws -> ScanReport {
        let data = try run(PhoneArguments.scan(
            catalog: bundles,
            input: input.path,
            bundle: bundle,
            longSide: longSide,
            coverage: coverage,
            posters: posters.path,
            frames: frames.path,
            out: out.path,
            phone: phone,
            media: facts.media
        ))
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

