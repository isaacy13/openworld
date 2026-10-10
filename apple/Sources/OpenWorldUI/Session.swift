// SPDX-License-Identifier: Apache-2.0
#if canImport(Darwin)
import Darwin
#elseif canImport(Glibc)
import Glibc
#endif
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
        if let env = environmentValue("OPENWORLD_BUNDLES"), !env.isEmpty {
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

    /// `ProcessInfo.environment` keeps the values from process start.
    static func environmentValue(_ name: String) -> String? {
        guard let raw = getenv(name) else { return nil }
        return String(cString: raw)
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

    func scan(input: URL, bundle: String, longSide: String, coverage: String, posters: URL, frames: URL, facts: PlatformDecoder.Facts, out: URL, phone: Bool, missing: Bool = true, wanted: Bool = true, onProgress: ((String) -> Void)? = nil) throws -> ScanReport {
        var args = PhoneArguments.scan(
            catalog: bundles,
            input: input.path,
            bundle: bundle,
            longSide: longSide,
            coverage: coverage,
            posters: posters.path,
            frames: frames.path,
            out: out.path,
            phone: phone,
            media: facts.media,
            missing: missing,
            wanted: wanted
        )
        if onProgress != nil {
            args.append("--progress")
        }
        let data = try run(args, onProgress: onProgress)
        return try JSONDecoder().decode(ScanReport.self, from: data)
    }

    func run(_ args: [String], onProgress: ((String) -> Void)? = nil) throws -> Data {
        if let linked = LinkedCore.invoke(args, onProgress: onProgress) {
            return linked
        }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: binary)
        process.arguments = args
        let pipe = Pipe()
        process.standardOutput = pipe
        let errors = Pipe()
        if onProgress != nil {
            process.standardError = errors
        }
        try process.run()
        if let onProgress {
            let reader = StderrLines(onProgress)
            let done = DispatchSemaphore(value: 0)
            Thread {
                let handle = errors.fileHandleForReading
                while true {
                    let chunk = handle.availableData
                    if chunk.isEmpty { break }
                    reader.append(chunk)
                }
                reader.finish()
                done.signal()
            }.start()
            let stdout = pipe.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()
            done.wait()
            return stdout
        }
        process.waitUntilExit()
        return pipe.fileHandleForReading.readDataToEndOfFile()
    }
}

/// Progress lines from the program's stderr. One thread reads it.
private final class StderrLines: @unchecked Sendable {
    private var pending = Data()
    private let onLine: (String) -> Void

    init(_ onLine: @escaping (String) -> Void) {
        self.onLine = onLine
    }

    func append(_ data: Data) {
        pending.append(data)
        while let newline = pending.firstIndex(of: 10) {
            let chunk = Data(pending.prefix(upTo: newline))
            pending.removeSubrange(pending.startIndex...newline)
            emit(chunk)
        }
    }

    func finish() {
        emit(pending)
        pending.removeAll()
    }

    private func emit(_ data: Data) {
        guard let text = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines),
              !text.isEmpty else {
            return
        }
        onLine(text)
    }
}

