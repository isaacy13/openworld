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
        return PhonePreview.wrapped(screen: screen, model: model)
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
