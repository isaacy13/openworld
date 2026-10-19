// SPDX-License-Identifier: Apache-2.0
import OpenWorldKit
import SwiftUI

/// Screen 3: the scan as it runs, then its result, on one screen.
struct ScanView: View {
    let model: ScanModel
    @State private var confirmStop = false
    @State private var confirmDelete = false
    @State private var leaving: LeavePrompt?
    @State private var shownFrame: ShownFrame?
    @Environment(\.openURL) private var openURL

    var body: some View {
        Page {
            switch model.state {
            case .scanning(let progress):
                scanning(progress)
            case .result(let result):
                ResultContent(result: result, copy: model.copy, showFrame: { shownFrame = $0 }, open: open, delete: { confirmDelete = true }, again: { model.startOver() })
            case .refused(let line):
                refused(line)
            default:
                EmptyView()
            }
        }
        .navigationTitle(title)
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .navigationBarBackButtonHidden(model.isScanning)
        .toolbar {
            if model.isScanning {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Back") { confirmStop = true }
                }
            }
        }
        .confirmationDialog("Stop the scan?", isPresented: $confirmStop, titleVisibility: .visible) {
            Button("Stop scan", role: .destructive) { model.stop(.cancel) }
            Button("Keep scanning", role: .cancel) {}
        } message: {
            Text("A stopped scan is Incomplete. It is never a clearance.")
        }
        .confirmationDialog("Delete this result?", isPresented: $confirmDelete, titleVisibility: .visible) {
            Button("Delete result", role: .destructive) {
                Task { await model.deleteResult() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("The crops and frames from this scan are removed from this device.")
        }
        .sheet(item: $leaving) { prompt in
            LeaveSheet(prompt: prompt) { openURL(prompt.url) }
        }
        .sheet(item: $shownFrame) { frame in
            FrameSheet(frame: frame)
        }
    }

    private var title: String {
        switch model.state {
        case .scanning: return String(localized: "Scanning")
        case .refused: return String(localized: "Not scanned")
        default: return String(localized: "Result")
        }
    }

    /// Ask the core first. Only the URL it approves can open.
    private func open(_ candidate: Candidate) {
        Task { leaving = await model.leave(candidate) }
    }

    // MARK: Scanning

    @ViewBuilder
    private func scanning(_ progress: ScanProgress) -> some View {
        Card {
            Label("Scanning on this device", systemImage: "lock.shield")
                .font(.headline)
            ProgressView(value: progress.fraction)
                .accessibilityLabel(Text("Scan progress"))
            Text(progressLine(progress))
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .monospacedDigit()
            if progress.stopping != nil {
                Text("Stopping. The result will say Incomplete.")
                    .font(.subheadline)
                    .foregroundStyle(Tone.incomplete.text)
            }
        }
        if !progress.faces.isEmpty {
            Card {
                Text("Faces")
                    .font(.headline)
                FaceGrid(cells: progress.faces.map(FaceCell.init(tile:)))
            }
        }
        Button {
            model.stop(.cancel)
        } label: {
            Label("Stop", systemImage: "stop.circle")
                .frame(maxWidth: .infinity)
        }
        .buttonStyle(.bordered)
        .controlSize(.large)
        .disabled(progress.stopping != nil)
        .accessibilityIdentifier("stop")
        Text("A stop is Incomplete, never a clearance.")
            .font(.footnote)
            .foregroundStyle(.secondary)
    }

    private func progressLine(_ progress: ScanProgress) -> String {
        guard let frame = progress.frame else {
            return String(localized: "Starting…")
        }
        let position = String(localized: "Frame \((frame + 1).formatted()) of \(progress.totalFrames.formatted())")
        guard let left = progress.secondsLeft else { return position }
        let time = Duration.seconds(max(1, left)).formatted(.units(allowed: [.hours, .minutes, .seconds], width: .wide, maximumUnitCount: 1))
        return "\(position) · " + String(localized: "about \(time) left")
    }

    // MARK: Refused

    @ViewBuilder
    private func refused(_ line: Line) -> some View {
        Card(tone: .error) {
            LineLabel(line: line, systemImage: "xmark.octagon", tone: .error)
                .font(.title3.weight(.semibold))
                .accessibilityAddTraits(.isHeader)
            Text(advice(line.code))
                .foregroundStyle(Tone.error.text)
                .fixedSize(horizontal: false, vertical: true)
        }
        .onAppear {
            AccessibilityNotification.Announcement(line.localized()).post()
        }
        if Self.posterRefusals.contains(line.code), model.demoBuild {
            Button("Install demo posters") {
                Task { await model.installDemoPosters() }
            }
            .buttonStyle(.bordered)
        }
        Button {
            model.startOver()
        } label: {
            Text("Choose another file")
                .frame(maxWidth: .infinity)
        }
        .buttonStyle(.borderedProminent)
        .controlSize(.large)
    }

    static let posterRefusals: Set<String> = ["missing_pack", "bad_hash", "expired_pack", "old_pack", "future_pack", "unreadable_pack", "bad_vectors", "embedder_mismatch", "unknown_perception"]

    /// What to do next, by refusal code.
    private func advice(_ code: String) -> String {
        if Self.posterRefusals.contains(code) {
            return String(localized: "Nothing was scanned. Install the posters again, then scan.")
        }
        switch code {
        case "unreadable", "bad_codec", "no_fps", "too_large":
            return String(localized: "Nothing was scanned. Choose another file, or open this one on a computer.")
        default:
            return String(localized: "Nothing was scanned. Try again, or choose another file.")
        }
    }
}

/// The result: headline, candidate cards, other faces, details, disclosure, and actions.
private struct ResultContent: View {
    let result: ScanResult
    let copy: CopyTable
    let showFrame: (ShownFrame) -> Void
    let open: (Candidate) -> Void
    let delete: () -> Void
    let again: () -> Void

    private var report: Report { result.report }

    var body: some View {
        HeadlineCard(report: report)
        if let note = report.perceptionNote {
            LineLabel(line: note, systemImage: "theatermasks", tone: .demo)
                .font(.subheadline)
        }
        if let banner = report.coverageBanner {
            LineLabel(line: banner, systemImage: "exclamationmark.triangle", tone: .incomplete)
                .font(.subheadline)
        }
        ForEach(report.warnings, id: \.self) { warning in
            LineLabel(line: warning, systemImage: "exclamationmark.triangle", tone: .incomplete)
                .font(.subheadline)
        }
        ForEach(report.candidates) { candidate in
            CandidateCard(candidate: candidate, crop: result.url(candidate.crop), showFrame: {
                if let url = result.url(candidate.frame) {
                    showFrame(ShownFrame(url: url, label: candidate.frameLabel))
                }
            }, open: { open(candidate) })
        }
        let others = report.otherFaces
        if !others.isEmpty {
            Card {
                Text(report.candidates.isEmpty ? String(localized: "Faces") : String(localized: "Other faces"))
                    .font(.headline)
                FaceGrid(cells: others.map { FaceCell(track: $0, crop: result.url($0.crop)) })
            }
        }
        Card {
            DisclosureGroup {
                DetailsList(report: report)
                    .padding(.top, 8)
            } label: {
                Label("Details", systemImage: "list.bullet.rectangle")
                    .font(.headline)
            }
        }
        Card {
            Label("What happened to this file", systemImage: "lock.shield")
                .font(.headline)
            ForEach(report.disclosure, id: \.self) { line in
                Text(line.localized())
                    .font(.subheadline)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        VStack(spacing: 12) {
            Button(role: .destructive, action: delete) {
                Label("Delete result", systemImage: "trash")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .controlSize(.large)
            .accessibilityIdentifier("delete-result")
            Button(action: again) {
                Label("Scan another file", systemImage: "plus.circle")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
        }
    }
}

/// The headline card for a result. Announced when it appears.
private struct HeadlineCard: View {
    let report: Report

    var body: some View {
        Card(tone: report.tone) {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: report.symbol)
                    .font(.title2)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 6) {
                    Text(report.headline.localized())
                        .font(.title3.weight(.bold))
                        .fixedSize(horizontal: false, vertical: true)
                    if let detail {
                        Text(detail)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
            .foregroundStyle(report.tone.text)
        }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
        .onAppear {
            AccessibilityNotification.Announcement(report.headline.localized()).post()
        }
    }

    private var detail: String? {
        switch report.tone {
        case .candidate:
            return String(localized: "\(report.candidates.count) possible candidates. Review each one below.")
        case .incomplete:
            return report.reason?.localized()
        case .neutral, .error, .demo:
            return report.facesLine?.localized()
        }
    }
}

/// The record of how the scan ran, with the bundle and poster hashes.
private struct DetailsList: View {
    let report: Report

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            row("Frames", report.framesLine?.localized())
            row("Faces", report.facesLine?.localized())
            row("Coverage", report.coverageLine?.localized())
            row("Detection size", report.detectionLine?.localized())
            row("Chance", report.chance?.localized())
            row("Classes", report.classes?.localized())
            if let bundle = report.bundle, !bundle.id.isEmpty {
                row("Bundle", "\(bundle.name) \(bundle.version)")
                ForEach(bundle.weights, id: \.sha256) { weight in
                    row(LocalizedStringKey(weight.role.capitalized), "\(weight.name) · SHA-256 \(weight.sha256)", mono: true)
                }
            }
            if let posters = report.posters, !posters.id.isEmpty {
                row("Posters", "\(posters.id) · SHA-256 \(posters.sha256)", mono: true)
            }
            row("Ran on", ranOn)
            row("Device note", report.deviceNote?.localized())
        }
    }

    private var ranOn: String? {
        guard let execution = report.execution, !execution.isEmpty else { return nil }
        let name: String
        switch execution {
        case "neural": name = String(localized: "Neural Engine")
        case "gpu": name = String(localized: "GPU")
        default: name = String(localized: "CPU")
        }
        guard let provider = report.provider, !provider.isEmpty else { return name }
        return "\(name) · \(provider)"
    }

    @ViewBuilder
    private func row(_ title: LocalizedStringKey, _ value: String?, mono: Bool = false) -> some View {
        if let value, !value.isEmpty {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                Text(value)
                    .font(mono ? .footnote.monospaced() : .subheadline)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .accessibilityElement(children: .combine)
        }
    }
}

/// A frame to show in a sheet.
struct ShownFrame: Identifiable {
    let url: URL
    let label: Line

    var id: URL { url }
}
