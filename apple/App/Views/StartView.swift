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

    var body: some View {
        Page {
            Text("Check a photo or video you already have against public FBI missing and wanted posters, on this device.")
                .font(.title3)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
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
        #else
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button(action: chooseFile) {
                    Label("Choose a Photo or Video…", systemImage: "folder")
                }
                .help("Choose a photo or video (⌘O)")
                .disabled(model.isScanning)
            }
        }
        #endif
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
            if model.demoBuild, !postersReady {
                Button("Install demo posters") {
                    Task { await model.installDemoPosters() }
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("install-demo-posters")
            }
            Text("Results are deleted after 7 days, or when the posters expire.")
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
        .padding(.top, 8)
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
