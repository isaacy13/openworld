// SPDX-License-Identifier: Apache-2.0
import SwiftUI

enum Step {
    case choose, device, bundle, size, estimate, results
}

@MainActor
final class FlowModel: ObservableObject {
    @Published var step: Step = .choose
    @Published var file: URL?
    @Published var bundles: [BundleRow] = []
    @Published var bundleID = "fast"
    @Published var longSide = "640"
    @Published var coverage = "complete"
    @Published var estimate: Estimate?
    @Published var report: ScanReport?
    @Published var leavingURL: URL?
    @Published var oldFile = false
    @Published var error: String?
    let phone: Bool
    private let core = CoreClient()

    init(phone: Bool) {
        self.phone = phone
    }

    func choose(_ url: URL) {
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

    func loadBundles() {
        bundles = (try? core.bundlesJSON()) ?? []
        if let selected = bundles.first(where: { $0.preselected }) {
            bundleID = selected.id
        }
        step = .bundle
    }

    func loadEstimate() {
        guard let file else { return }
        estimate = try? core.estimate(input: file, bundle: bundleID, longSide: longSide, coverage: coverage, phone: phone)
        step = .estimate
    }

    func analyze() {
        guard let file else { return }
        let out = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let posters = out.appendingPathComponent("posters")
        // The fixture pack is written by the CLI before a real curve allows FBI photos.
        _ = try? CoreClient().runPublic(posters: posters)
        report = try? core.scan(
            input: file,
            bundle: bundleID,
            longSide: longSide,
            coverage: coverage,
            posters: posters,
            out: out.appendingPathComponent("result"),
            phone: phone
        )
        step = .results
    }
}

extension CoreClient {
    func runPublic(posters: URL) throws {
        _ = try run(["posters", "write-fixture", "--out", posters.path])
    }
}

struct FlowView: View {
    @ObservedObject var model: FlowModel
    var importControl: AnyView
    @Environment(\.openURL) private var openURL

    var body: some View {
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
            Button("Continue") { model.loadBundles() }
                .buttonStyle(.borderedProminent)
        }
        .padding()
    }

    private var bundles: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Model bundle").font(model.phone ? .largeTitle : .title)
            Text("Scores are not comparable across bundles. Results name the bundle you pick.")
                .foregroundStyle(.secondary)
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
            Button("Continue") { model.step = .size }
                .buttonStyle(.borderedProminent)
        }
        .padding(model.phone ? 0 : 8)
    }

    private var size: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Detection size").font(model.phone ? .largeTitle : .title)
            Text("Smaller frames are a resize of each decoded frame in memory. Evidence crops come from the original frame. Full resolution is slower.")
                .foregroundStyle(.secondary)
            sizePicker
            coveragePicker
            if model.coverage == "measured" {
                Text(Copy.brief)
            }
            Button("Continue") { model.loadEstimate() }
                .buttonStyle(.borderedProminent)
        }
        .padding()
    }

    private var estimate: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Estimate").font(model.phone ? .largeTitle : .title)
            if let estimate = model.estimate {
                Text(estimate.human).font(.headline)
                Text(estimate.caveat).foregroundStyle(.secondary)
                if let note = estimate.deviceNote { Text(note) }
                if let note = estimate.heatNote { Text(note) }
                if let note = estimate.batteryNote { Text(note) }
                if let note = estimate.suggestComputerText { Text(note) }
            }
            if model.coverage == "measured" {
                Text(Copy.brief)
            }
            Button("Analyze") { model.analyze() }
                .buttonStyle(.borderedProminent)
            Spacer()
        }
        .padding()
    }

    private var results: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Text(model.report?.summary ?? Copy.incomplete)
                    .font(model.phone ? .largeTitle : .title)
                if let banner = model.report?.coverageBanner {
                    Text(banner)
                }
                if let name = model.report?.bundleName {
                    Text("Bundle: \(name)")
                }
                if model.report?.candidates.isEmpty == false {
                    ForEach(model.report?.candidates ?? []) { candidate in
                        VStack(alignment: .leading, spacing: 6) {
                            Text(candidate.wording).font(.headline)
                            Text(candidate.uncertainty)
                            Text("\(candidate.posterTitle) (\(candidate.posterClass))")
                            Button("Open FBI page") {
                                model.leavingURL = URL(string: candidate.fbiUrl)
                            }
                        }
                        .padding()
                        .background(.quaternary.opacity(0.4))
                        .clipShape(RoundedRectangle(cornerRadius: model.phone ? 12 : 8))
                    }
                } else if model.report?.status == "complete" {
                    Text(Copy.clearance)
                }
                ForEach(model.report?.disclosure ?? [], id: \.self) { line in
                    Text(line).font(.footnote)
                }
            }
            .padding()
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


    @ViewBuilder private var sizePicker: some View {
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
    }

    @ViewBuilder private var coveragePicker: some View {
        let picker = Picker("Coverage", selection: $model.coverage) {
            Text("Complete. Every decoded frame.").tag("complete")
            Text("Measured. 5 frames a second, plus the tracker.").tag("measured")
        }
        if model.phone {
            picker.pickerStyle(.inline)
        } else {
            picker.pickerStyle(.radioGroup)
        }
    }

    private var leavingPresented: Binding<Bool> {
        Binding(get: { model.leavingURL != nil }, set: { if !$0 { model.leavingURL = nil } })
    }
}
