// SPDX-License-Identifier: Apache-2.0
import OpenWorldKit
import PhotosUI
import SwiftUI

/// Screen 1: choose the file. Nothing is scanned until Analyze.
struct StartView: View {
    let model: ScanModel
    let dropTargeted: Bool
    let chooseFile: () -> Void
    let pickPhoto: (PhotosPickerItem) -> Void
    @State private var photo: PhotosPickerItem?
    @State private var showAbout = false

    var body: some View {
        Page {
            Text("Check a photo or video you already have against public FBI missing and wanted posters, on this device.")
                .font(.title3)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if let notAffiliated = model.notAffiliated {
                Text(notAffiliated.localized())
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if model.interruptedScans > 0 {
                interrupted
            }
            chooser
            FlowLayout().callAsFunction {
                Chip(text: String(localized: "Stays on this device"), systemImage: "lock.shield")
                Chip(text: String(localized: "Nothing uploaded"), systemImage: "icloud.slash")
                Chip(text: String(localized: "No camera"), systemImage: "video.slash")
            }
            posters
        }
        .navigationTitle("OpenWorld")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.large)
        #endif
        .toolbar {
            #if os(macOS)
            ToolbarItem(placement: .primaryAction) {
                Button(action: chooseFile) {
                    Label("Choose a Photo or Video…", systemImage: "folder")
                }
                .help("Choose a photo or video (⌘O)")
                .disabled(model.isScanning)
            }
            #endif
            ToolbarItem(placement: .primaryAction) {
                Button {
                    showAbout = true
                } label: {
                    Label("About OpenWorld", systemImage: "info.circle")
                }
                .help("About OpenWorld")
                .accessibilityIdentifier("about")
            }
        }
        .sheet(isPresented: $showAbout) {
            AboutSheet(model: model)
        }
        .onChange(of: photo) { _, item in
            guard let item else { return }
            photo = nil
            pickPhoto(item)
        }
    }

    private var chooser: some View {
        VStack(spacing: 16) {
            Image(systemName: "photo.on.rectangle.angled")
                .font(.system(size: 44, weight: .light))
                .foregroundStyle(Color.accentColor)
                .accessibilityHidden(true)
            #if os(iOS)
            PhotosPicker(selection: $photo, matching: .any(of: [.images, .videos]), preferredItemEncoding: .current) {
                Label("Choose a photo or video", systemImage: "photo")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .accessibilityIdentifier("choose-photo")
            Button(action: chooseFile) {
                Label("Files", systemImage: "folder")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .controlSize(.large)
            .accessibilityIdentifier("choose-file")
            #else
            Button(action: chooseFile) {
                Label("Choose a photo or video", systemImage: "photo")
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .accessibilityIdentifier("choose-file")
            Text("or drop it here")
                .foregroundStyle(.secondary)
            #endif
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 28)
        .padding(.horizontal, 16)
        .background(Color.card, in: RoundedRectangle(cornerRadius: Metrics.corner, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: Metrics.corner, style: .continuous)
                .strokeBorder(dropTargeted ? Color.accentColor : Color.secondary.opacity(0.3), style: StrokeStyle(lineWidth: dropTargeted ? 2 : 1, dash: [6, 4]))
        }
        .disabled(model.isScanning)
    }

    private var interrupted: some View {
        Card(tone: .incomplete) {
            Label {
                VStack(alignment: .leading, spacing: 4) {
                    Text(model.copy.line("incomplete").localized())
                        .font(.headline)
                    Text("The system stopped the last scan before it finished. It is not a clearance.")
                        .fixedSize(horizontal: false, vertical: true)
                    // The core's disclosure for a scan that did not finish.
                    ForEach(model.copy.disclosure, id: \.self) { line in
                        Text(line.localized())
                            .font(.subheadline)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            } icon: {
                Image(systemName: "exclamationmark.triangle")
            }
            .foregroundStyle(Tone.incomplete.text)
            Button("Dismiss") { model.interruptedScans = 0 }
                .buttonStyle(.bordered)
        }
    }

    private var posters: some View {
        VStack(alignment: .leading, spacing: 10) {
            switch model.posters {
            case .loading:
                ProgressView()
            case .ready(let pack):
                HStack(spacing: 8) {
                    if pack.demo {
                        Chip(text: String(localized: "Demo"), systemImage: "theatermasks", tone: .demo)
                    }
                    Text(packLine(pack))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
            case .refused(let line):
                if line.code == "missing_pack" {
                    Text("No posters installed. Real FBI photos stay off.")
                        .font(.subheadline)
                } else {
                    LineLabel(line: line, systemImage: "xmark.octagon", tone: .error)
                        .font(.subheadline)
                }
            }
            FlowLayout().callAsFunction {
                if model.demoBuild, !postersReady {
                    Button("Install demo posters") {
                        Task { await model.installDemoPosters() }
                    }
                    .buttonStyle(.bordered)
                    .disabled(updating)
                    .accessibilityIdentifier("install-demo-posters")
                }
                Button {
                    Task { await model.updatePosters() }
                } label: {
                    Label("Update posters", systemImage: "arrow.down.circle")
                }
                .buttonStyle(.bordered)
                .disabled(updating || model.isScanning)
                .accessibilityIdentifier("update-posters")
            }
            updateStatus
                .font(.footnote)
                .foregroundStyle(.secondary)
            Text("Results are deleted after 7 days, or when the posters expire.")
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
        .padding(.top, 8)
    }

    /// Update posters' progress, then what it kept and left out, or the core's refusal, in quiet type.
    @ViewBuilder
    private var updateStatus: some View {
        switch model.update {
        case .idle:
            EmptyView()
        case .running(let progress):
            if progress.posters > 0 {
                ProgressView(value: Double(progress.checked), total: Double(progress.posters)) {
                    Text("Checking poster photos · \(progress.checked) of \(progress.posters)")
                }
            } else {
                HStack(spacing: 8) {
                    ProgressView()
                        .controlSize(.small)
                    if progress.page == 0 {
                        Text("Reading the FBI list…")
                    } else if let pages = progress.pages {
                        Text("Reading the FBI list · page \(progress.page) of \(pages)")
                    } else {
                        Text("Reading the FBI list · page \(progress.page)")
                    }
                }
            }
        case .done(let kept, let skipped):
            VStack(alignment: .leading, spacing: 4) {
                Text("Kept \(kept) posters.")
                ForEach(skipped, id: \.reason.code) { row in
                    Text("\(row.count) left out: \(row.reason.localized())")
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        case .refused(let line):
            Text(line.localized())
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private var updating: Bool {
        if case .running = model.update { return true }
        return false
    }

    private var postersReady: Bool {
        if case .ready = model.posters { return true }
        return false
    }

    private func packLine(_ pack: PackInfo) -> String {
        let name = pack.demo ? String(localized: "Demo posters") : String(localized: "FBI posters")
        let count = String(localized: "\(pack.count) posters")
        return "\(name) · \(count) · \(fetched(pack.created))"
    }

    private func fetched(_ date: Date?) -> String {
        guard let date else { return String(localized: "fetch date unknown") }
        if Calendar.current.isDateInToday(date) {
            return String(localized: "fetched today")
        }
        return String(localized: "fetched \(date.formatted(.relative(presentation: .named)))")
    }
}
