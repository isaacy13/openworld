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

public enum Step {
    case choose, device, bundle, size, estimate, results
}

@MainActor
public final class FlowModel: ObservableObject {
    @Published public var step: Step = .choose
    @Published public var file: URL?
    @Published public var bundles: [BundleRow] = []
    @Published public var bundleID = "fast"
    @Published public var longSide = "640"
    @Published public var coverage = "complete"
    @Published public var estimate: Estimate?
    @Published public var report: ScanReport?
    @Published public var resultDirectory: URL?
    @Published public var leavingURL: URL?
    @Published public var leaveError: String?
    @Published public var deleteNotice: String?
    @Published public var oldFile = false
    @Published public var error: String?
    @Published public var canAnalyze = true
    let phone: Bool
    private let core = CoreClient()
    private var scanRoot: URL?

    public init(phone: Bool) {
        self.phone = phone
    }

    public func choose(_ url: URL) {
        #if !os(Linux)
        _ = url.startAccessingSecurityScopedResource()
        #endif
        file = url
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
        bundles = (try? core.bundlesJSON()) ?? []
        if let selected = bundles.first(where: { $0.preselected }) {
            bundleID = selected.id
        }
        step = .bundle
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
            canAnalyze = true
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

    public func goBack() {
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
        guard removeResult() else { return }
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
        step = .choose
    }

    public func analyze() {
        guard canAnalyze, let file else { return }
        guard removeResult() else { return }
        leavingURL = nil
        leaveError = nil
        deleteNotice = nil
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        scanRoot = root
        let posters = root.appendingPathComponent("posters")
        let frames = root.appendingPathComponent("frames")
        do {
            // The fixture pack is written by the CLI before a real curve allows FBI photos.
            try CoreClient().runPublic(posters: posters)
            let reel = try PlatformDecoder.writeFrames(url: file, directory: frames)
            let result = root.appendingPathComponent("result")
            report = try core.scan(
                input: file,
                bundle: bundleID,
                longSide: longSide,
                coverage: coverage,
                posters: posters,
                frames: reel.directory ?? frames,
                facts: reel,
                out: result,
                phone: phone
            )
            resultDirectory = result
            error = nil
        } catch {
            report = nil
            resultDirectory = nil
            if let scanRoot { try? FileManager.default.removeItem(at: scanRoot) }
            self.scanRoot = nil
            self.error = error.localizedDescription
        }
        step = .results
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
        _ = try run(PhoneArguments.writeFixture(out: posters.path))
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
            Spacer()
            prominent("Continue") { model.loadBundles() }
        }
        .padding()
    }

    private var bundles: some View {
        VStack(alignment: .leading, spacing: 8) {
            backControl
            Text("Model bundle").font(model.phone ? .largeTitle : .title)
            Text("Scores are not comparable across bundles. Results name the bundle you pick.")
                .foregroundStyle(.secondary)
            #if os(Linux)
            ForEach(model.bundles) { row in
                Button(action: { model.bundleID = row.id }) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(row.name).font(.headline)
                        Text(row.bestFor)
                        Text(row.curveLine).foregroundStyle(.secondary)
                    }
                }
            }
            #else
            List(model.bundles) { row in
                Button {
                    model.bundleID = row.id
                } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(row.name).font(.headline)
                        Text(row.bestFor)
                        Text(row.curveLine).foregroundStyle(.secondary)
                    }
                }
                .listRowBackground(row.id == model.bundleID ? Color.accentColor.opacity(0.15) : Color.clear)
            }
            #endif
            prominent("Continue") { model.step = .size }
        }
        .padding(model.phone ? 0 : 8)
    }

    private var size: some View {
        VStack(alignment: .leading, spacing: 12) {
            backControl
            Text("Detection size").font(model.phone ? .largeTitle : .title)
            Text("Smaller frames are a resize of each decoded frame in memory. Evidence crops come from the original frame. Full resolution is slower.")
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
        VStack(alignment: .leading, spacing: 10) {
            backControl
            Text("Estimate").font(model.phone ? .largeTitle : .title)
            if let estimate = model.estimate {
                Text(estimate.human).font(.headline)
                Text(estimate.caveat).foregroundStyle(.secondary)
                if let note = estimate.deviceNote { Text(note) }
                if let note = estimate.heatNote { Text(note) }
                if let note = estimate.batteryNote { Text(note) }
                if let note = estimate.suggestComputerText { Text(note) }
                Text(model.choiceLine)
            } else if let error = model.error {
                Text(error)
            }
            if model.coverage == "measured" {
                Text(Copy.brief)
            }
            if model.canAnalyze {
                prominent("Analyze") { model.analyze() }
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
                if let banner = model.report?.coverageBanner {
                    Text(banner)
                }
                if let name = model.report?.bundleName {
                    Text("Bundle: \(name)")
                }
                if let note = model.report?.perceptionNote {
                    Text(note)
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
                                labeledCrop(candidate.frame, "Frame")
                            }
                            Text(candidate.uncertainty)
                            Text("\(candidate.posterTitle) (\(candidate.posterClass))")
                            prominent("Open FBI page") {
                                model.requestLeave(candidate.fbiUrl)
                            }
                        }
                        #if !os(Linux)
                        .padding()
                        .background(.quaternary.opacity(0.4))
                        .clipShape(RoundedRectangle(cornerRadius: model.phone ? 12 : 8))
                        #endif
                    }
                } else if model.report?.status == "complete" {
                    Text(Copy.clearance)
                }
                if let items = model.report?.inventory, !items.isEmpty {
                    Text("Crops from this file.")
                        .font(model.phone ? .title3 : .headline)
                    #if os(Linux)
                    HStack(alignment: .top, spacing: 12) {
                        ForEach(items) { item in
                            VStack(spacing: 4) {
                                cropImage(item.crop)
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
                ForEach(model.report?.disclosure ?? [], id: \.self) { line in
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
            sizeChoice("320", "320 px on the long side")
            sizeChoice("480", "480 px on the long side")
            sizeChoice("640", "640 px on the long side")
            sizeChoice("full", "Full resolution")
        }
        #elseif os(macOS)
        let picker = Picker("Detection size", selection: $model.longSide) {
            Text("320 px on the long side").tag("320")
            Text("480 px on the long side").tag("480")
            Text("640 px on the long side").tag("640")
            Text("Full resolution").tag("full")
        }
        if model.phone {
            picker.pickerStyle(.inline)
        } else {
            picker.pickerStyle(.radioGroup)
        }
        #else
        Picker("Detection size", selection: $model.longSide) {
            Text("320 px on the long side").tag("320")
            Text("480 px on the long side").tag("480")
            Text("640 px on the long side").tag("640")
            Text("Full resolution").tag("full")
        }
        .pickerStyle(.inline)
        #endif
    }

    @ViewBuilder private var coveragePicker: some View {
        #if os(Linux)
        VStack(alignment: .leading, spacing: 8) {
            Button(action: { model.coverage = "complete" }) {
                Text(model.coverage == "complete" ? "Complete. Every decoded frame. Selected." : "Complete. Every decoded frame.")
            }
            Button(action: { model.coverage = "measured" }) {
                Text(model.coverage == "measured" ? "Measured. 5 frames a second, plus the tracker. Selected." : "Measured. 5 frames a second, plus the tracker.")
            }
        }
        #elseif os(macOS)
        let picker = Picker("Coverage", selection: $model.coverage) {
            Text("Complete. Every decoded frame.").tag("complete")
            Text("Measured. 5 frames a second, plus the tracker.").tag("measured")
        }
        if model.phone {
            picker.pickerStyle(.inline)
        } else {
            picker.pickerStyle(.radioGroup)
        }
        #else
        Picker("Coverage", selection: $model.coverage) {
            Text("Complete. Every decoded frame.").tag("complete")
            Text("Measured. 5 frames a second, plus the tracker.").tag("measured")
        }
        .pickerStyle(.inline)
        #endif
    }

    #if os(Linux)
    @ViewBuilder
    private func sizeChoice(_ value: String, _ title: String) -> some View {
        Button(action: { model.longSide = value }) {
            Text(model.longSide == value ? "\(title). Selected." : title)
        }
    }
    #endif

    @ViewBuilder
    private var backControl: some View {
        Button(action: { model.goBack() }) { Text("Back") }
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
