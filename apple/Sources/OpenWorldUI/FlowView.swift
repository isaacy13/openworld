// SPDX-License-Identifier: Apache-2.0
#if canImport(SwiftUI)
import SwiftUI
#endif
import OpenWorldContract
#if os(macOS)
import AppKit
#elseif os(iOS)
import UIKit
#endif
import Foundation

#if !canImport(Combine)
/// The phone model on Linux stores these fields directly. Combine is an Apple framework.
public protocol ObservableObject: AnyObject {}

@propertyWrapper
public struct Published<Value> {
    public var wrappedValue: Value
    public init(wrappedValue: Value) {
        self.wrappedValue = wrappedValue
    }
}
#endif

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

/// Set when the window closes, so a decode that is still writing frames stops.
final class DecodeStop: @unchecked Sendable {
    private let lock = NSLock()
    private var stopped = false

    func stop() {
        lock.lock()
        stopped = true
        lock.unlock()
    }

    func isStopped() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return stopped
    }
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
    /// When set, the result folder is created here. A file at this path cannot hold that folder.
    public var scratchDirectory: URL?
    let phone: Bool
    /// Scan programs this window started. Another window has its own.
    let programs: RunningProgram
    private let core: CoreClient
    var scanRoot: URL?
    /// Set when the window closes, so a scan that finishes afterward does not return to the page.
    private var closed = false
    /// Readable off the main thread. Closing the window stops a decode that is still writing frames.
    nonisolated let stopFlag = DecodeStop()
    /// The catalog sentence currently on the bundle page, so a later catalog can clear it.
    private var catalogNotice: String?
    /// The estimate refusal, so a later unreadable file does not rename that headline.
    private var estimateNotice: String?
    /// The result sentence when there is no report, so a later unreadable file does not rename it.
    private var resultNotice: String?

    public init(phone: Bool) {
        self.phone = phone
        let programs = RunningProgram()
        self.programs = programs
        self.core = CoreClient(programs: programs)
    }

    public func choose(_ url: URL) {
        guard !closed else { return }
        #if !os(Linux)
        _ = url.startAccessingSecurityScopedResource()
        #endif
        guard !scanning else { return }
        var directory = ObjCBool(false)
        let exists = FileManager.default.fileExists(atPath: url.path, isDirectory: &directory)
        if !exists || directory.boolValue || !FileManager.default.isReadableFile(atPath: url.path) {
            error = "The file could not be read. Refusing."
            return
        }
        guard releaseResult() else {
            if error == "The file could not be read. Refusing." {
                error = nil
            }
            return
        }
        file = url
        report = nil
        error = nil
        estimateNotice = nil
        resultNotice = nil
        leavingURL = nil
        leaveError = nil
        deleteNotice = nil
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
        guard !closed else { return }
        let previousCatalog = catalogNotice
        let keptFile = error == "The file could not be read. Refusing."
        do {
            bundles = try core.bundlesJSON()
            if bundles.isEmpty {
                catalogNotice = "The scan program is not on this device. Refusing."
                if !keptFile {
                    error = catalogNotice
                }
            } else if error == previousCatalog
                || error == "The scan program is not on this device. Refusing."
                || error == "The bundle catalog could not be read. Refusing." {
                error = nil
                catalogNotice = nil
            }
        } catch let failure as CoreFailure {
            bundles = []
            catalogNotice = failure.message
            if !keptFile {
                error = failure.message
            }
        } catch {
            bundles = []
            catalogNotice = "The scan program is not on this device. Refusing."
            if !keptFile {
                self.error = catalogNotice
            }
        }
        if !bundles.contains(where: { $0.id == bundleID }),
           let selected = bundles.first(where: { $0.preselected }) {
            bundleID = selected.id
        }
        step = .bundle
    }

    /// The result headline. An empty summary falls through to the error, then Incomplete.
    /// A file that cannot be read does not replace a refusal or Deleted.
    public var resultHeadline: String {
        if let summary = report?.summary, !summary.isEmpty { return summary }
        if error == "The file could not be read. Refusing.",
           let notice = resultNotice, !notice.isEmpty {
            return notice
        }
        if let error, !error.isEmpty { return error }
        return Copy.incomplete
    }

    /// A refusal is the headline, in the warning color. Deleted and a finished scan stay in the title color.
    public var resultRefused: Bool {
        if report?.status == "refused" { return true }
        return resultHeadline.contains("Refusing.")
    }

    /// The reason under Incomplete. The headline stays in the title color. This line is the warning.
    public var resultReason: String? {
        guard report?.status == "incomplete",
              let reason = report?.message,
              !reason.isEmpty,
              reason != report?.summary else { return nil }
        return reason
    }

    /// A catalog that can run is titled Model bundle. A refusal is the headline.
    /// A file that cannot be read stays above that headline. It does not rename an empty catalog.
    public var bundleHeadline: String {
        guard bundles.isEmpty else { return "Model bundle" }
        if let notice = catalogNotice, !notice.isEmpty {
            return notice
        }
        if let error, !error.isEmpty, error != "The file could not be read. Refusing." {
            return error
        }
        return "The scan program is not on this device. Refusing."
    }

    /// An empty or unreadable catalog already says why. Continue does not open the size page.
    public func continueFromBundles() {
        guard !closed else { return }
        guard !bundles.isEmpty else { return }
        step = .size
    }

    /// A refusal the current page does not already print. An empty catalog prints its own line.
    /// A file that cannot be read stays first. A refused estimate, a deleted result, and a result
    /// that is only a refusal keep that headline.
    public var showsTopError: Bool {
        guard let error, !error.isEmpty else { return false }
        if error == "The file could not be read. Refusing." {
            switch step {
            case .estimate:
                if estimate != nil { return true }
                if let notice = estimateNotice, !notice.isEmpty, notice != error { return true }
                return false
            case .results:
                if report != nil { return true }
                if let notice = resultNotice, !notice.isEmpty, notice != error { return true }
                return false
            default:
                return true
            }
        }
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
        guard !closed else { return }
        guard let file else {
            estimate = nil
            canAnalyze = false
            error = "The file could not be read. Refusing."
            estimateNotice = error
            step = .estimate
            return
        }
        do {
            estimate = try core.estimate(input: file, bundle: bundleID, longSide: longSide, coverage: coverage, phone: phone)
            if error != "The file could not be read. Refusing." {
                error = nil
            }
            estimateNotice = nil
            canAnalyze = includeMissing || includeWanted
        } catch {
            estimate = nil
            canAnalyze = false
            let message = error.localizedDescription
            estimateNotice = message
            if self.error != "The file could not be read. Refusing." {
                self.error = message
            }
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

    /// The file page and an estimate that can run both warn. A refusal is the headline instead.
    public var estimateWarning: String? { estimate != nil && oldFile ? Copy.oldFile : nil }

    /// A scan that can run is titled Estimate. A refusal is the headline, so the page does not say Estimate.
    /// A file that cannot be read stays above that headline.
    public var estimateHeadline: String {
        if scanning { return "Scanning" }
        if estimate == nil {
            if error == "The file could not be read. Refusing.",
               let notice = estimateNotice, !notice.isEmpty {
                return notice
            }
            if let error, !error.isEmpty { return error }
        }
        return "Estimate"
    }

    /// Back stays off while a scan is running.
    public var backEnabled: Bool { !scanning }

    public func goBack() {
        guard !closed else { return }
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
        guard !closed else { return }
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
        estimateNotice = nil
        resultNotice = nil
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
        guard let paths = makeScanPaths() else {
            refuseUncreatableOutput()
            return
        }
        apply(Self.finishedScan(core: core, file: file, bundleID: bundleID, longSide: longSide, coverage: coverage, phone: phone, missing: includeMissing, wanted: includeWanted, paths: paths))
    }

    /// Closing the window stops a scan that is still running, including a decode that is still writing frames, and removes that temporary folder.
    public func closeWindow() {
        closed = true
        stopFlag.stop()
        programs.stop()
        if let scanRoot {
            try? FileManager.default.removeItem(at: scanRoot)
            self.scanRoot = nil
        }
        resultDirectory = nil
        scanning = false
    }

    /// Leaves the estimate page in place and says Scanning until the result is ready.
    public func startScan() {
        guard !closed else { return }
        guard !scanning, canAnalyze, let file else { return }
        guard releaseResult() else { return }
        guard let paths = makeScanPaths() else {
            refuseUncreatableOutput()
            return
        }
        scanning = true
        liveCrops = []
        let core = core
        let bundleID = bundleID
        let longSide = longSide
        let coverage = coverage
        let phone = phone
        let missing = includeMissing
        let wanted = includeWanted
        let gate = ProgressOnce(seen: progressSeen, hold: progressHold)
        let result = paths.result
        let flag = stopFlag
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
                stopped: { flag.isStopped() },
                onProgress: { line in
                    Self.deliverCrop(line: line, directory: result, gate: gate, model: self)
                }
            )
            await MainActor.run {
                if self.closed {
                    try? FileManager.default.removeItem(at: outcome.root)
                    self.scanning = false
                    return
                }
                self.apply(outcome)
                self.liveCrops = []
                self.scanning = false
            }
        }
    }

    func appendLiveCrop(label: String, frameLabel: String, path: String) {
        guard !closed else { return }
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

    /// The result folder could not be created. The page stays here, and the refusal is the headline.
    private func refuseUncreatableOutput() {
        estimate = nil
        canAnalyze = false
        scanning = false
        error = "The output directory could not be created. Refusing."
        estimateNotice = error
    }

    private func makeScanPaths() -> ScanPaths? {
        leavingURL = nil
        leaveError = nil
        deleteNotice = nil
        let parent = scratchDirectory ?? FileManager.default.temporaryDirectory
        let root = parent.appendingPathComponent("openworld-" + UUID().uuidString)
        do {
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        } catch {
            return nil
        }
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
            let marker = outcome.result.appendingPathComponent("result.json")
            resultDirectory = FileManager.default.fileExists(atPath: marker.path) ? outcome.result : nil
            error = nil
            resultNotice = nil
        } else {
            report = nil
            resultDirectory = nil
            try? FileManager.default.removeItem(at: outcome.root)
            scanRoot = nil
            error = outcome.error
            resultNotice = outcome.error
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
        stopped: @Sendable () -> Bool = { false },
        onProgress: ((String) -> Void)? = nil
    ) -> FinishedScan {
        do {
            // The fixture pack is written by the CLI before a real curve allows FBI photos.
            try core.runPublic(posters: paths.posters)
            if stopped() { throw DecodeStopped() }
            let reel = try PlatformDecoder.writeFrames(url: file, directory: paths.frames, stopped: stopped)
            if stopped() { throw DecodeStopped() }
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
        } catch is DecodeStopped {
            return FinishedScan(report: nil, error: nil, keepRoot: false, root: paths.root, result: paths.result)
        } catch {
            return FinishedScan(report: nil, error: error.localizedDescription, keepRoot: false, root: paths.root, result: paths.result)
        }
    }

    /// Ask the library before any FBI page is shown. A lookalike host stays closed.
    public func requestLeave(_ urlString: String) {
        guard !closed else { return }
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
        guard let resultDirectory else {
            if let scanRoot { try? FileManager.default.removeItem(at: scanRoot) }
            self.scanRoot = nil
            return true
        }
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
        guard !closed else { return }
        guard removeResult() else { return }
        report = nil
        leavingURL = nil
        leaveError = nil
        error = "Deleted."
        resultNotice = "Deleted."
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

#if canImport(SwiftUI)
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
            if let notice = model.deleteNotice, !notice.isEmpty,
               model.step != .estimate, model.step != .results {
                Text(notice)
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
            Text(Copy.importHint(phone: model.phone))
                .foregroundStyle(.secondary)
            importControl
            Spacer()
        }
        .padding()
    }

    private var device: some View {
        ScrollView { deviceColumn }
    }

    private var deviceColumn: some View {
        VStack(alignment: .leading, spacing: 10) {
            backControl
            Text(Copy.onDevice).font(model.phone ? .largeTitle : .title)
            if let file = model.file {
                Text(PhonePreview.wrappingFileName(file.lastPathComponent))
                    .font(.headline)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityLabel(file.lastPathComponent)
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
        ScrollView { bundleColumn }
    }

    private var bundleColumn: some View {
        VStack(alignment: .leading, spacing: 8) {
            backControl
            Text(model.bundleHeadline)
                .font(model.phone ? .largeTitle : .title)
                .foregroundStyle(model.bundles.isEmpty && model.bundleHeadline != "Model bundle" ? .orange : .primary)
            if !model.bundles.isEmpty {
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
            }
            prominent("Continue") { model.continueFromBundles() }
                .disabled(model.bundles.isEmpty)
        }
        .padding(model.phone ? 0 : 8)
    }

    private var size: some View {
        ScrollView { sizeColumn }
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
        ScrollViewReader { proxy in
            ScrollView { estimateColumn }
                .onChange(of: model.deleteNotice, initial: true) { _, notice in
                    if notice != nil {
                        proxy.scrollTo("deleteNotice", anchor: .top)
                    }
                }
        }
    }

    private var estimateColumn: some View {
        VStack(alignment: .leading, spacing: 10) {
            backControl
            if let notice = model.deleteNotice {
                Text(notice).foregroundStyle(.orange).id("deleteNotice")
            }
            Text(model.estimateHeadline)
                .font(model.phone ? .largeTitle : .title)
                .foregroundStyle(model.estimate == nil && !model.scanning && model.error != nil ? .orange : .primary)
            if model.scanning, !model.liveCrops.isEmpty {
                Text("Crops from this file.")
                    .font(model.phone ? .title3 : .headline)
                ForEach(model.liveCrops) { crop in
                    VStack(spacing: 4) {
                        liveImage(crop.path)
                        Text(crop.frameLabel).multilineTextAlignment(.center)
                        Text(crop.label).multilineTextAlignment(.center)
                    }
                    .frame(width: 140)
                }
            }
            if let estimate = model.estimate {
                if let warning = model.estimateWarning {
                    Text(warning).foregroundStyle(.orange)
                }
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
            }
            if model.showsAnalyze {
                prominent(model.scanning ? "Scanning" : "Analyze") { model.startScan() }
                    .disabled(model.scanning || !model.canAnalyze)
            }
            Spacer()
        }
        .padding()
    }

    @ViewBuilder
    private var results: some View {
        ScrollViewReader { proxy in
            ScrollView {
                resultsColumn
            }
            .onChange(of: model.leaveError, initial: true) { _, notice in
                if notice != nil {
                    proxy.scrollTo("leaveError", anchor: .top)
                }
            }
            .onChange(of: model.deleteNotice, initial: true) { _, notice in
                if notice != nil {
                    proxy.scrollTo("deleteNotice", anchor: .top)
                }
            }
        }
        .confirmationDialog(Copy.leaving, isPresented: leavingPresented, titleVisibility: .visible) {
            Button("Open") {
                if let url = model.leavingURL { openURL(url) }
            }
            Button("Stay", role: .cancel) { model.leavingURL = nil }
        } message: {
            Text(model.leavingURL?.absoluteString ?? "")
        }
    }

    private var resultsColumn: some View {
        VStack(alignment: .leading, spacing: 12) {
                backControl
                if let notice = model.leaveError {
                    Text(notice).foregroundStyle(.orange).id("leaveError")
                }
                if let notice = model.deleteNotice {
                    Text(notice).foregroundStyle(.orange).id("deleteNotice")
                }
                Text(model.resultHeadline)
                    .font(model.phone ? .largeTitle : .title)
                    .foregroundStyle(model.resultRefused ? .orange : .primary)
                if let reason = model.resultReason {
                    Text(reason).foregroundStyle(.orange)
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
                        .padding()
                        .background(.quaternary.opacity(0.4))
                        .clipShape(RoundedRectangle(cornerRadius: model.phone ? 12 : 8))
                    }
                }
                if let items = model.report?.inventory, !items.isEmpty {
                    Text("Crops from this file.")
                        .font(model.phone ? .title3 : .headline)
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
                }
                ForEach(PhonePreview.disclosureLines(model), id: \.self) { line in
                    Text(line).font(.footnote)
                }
                if model.resultDirectory != nil {
                    prominent("Delete") { model.deleteResult() }
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
        #endif
    }

    @ViewBuilder private var sizePicker: some View {
        #if os(macOS)
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
        #if os(macOS)
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
        Button(title, action: action).buttonStyle(.borderedProminent)
    }

    private var leavingPresented: Binding<Bool> {
        Binding(get: { model.leavingURL != nil }, set: { if !$0 { model.leavingURL = nil } })
    }
}
#endif
