// SPDX-License-Identifier: Apache-2.0
#if os(Linux)
import OpenSwiftUI
#else
import SwiftUI
#endif
import OpenWorldContract
#if os(macOS)
import AppKit
#elseif os(iOS)
import UIKit
#endif
import Foundation

/// The choice that will run, in the same words on every screen.
public func markedChoice(_ title: String, selected: Bool) -> String {
    guard selected else { return title }
    if title.hasSuffix(".") {
        return "\(title) Selected."
    }
    return "\(title). Selected."
}

/// The detection-size row, including the mark on the size that will run.
public func detectionChoice(_ value: String, selected: String) -> String {
    let title: String
    switch value {
    case "320": title = "320 px on the long side"
    case "480": title = "480 px on the long side"
    case "640": title = "640 px on the long side"
    case "full": title = "Full resolution"
    default: title = value
    }
    return markedChoice(title, selected: value == selected)
}

/// The coverage row, including the mark on the coverage that will run.
public func coverageChoice(_ value: String, selected: String) -> String {
    let title: String
    switch value {
    case "measured": title = "Measured. 5 frames a second, plus the tracker."
    case "complete": title = "Complete. Every decoded frame."
    default: title = value
    }
    return markedChoice(title, selected: value == selected)
}

public enum Step {
    case choose, device, bundle, size, estimate, results
}

public struct LiveCrop: Identifiable, Equatable {
    public let id: Int
    public let label: String
    public let frameLabel: String
    public let path: String
}

/// The first crop may wait so a test can read the estimate before the scan finishes.
private final class ProgressOnce: @unchecked Sendable {
    let seen: DispatchSemaphore?
    let hold: DispatchSemaphore?
    private let lock = NSLock()
    private var claimed = false

    init(seen: DispatchSemaphore?, hold: DispatchSemaphore?) {
        self.seen = seen
        self.hold = hold
    }

    func claim() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        if claimed { return false }
        claimed = true
        return true
    }
}

private struct ScanPaths: Sendable {
    var root: URL
    var posters: URL
    var frames: URL
    var result: URL
}

private struct FinishedScan {
    var report: ScanReport?
    var error: String?
    var keepRoot: Bool
    var root: URL
    var result: URL
}

@MainActor
public final class FlowModel: ObservableObject {
    @Published public var step: Step = .choose
    @Published public var file: URL?
    @Published public var bundles: [BundleRow] = []
    @Published public var bundleID = "fast"
    @Published public var longSide = "640"
    @Published public var coverage = "complete"
    @Published public var includeMissing = true
    @Published public var includeWanted = true
    @Published public var estimate: Estimate?
    @Published public var report: ScanReport?
    @Published public var resultDirectory: URL?
    @Published public var leavingURL: URL?
    @Published public var leaveError: String?
    @Published public var deleteNotice: String?
    @Published public var oldFile = false
    @Published public var error: String?
    @Published public var canAnalyze = true
    @Published public var scanning = false
    @Published public var liveCrops: [LiveCrop] = []
    /// Signaled on the main thread after the first crop is on the estimate.
    public var progressSeen: DispatchSemaphore?
    /// The scan thread waits here after the first crop so a test can read the estimate.
    public var progressHold: DispatchSemaphore?
    /// When set, the next scan writes the fixture pack here. A file at this path is refused.
    public var fixturePackDirectory: URL?
    let phone: Bool
    private let core = CoreClient()
    private var scanRoot: URL?
    /// The catalog sentence currently on the bundle page, so a later catalog can clear it.
    private var catalogNotice: String?

    public init(phone: Bool) {
        self.phone = phone
    }

    public func choose(_ url: URL) {
        #if !os(Linux)
        _ = url.startAccessingSecurityScopedResource()
        #endif
        var directory = ObjCBool(false)
        let exists = FileManager.default.fileExists(atPath: url.path, isDirectory: &directory)
        if !exists || directory.boolValue || !FileManager.default.isReadableFile(atPath: url.path) {
            error = "The file could not be read. Refusing."
            return
        }
        file = url
        error = nil
        if let values = try? url.resourceValues(forKeys: [.contentModificationDateKey]),
           let modified = values.contentModificationDate,
           Date().timeIntervalSince(modified) > 30 * 24 * 3600 {
            oldFile = true
        } else {
            oldFile = false
        }
        step = .device
    }

    public func loadBundles() {
        let previousCatalog = catalogNotice
        do {
            bundles = try core.bundlesJSON()
            if bundles.isEmpty {
                catalogNotice = "The scan program is not on this device. Refusing."
                error = catalogNotice
            } else if error == previousCatalog
                || error == "The scan program is not on this device. Refusing."
                || error == "The bundle catalog could not be read. Refusing." {
                error = nil
                catalogNotice = nil
            }
        } catch let failure as CoreFailure {
            bundles = []
            catalogNotice = failure.message
            error = failure.message
        } catch {
            bundles = []
            catalogNotice = "The scan program is not on this device. Refusing."
            self.error = catalogNotice
        }
        if !bundles.contains(where: { $0.id == bundleID }),
           let selected = bundles.first(where: { $0.preselected }) {
            bundleID = selected.id
        }
        step = .bundle
    }

    /// A refusal the current page does not already print. An empty catalog prints its own line.
    public var showsTopError: Bool {
        guard let error, !error.isEmpty else { return false }
        switch step {
        case .estimate, .results:
            return false
        case .bundle where bundles.isEmpty:
            return false
        default:
            return true
        }
    }

    public func loadEstimate() {
        guard let file else {
            estimate = nil
            canAnalyze = false
            error = "The file could not be read. Refusing."
            step = .estimate
            return
        }
        do {
            estimate = try core.estimate(input: file, bundle: bundleID, longSide: longSide, coverage: coverage, phone: phone)
            error = nil
            canAnalyze = includeMissing || includeWanted
        } catch {
            estimate = nil
            canAnalyze = false
            self.error = error.localizedDescription
        }
        step = .estimate
    }

    /// The bundle, size, and coverage in the same words as the choices above.
    public var choiceLine: String {
        let name = bundles.first(where: { $0.id == bundleID })?.name ?? bundleID
        let size = longSide == "full" ? "Full resolution" : "\(longSide) px on the long side"
        let cover = coverage == "measured" ? "5 frames a second, plus the tracker." : "Every decoded frame."
        return "\(name). \(size). \(cover)"
    }

    /// Missing and wanted start on. Analyze stays on the page and does not run when both are off.
    public var classLine: String {
        switch (includeMissing, includeWanted) {
        case (true, true): return "Missing and wanted."
        case (true, false): return "Missing."
        case (false, true): return "Wanted."
        default: return "Choose missing, wanted, or both."
        }
    }

    public func applyClassGate() {
        if estimate != nil {
            canAnalyze = includeMissing || includeWanted
        }
    }

    public func classArguments() -> [String] {
        var args: [String] = []
        if !includeMissing { args.append("--no-missing") }
        if !includeWanted { args.append("--no-wanted") }
        return args
    }

    /// The measured banner belongs on an estimate that can run.
    public var showBriefOnEstimate: Bool {
        estimate != nil && coverage == "measured"
    }

    /// A refusal has no estimate, so Analyze is not on that page. Both classes off keeps the button.
    public var showsAnalyze: Bool { estimate != nil }

    /// The file page and the estimate both warn. The result repeats it after the scan.
    public var estimateWarning: String? { oldFile ? Copy.oldFile : nil }

    /// Back stays off while a scan is running.
    public var backEnabled: Bool { !scanning }

    public func goBack() {
        guard !scanning else { return }
        leavingURL = nil
        leaveError = nil
        deleteNotice = nil
        switch step {
        case .choose:
            break
        case .device:
            step = .choose
        case .bundle:
            step = .device
        case .size:
            step = .bundle
        case .estimate:
            step = .size
        case .results:
            step = .estimate
        }
    }

    public func chooseAnother() {
        guard !scanning else { return }
        guard releaseResult() else { return }
        file = nil
        report = nil
        resultDirectory = nil
        leavingURL = nil
        leaveError = nil
        deleteNotice = nil
        oldFile = false
        error = nil
        estimate = nil
        canAnalyze = true
        coverage = "complete"
        longSide = "640"
        bundleID = "fast"
        includeMissing = true
        includeWanted = true
        step = .choose
    }

    public func analyze() {
        guard canAnalyze, let file else { return }
        guard releaseResult() else { return }
        let paths = makeScanPaths()
        apply(Self.finishedScan(core: core, file: file, bundleID: bundleID, longSide: longSide, coverage: coverage, phone: phone, missing: includeMissing, wanted: includeWanted, paths: paths))
    }

    /// Leaves the estimate page in place and says Scanning until the result is ready.
    public func startScan() {
        guard !scanning, canAnalyze, let file else { return }
        guard releaseResult() else { return }
        scanning = true
        liveCrops = []
        let paths = makeScanPaths()
        let core = core
        let bundleID = bundleID
        let longSide = longSide
        let coverage = coverage
        let phone = phone
        let missing = includeMissing
        let wanted = includeWanted
        let gate = ProgressOnce(seen: progressSeen, hold: progressHold)
        let result = paths.result
        Task.detached {
            let outcome = Self.finishedScan(
                core: core,
                file: file,
                bundleID: bundleID,
                longSide: longSide,
                coverage: coverage,
                phone: phone,
                missing: missing,
                wanted: wanted,
                paths: paths,
                onProgress: { line in
                    Self.deliverCrop(line: line, directory: result, gate: gate, model: self)
                }
            )
            await MainActor.run {
                self.apply(outcome)
                self.liveCrops = []
                self.scanning = false
            }
        }
    }

    func appendLiveCrop(label: String, frameLabel: String, path: String) {
        liveCrops.append(LiveCrop(id: liveCrops.count, label: label, frameLabel: frameLabel, path: path))
    }

    /// Hop the crop onto the main thread, then let only the first one wait for the test.
    nonisolated private static func deliverCrop(line: String, directory: URL, gate: ProgressOnce, model: FlowModel) {
        guard let data = line.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return
        }
        let kind = object["kind"] as? String ?? ""
        guard kind == "face" || kind == "plate" || kind == "vehicle",
              let label = object["label"] as? String, !label.isEmpty,
              let relative = object["crop"] as? String, !relative.isEmpty else {
            return
        }
        let path = directory.appendingPathComponent(relative).path
        guard FileManager.default.fileExists(atPath: path) else { return }
        let frameLabel = (object["frame_label"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? "Frame"
        if Thread.isMainThread {
            MainActor.assumeIsolated {
                model.appendLiveCrop(label: label, frameLabel: frameLabel, path: path)
            }
            return
        }
        let first = gate.claim()
        let posted = DispatchSemaphore(value: 0)
        DispatchQueue.main.async {
            MainActor.assumeIsolated {
                model.appendLiveCrop(label: label, frameLabel: frameLabel, path: path)
            }
            if first {
                gate.seen?.signal()
            }
            posted.signal()
        }
        posted.wait()
        if first {
            _ = gate.hold?.wait(timeout: .now() + 30)
        }
    }

    private func makeScanPaths() -> ScanPaths {
        leavingURL = nil
        leaveError = nil
        deleteNotice = nil
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        scanRoot = root
        return ScanPaths(
            root: root,
            posters: fixturePackDirectory ?? root.appendingPathComponent("posters"),
            frames: root.appendingPathComponent("frames"),
            result: root.appendingPathComponent("result")
        )
    }

    private func apply(_ outcome: FinishedScan) {
        if outcome.keepRoot, let report = outcome.report {
            self.report = report
            resultDirectory = outcome.result
            error = nil
        } else {
            report = nil
            resultDirectory = nil
            try? FileManager.default.removeItem(at: outcome.root)
            scanRoot = nil
            error = outcome.error
        }
        step = .results
    }

    nonisolated private static func finishedScan(
        core: CoreClient,
        file: URL,
        bundleID: String,
        longSide: String,
        coverage: String,
        phone: Bool,
        missing: Bool,
        wanted: Bool,
        paths: ScanPaths,
        onProgress: ((String) -> Void)? = nil
    ) -> FinishedScan {
        do {
            // The fixture pack is written by the CLI before a real curve allows FBI photos.
            try CoreClient().runPublic(posters: paths.posters)
            let reel = try PlatformDecoder.writeFrames(url: file, directory: paths.frames)
            let report = try core.scan(
                input: file,
                bundle: bundleID,
                longSide: longSide,
                coverage: coverage,
                posters: paths.posters,
                frames: reel.directory ?? paths.frames,
                facts: reel,
                out: paths.result,
                phone: phone,
                missing: missing,
                wanted: wanted,
                onProgress: onProgress
            )
            return FinishedScan(report: report, error: nil, keepRoot: true, root: paths.root, result: paths.result)
        } catch {
            return FinishedScan(report: nil, error: error.localizedDescription, keepRoot: false, root: paths.root, result: paths.result)
        }
    }

    /// Ask the library before any FBI page is shown. A lookalike host stays closed.
    public func requestLeave(_ urlString: String) {
        leavingURL = nil
        leaveError = nil
        let data: Data
        do {
            data = try core.run(PhoneArguments.leave(url: urlString))
        } catch {
            leaveError = error.localizedDescription
            return
        }
        let object = (try? JSONSerialization.jsonObject(with: data) as? [String: Any]) ?? [:]
        let message = object["message"] as? String
        let allowed = object["url"] as? String
        if message == Copy.leaving, let allowed, !allowed.isEmpty, let url = URL(string: allowed) {
            leavingURL = url
        } else if let message, !message.isEmpty {
            leaveError = message
        } else {
            leaveError = "OpenWorld only opens an FBI page."
        }
    }

    /// Leave a folder that is not a result on disk so Choose another file and Analyze can continue.
    private func releaseResult() -> Bool {
        guard let resultDirectory else { return true }
        let marker = resultDirectory.appendingPathComponent("result.json")
        if FileManager.default.fileExists(atPath: resultDirectory.path),
           !FileManager.default.fileExists(atPath: marker.path) {
            self.resultDirectory = nil
            self.scanRoot = nil
            deleteNotice = nil
            return true
        }
        return removeResult()
    }

    /// Remove the result directory through the library, then the temporary frames beside it.
    public func deleteResult() {
        guard removeResult() else { return }
        report = nil
        leavingURL = nil
        leaveError = nil
        error = "Deleted."
    }

    private func removeResult() -> Bool {
        guard let resultDirectory else { return true }
        deleteNotice = nil
        if !FileManager.default.fileExists(atPath: resultDirectory.path) {
            self.resultDirectory = nil
            if let scanRoot { try? FileManager.default.removeItem(at: scanRoot) }
            scanRoot = nil
            return true
        }
        let data: Data
        do {
            data = try core.run(PhoneArguments.delete(out: resultDirectory.path))
        } catch {
            deleteNotice = error.localizedDescription
            return false
        }
        let object = (try? JSONSerialization.jsonObject(with: data) as? [String: Any]) ?? [:]
        guard (object["deleted"] as? Bool) == true else {
            let message = object["message"] as? String
            deleteNotice = (message?.isEmpty == false ? message : nil) ?? "The result could not be deleted."
            return false
        }
        if let scanRoot { try? FileManager.default.removeItem(at: scanRoot) }
        self.scanRoot = nil
        self.resultDirectory = nil
        return true
    }
}

extension CoreClient {
    func runPublic(posters: URL) throws {
        let data = try run(PhoneArguments.writeFixture(out: posters.path))
        if let message = Self.refusalMessage(data) {
            throw CoreFailure(message: message)
        }
        let object = (try? JSONSerialization.jsonObject(with: data) as? [String: Any]) ?? [:]
        let id = object["id"] as? String
        if id?.isEmpty != false {
            throw CoreFailure(message: "The poster pack could not be read. Refusing.")
        }
    }
}

public struct FlowView: View {
    @ObservedObject var model: FlowModel
    var importControl: AnyView
    @Environment(\.openURL) private var openURL

    public init(model: FlowModel, importControl: AnyView) {
        self.model = model
        self.importControl = importControl
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if model.showsTopError, let error = model.error {
                Text(error)
                    .foregroundStyle(.orange)
                    .padding(.horizontal)
                    .padding(.top, 8)
            }
            Group {
            switch model.step {
            case .choose:
                choose
            case .device:
                device
            case .bundle:
                bundles
            case .size:
                size
            case .estimate:
                estimate
            case .results:
                results
            }
            }
        }
    }

    private var choose: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Choose a photo or video")
                .font(model.phone ? .largeTitle : .title)
            Text("Import a file you already have. There is no camera.")
                .foregroundStyle(.secondary)
            importControl
            Spacer()
        }
        .padding()
    }

    private var device: some View {
        #if os(Linux)
        deviceColumn
        #else
        ScrollView { deviceColumn }
        #endif
    }

    private var deviceColumn: some View {
        VStack(alignment: .leading, spacing: 10) {
            backControl
            Text(Copy.onDevice).font(model.phone ? .largeTitle : .title)
            if let file = model.file {
                Text(file.lastPathComponent).font(.headline)
            }
            if model.oldFile {
                Text(Copy.oldFile).foregroundStyle(.orange)
            }
            ForEach(Copy.disclosure, id: \.self) { line in
                Text(line)
            }
            Text("Fixture posters. Real FBI photos stay off.")
                .foregroundStyle(.secondary)
            prominent("Continue") { model.loadBundles() }
        }
        .padding()
    }

    private var bundles: some View {
        #if os(Linux)
        bundleColumn
        #else
        ScrollView { bundleColumn }
        #endif
    }

    private var bundleColumn: some View {
        VStack(alignment: .leading, spacing: 8) {
            backControl
            Text("Model bundle").font(model.phone ? .largeTitle : .title)
            Text("Scores are not comparable across bundles. Results name the bundle you pick.")
                .foregroundStyle(.secondary)
            ForEach(model.bundles) { row in
                Button(action: { model.bundleID = row.id }) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(markedChoice(row.name, selected: model.bundleID == row.id)).font(.headline)
                        Text(row.bestFor)
                        Text(row.curveLine).foregroundStyle(.secondary)
                    }
                }
            }
            if model.bundles.isEmpty, let error = model.error {
                Text(error)
            }
            prominent("Continue") { model.step = .size }
        }
        .padding(model.phone ? 0 : 8)
    }

    private var size: some View {
        #if os(Linux)
        sizeColumn
        #else
        ScrollView { sizeColumn }
        #endif
    }

    private var sizeColumn: some View {
        VStack(alignment: .leading, spacing: 12) {
            backControl
            Text("Detection size").font(model.phone ? .largeTitle : .title)
            Text(Copy.sizeHint)
                .foregroundStyle(.secondary)
            sizePicker
            coveragePicker
            if model.coverage == "measured" {
                Text(Copy.brief)
            }
            prominent("Continue") { model.loadEstimate() }
        }
        .padding()
    }

    private var estimate: some View {
        #if os(Linux)
        estimateColumn
        #else
        ScrollView { estimateColumn }
        #endif
    }

    private var estimateColumn: some View {
        VStack(alignment: .leading, spacing: 10) {
            backControl
            Text(model.scanning ? "Scanning" : "Estimate").font(model.phone ? .largeTitle : .title)
            if model.scanning, !model.liveCrops.isEmpty {
                Text("Crops from this file.")
                    .font(model.phone ? .title3 : .headline)
                ForEach(model.liveCrops) { crop in
                    VStack(spacing: 4) {
                        liveImage(crop.path)
                        Text(crop.frameLabel)
                        Text(crop.label)
                    }
                }
            }
            if let warning = model.estimateWarning {
                Text(warning).foregroundStyle(.orange)
            }
            if let estimate = model.estimate {
                Text(estimate.human).font(.headline)
                Text(estimate.caveat).foregroundStyle(.secondary)
                if let note = estimate.deviceNote { Text(note) }
                if let note = estimate.heatNote { Text(note) }
                if let note = estimate.batteryNote { Text(note) }
                if let note = estimate.suggestComputerText { Text(note) }
                Text(model.choiceLine)
                if model.showBriefOnEstimate {
                    Text(Copy.brief)
                }
                Text(model.classLine)
                classToggle(on: model.includeMissing, title: "Missing") {
                    model.includeMissing.toggle()
                    model.applyClassGate()
                }
                classToggle(on: model.includeWanted, title: "Wanted") {
                    model.includeWanted.toggle()
                    model.applyClassGate()
                }
            } else if let error = model.error {
                Text(error)
            }
            if model.showsAnalyze {
                prominent(model.scanning ? "Scanning" : "Analyze") { model.startScan() }
                    .disabled(model.scanning || !model.canAnalyze)
            }
            if let notice = model.deleteNotice {
                Text(notice).foregroundStyle(.orange)
            }
            Spacer()
        }
        .padding()
    }

    @ViewBuilder
    private var results: some View {
        #if os(Linux)
        resultsColumn
        if model.leavingURL != nil {
            VStack(alignment: .leading, spacing: 8) {
                Text(Copy.leaving)
                Text(model.leavingURL?.absoluteString ?? "")
                prominent("Open") {
                    if let url = model.leavingURL { openURL(url) }
                    model.leavingURL = nil
                }
                prominent("Stay") { model.leavingURL = nil }
            }
        }
        #else
        ScrollView {
            resultsColumn
        }
        .confirmationDialog(Copy.leaving, isPresented: leavingPresented, titleVisibility: .visible) {
            Button("Open") {
                if let url = model.leavingURL { openURL(url) }
            }
            Button("Stay", role: .cancel) { model.leavingURL = nil }
        } message: {
            Text(model.leavingURL?.absoluteString ?? "")
        }
        #endif
    }

    private var resultsColumn: some View {
        VStack(alignment: .leading, spacing: 12) {
                backControl
                Text(model.report?.summary ?? model.error ?? Copy.incomplete)
                    .font(model.phone ? .largeTitle : .title)
                if model.report?.status == "incomplete",
                   let reason = model.report?.message,
                   !reason.isEmpty,
                   reason != model.report?.summary {
                    Text(reason)
                }
                ForEach(PhonePreview.contextLines(model.report), id: \.self) { line in
                    Text(line)
                }
                ForEach(model.report?.warnings ?? [], id: \.self) { line in
                    Text(line).foregroundStyle(.orange)
                }
                if model.report?.candidates.isEmpty == false {
                    ForEach(model.report?.candidates ?? []) { candidate in
                        VStack(alignment: .leading, spacing: 6) {
                            Text(candidate.wording).font(.headline)
                            HStack(alignment: .top, spacing: 8) {
                                labeledCrop(candidate.crop, "Crop")
                                labeledCrop(candidate.frame, candidate.frameLabel)
                            }
                            Text(candidate.uncertainty)
                            if !candidate.posterLine.isEmpty {
                                Text(candidate.posterLine)
                            }
                            if !candidate.fbiUrl.isEmpty {
                                prominent("Open FBI page") {
                                    model.requestLeave(candidate.fbiUrl)
                                }
                            }
                        }
                        #if !os(Linux)
                        .padding()
                        .background(.quaternary.opacity(0.4))
                        .clipShape(RoundedRectangle(cornerRadius: model.phone ? 12 : 8))
                        #endif
                    }
                }
                if let items = model.report?.inventory, !items.isEmpty {
                    Text("Crops from this file.")
                        .font(model.phone ? .title3 : .headline)
                    #if os(Linux)
                    HStack(alignment: .top, spacing: 12) {
                        ForEach(items) { item in
                            VStack(spacing: 4) {
                                cropImage(item.crop)
                                Text(item.frameLabel)
                                Text(item.label)
                            }
                        }
                    }
                    #else
                    ScrollView(.horizontal) {
                        HStack(alignment: .top, spacing: 12) {
                            ForEach(items) { item in
                                VStack(spacing: 4) {
                                    cropImage(item.crop)
                                    Text(item.frameLabel)
                                        .font(.caption)
                                    Text(item.label)
                                        .font(.caption)
                                        .multilineTextAlignment(.center)
                                        .frame(width: 140)
                                }
                            }
                        }
                    }
                    #endif
                }
                ForEach(PhonePreview.disclosureLines(model), id: \.self) { line in
                    Text(line).font(.footnote)
                }
                if let notice = model.leaveError {
                    Text(notice).foregroundStyle(.orange)
                }
                if model.resultDirectory != nil {
                    prominent("Delete") { model.deleteResult() }
                }
                if let notice = model.deleteNotice {
                    Text(notice).foregroundStyle(.orange)
                }
                prominent("Choose another file") { model.chooseAnother() }
            }
            .padding()
    }


    @ViewBuilder
    private func labeledCrop(_ relative: String?, _ caption: String) -> some View {
        if let relative, !relative.isEmpty {
            VStack(spacing: 4) {
                cropImage(relative)
                Text(caption).font(.caption)
            }
        }
    }

    @ViewBuilder
    private func liveImage(_ path: String) -> some View {
        #if os(macOS)
        if let image = NSImage(contentsOfFile: path) {
            Image(nsImage: image)
                .resizable()
                .interpolation(.none)
                .frame(width: 112, height: 112)
        }
        #elseif os(iOS)
        if let image = UIImage(contentsOfFile: path) {
            Image(uiImage: image)
                .resizable()
                .interpolation(.none)
                .frame(width: 112, height: 112)
        }
        #else
        Color.clear.frame(width: 112, height: 112)
        #endif
    }

    @ViewBuilder
    private func cropImage(_ relative: String?) -> some View {
        let path = relative.flatMap { model.resultDirectory?.appendingPathComponent($0).path }
        #if os(macOS)
        if let path, let image = NSImage(contentsOfFile: path) {
            Image(nsImage: image)
                .resizable()
                .interpolation(.none)
                .frame(width: 112, height: 112)
        }
        #elseif os(iOS)
        if let path, let image = UIImage(contentsOfFile: path) {
            Image(uiImage: image)
                .resizable()
                .interpolation(.none)
                .frame(width: 112, height: 112)
        }
        #else
        Color.clear.frame(width: 112, height: 112)
        #endif
    }

    @ViewBuilder private var sizePicker: some View {
        #if os(Linux)
        VStack(alignment: .leading, spacing: 8) {
            sizeChoice("320")
            sizeChoice("480")
            sizeChoice("640")
            sizeChoice("full")
        }
        #elseif os(macOS)
        let picker = Picker("Detection size", selection: $model.longSide) {
            Text(detectionChoice("320", selected: model.longSide)).tag("320")
            Text(detectionChoice("480", selected: model.longSide)).tag("480")
            Text(detectionChoice("640", selected: model.longSide)).tag("640")
            Text(detectionChoice("full", selected: model.longSide)).tag("full")
        }
        if model.phone {
            picker.pickerStyle(.inline)
        } else {
            picker.pickerStyle(.radioGroup)
        }
        #else
        Picker("Detection size", selection: $model.longSide) {
            Text(detectionChoice("320", selected: model.longSide)).tag("320")
            Text(detectionChoice("480", selected: model.longSide)).tag("480")
            Text(detectionChoice("640", selected: model.longSide)).tag("640")
            Text(detectionChoice("full", selected: model.longSide)).tag("full")
        }
        .pickerStyle(.inline)
        #endif
    }

    @ViewBuilder private var coveragePicker: some View {
        #if os(Linux)
        VStack(alignment: .leading, spacing: 8) {
            Text("Coverage").font(model.phone ? .title3 : .headline)
            Button(action: { model.coverage = "complete" }) {
                Text(coverageChoice("complete", selected: model.coverage))
            }
            Button(action: { model.coverage = "measured" }) {
                Text(coverageChoice("measured", selected: model.coverage))
            }
        }
        #elseif os(macOS)
        let picker = Picker("Coverage", selection: $model.coverage) {
            Text(coverageChoice("complete", selected: model.coverage)).tag("complete")
            Text(coverageChoice("measured", selected: model.coverage)).tag("measured")
        }
        if model.phone {
            picker.pickerStyle(.inline)
        } else {
            picker.pickerStyle(.radioGroup)
        }
        #else
        Picker("Coverage", selection: $model.coverage) {
            Text(coverageChoice("complete", selected: model.coverage)).tag("complete")
            Text(coverageChoice("measured", selected: model.coverage)).tag("measured")
        }
        .pickerStyle(.inline)
        #endif
    }

    #if os(Linux)
    @ViewBuilder
    private func sizeChoice(_ value: String) -> some View {
        Button(action: { model.longSide = value }) {
            Text(detectionChoice(value, selected: model.longSide))
        }
    }
    #endif

    @ViewBuilder
    private func classToggle(on: Bool, title: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(markedChoice(title, selected: on))
        }
        .disabled(model.scanning)
    }

    private var backControl: some View {
        Button(action: { model.goBack() }) { Text("Back") }
            .disabled(!model.backEnabled)
    }

    @ViewBuilder
    private func prominent(_ title: String, action: @escaping () -> Void) -> some View {
        #if os(Linux)
        Button(action: action) { Text(title) }
        #else
        Button(title, action: action).buttonStyle(.borderedProminent)
        #endif
    }

    private var leavingPresented: Binding<Bool> {
        Binding(get: { model.leavingURL != nil }, set: { if !$0 { model.leavingURL = nil } })
    }
}
