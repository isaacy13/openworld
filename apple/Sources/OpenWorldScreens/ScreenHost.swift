// SPDX-License-Identifier: Apache-2.0
@_spi(StdoutRenderer) import OpenSwiftUI
import Foundation
import OpenWorldContract
import OpenWorldUI

/// Linux preview of the phone screens. Not the Mac or iPhone app.
/// OpenSwiftUI on this revision paints one stdout frame and exits.
@main
struct ScreenHost: App {
    private let lines: [String]

    init() {
        lines = ScreenCopy.make()
        print("OpenSwiftUI phone screen")
        for line in lines {
            print(line)
        }
        print("")
    }

    static var rendererConfiguration: _RendererConfiguration? {
        var options = _RendererConfiguration.StdoutOptions()
        options.viewMode = .terminal
        options.colorMode = .trueColor
        options.surface.width = 80
        options.surface.height = 24
        options.terminalSize = .init(columns: 80, rows: 24)
        return .stdout(options)
    }

    var body: some Scene {
        WindowGroup {
            PhoneFrame()
        }
    }
}

enum ScreenCopy {
    static func make() -> [String] {
        MainActor.assumeIsolated { build() }
    }

    @MainActor
    private static func build() -> [String] {
        let screen = ProcessInfo.processInfo.environment["OPENWORLD_SCREEN"] ?? "choose"
        let model = FlowModel(phone: true)
        prepare(model, screen: screen)
        let wrapped = words(screen, model).flatMap { wrap($0, width: 34) }
        return Array(wrapped.prefix(12))
    }

    @MainActor
    private static func prepare(_ model: FlowModel, screen: String) {
        guard screen != "choose" else { return }
        guard let url = still() else {
            model.error = "The scan program is not on this device. Refusing."
            return
        }
        if screen == "device" {
            let old = Date().addingTimeInterval(-40 * 24 * 3600)
            try? FileManager.default.setAttributes([.modificationDate: old], ofItemAtPath: url.path)
        }
        model.choose(url)
        guard screen != "device" else { return }
        model.loadBundles()
        model.longSide = "640"
        if screen == "size" {
            model.coverage = "measured"
            model.step = .size
            return
        }
        model.coverage = "complete"
        guard screen != "bundle" else { return }
        model.loadEstimate()
        guard screen == "results" || screen == "leaving" else { return }
        model.analyze()
        if screen == "leaving", let page = model.report?.candidates.first?.fbiUrl {
            model.requestLeave(page)
        }
    }

    @MainActor
    private static func words(_ screen: String, _ model: FlowModel) -> [String] {
        switch screen {
        case "device":
            var lines = [Copy.onDevice, model.file?.lastPathComponent ?? ""]
            if model.oldFile { lines.append(Copy.oldFile) }
            lines.append(contentsOf: Copy.disclosure)
            lines.append("Fixture posters. Real FBI photos stay off.")
            return lines
        case "bundle":
            var lines = ["Model bundle", "Scores are not comparable across bundles."]
            for row in model.bundles {
                lines.append(row.name)
                lines.append(row.bestFor)
                lines.append(row.curveLine)
            }
            if model.bundles.isEmpty, let error = model.error {
                lines.append(error)
            }
            return lines
        case "size":
            return [
                "Detection size",
                "640 px on the long side",
                "Complete. Every decoded frame.",
                "Measured. 5 frames a second, plus the tracker.",
                Copy.brief,
            ]
        case "estimate":
            var lines = ["Estimate"]
            if let estimate = model.estimate {
                lines.append(estimate.human)
                lines.append(estimate.caveat)
                if let note = estimate.deviceNote { lines.append(note) }
                if let note = estimate.heatNote { lines.append(note) }
                if let note = estimate.batteryNote { lines.append(note) }
            } else if let error = model.error {
                lines.append(error)
            }
            return lines
        case "results", "leaving":
            var lines = [model.report?.summary ?? model.error ?? Copy.incomplete, "Choose another file", "Back"]
            if model.report?.status == "incomplete",
               let reason = model.report?.message,
               !reason.isEmpty,
               reason != model.report?.summary {
                lines.append(reason)
            }
            if let banner = model.report?.coverageBanner { lines.append(banner) }
            if let name = model.report?.bundleName { lines.append("Bundle: \(name)") }
            if let note = model.report?.perceptionNote { lines.append(note) }
            lines.append(contentsOf: model.report?.warnings ?? [])
            for item in model.report?.inventory ?? [] where !lines.contains(item.label) {
                lines.append(item.label)
            }
            if model.resultDirectory != nil {
                lines.append("Delete")
            }
            if model.report?.candidates.isEmpty == false {
                if let candidate = model.report?.candidates.first {
                    lines.append(candidate.posterTitle + " (" + candidate.posterClass + ")")
                }
                lines.append("Open FBI page")
            } else if model.report?.status == "complete" {
                lines.append(Copy.clearance)
            }
            if screen == "leaving" {
                lines.append(Copy.leaving)
                if let page = model.leavingURL?.absoluteString { lines.append(page) }
                lines.append("Stay")
            }
            return lines
        default:
            return [
                "Choose a photo or video",
                "Import a file you already have. There is no camera.",
                "Choose File",
            ]
        }
    }

    private static func wrap(_ text: String, width: Int) -> [String] {
        if text.isEmpty { return [""] }
        var lines: [String] = []
        var line = ""
        for word in text.split(separator: " ") {
            let word = String(word)
            if line.isEmpty {
                line = word
            } else if line.count + 1 + word.count <= width {
                line += " " + word
            } else {
                lines.append(line)
                line = word
            }
        }
        if !line.isEmpty { lines.append(line) }
        return lines
    }

    private static func still() -> URL? {
        let out = FileManager.default.temporaryDirectory.appendingPathComponent("ow-screen-\(UUID().uuidString).png")
        let process = Process()
        process.executableURL = URL(fileURLWithPath: binary())
        process.arguments = ["--bundles", bundles(), "fixture-still", "--out", out.path, "--scene"]
        process.standardOutput = Pipe()
        process.standardError = Pipe()
        guard (try? process.run()) != nil else { return nil }
        process.waitUntilExit()
        guard process.terminationStatus == 0, FileManager.default.fileExists(atPath: out.path) else { return nil }
        return out
    }

    private static func binary() -> String {
        if let env = ProcessInfo.processInfo.environment["OPENWORLD_BIN"], !env.isEmpty {
            return env
        }
        return walked("core/target/debug/openworld") ?? walked("core/target/release/openworld") ?? "openworld"
    }

    private static func bundles() -> String {
        if let env = ProcessInfo.processInfo.environment["OPENWORLD_BUNDLES"], !env.isEmpty {
            return env
        }
        var url = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        for _ in 0..<8 {
            if FileManager.default.fileExists(atPath: url.appendingPathComponent("bundles/fast/manifest.toml").path) {
                return url.appendingPathComponent("bundles").path
            }
            if url.path == "/" { break }
            url.deleteLastPathComponent()
        }
        return "bundles"
    }

    private static func walked(_ relative: String) -> String? {
        var url = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        for _ in 0..<8 {
            let candidate = url.appendingPathComponent(relative)
            if FileManager.default.isExecutableFile(atPath: candidate.path) {
                return candidate.path
            }
            if url.path == "/" { break }
            url.deleteLastPathComponent()
        }
        return nil
    }
}
