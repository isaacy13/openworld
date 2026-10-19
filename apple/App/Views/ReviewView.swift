// SPDX-License-Identifier: Apache-2.0
import OpenWorldKit
import QuickLookThumbnailing
import SwiftUI

/// Screen 2: what will be scanned, what it costs, and what happens to the file.
struct ReviewView: View {
    let model: ScanModel
    @State private var optionsOpen = false
    @State private var showDisclosure = false

    var body: some View {
        Page {
            if let review = model.review {
                FileCard(review: review)
                EstimateCard(estimate: review.estimate)
                classes
                options
                disclosure
                actions
            }
        }
        .navigationTitle("Review")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .sheet(isPresented: $showDisclosure) {
            DisclosureSheet(lines: model.copy.disclosure)
        }
    }

    // MARK: Compare with

    private var classes: some View {
        Card {
            Text("Compare with")
                .font(.headline)
            HStack(spacing: 12) {
                ClassToggle(title: String(localized: "Missing"), count: pack?.missing, isOn: model.options.missing) {
                    model.update { $0.missing.toggle() }
                }
                ClassToggle(title: String(localized: "Wanted"), count: pack?.wanted, isOn: model.options.wanted) {
                    model.update { $0.wanted.toggle() }
                }
            }
            if !model.options.missing, !model.options.wanted {
                LineLabel(line: model.copy.line("choose_class"), systemImage: "exclamationmark.triangle", tone: .incomplete)
                    .font(.subheadline)
            }
        }
    }

    private var pack: PackInfo? {
        if case .ready(let pack) = model.posters { return pack }
        return nil
    }

    // MARK: Options

    private var options: some View {
        Card {
            DisclosureGroup(isExpanded: $optionsOpen) {
                VStack(alignment: .leading, spacing: 12) {
                    Text("Model bundle")
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(.secondary)
                        .padding(.top, 8)
                    ForEach(model.bundles) { row in
                        ChoiceRow(selected: model.options.bundle == row.id, action: { model.update { $0.bundle = row.id } }) {
                            Text(row.name).font(.body.weight(.semibold))
                            Text(row.bestFor).font(.subheadline).foregroundStyle(.secondary)
                            Text(row.curveLine.localized())
                                .font(.footnote)
                                .foregroundStyle(row.curveExists ? Color.secondary : Tone.incomplete.text)
                            if !row.official, let notice = row.unofficialNotice {
                                Text(notice).font(.footnote).foregroundStyle(Tone.incomplete.text)
                            }
                        }
                    }
                    if let note = model.bundleNote {
                        Text(note.localized()).font(.footnote).foregroundStyle(.secondary)
                    }
                    Divider()
                    Text("Coverage")
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(.secondary)
                    ChoiceRow(selected: model.options.coverage == .complete, action: { model.update { $0.coverage = .complete } }) {
                        Text("Every frame").font(.body.weight(.semibold))
                        Text(model.copy.line("coverage_complete").localized()).font(.subheadline).foregroundStyle(.secondary)
                    }
                    ChoiceRow(selected: model.options.coverage == .measured, action: { model.update { $0.coverage = .measured } }) {
                        Text("5 frames a second").font(.body.weight(.semibold))
                        Text(model.copy.line("brief_face").localized()).font(.subheadline).foregroundStyle(.secondary)
                    }
                }
            } label: {
                Label("Options", systemImage: "slider.horizontal.3")
                    .font(.headline)
            }
        }
    }

    // MARK: Disclosure

    private var disclosure: some View {
        Card {
            Label("What happens to this file", systemImage: "lock.shield")
                .font(.headline)
            ForEach(model.copy.disclosure.prefix(3), id: \.self) { line in
                Text(line.localized())
                    .fixedSize(horizontal: false, vertical: true)
            }
            Button("Read all") { showDisclosure = true }
                .accessibilityIdentifier("read-all")
        }
    }

    // MARK: Actions

    private var actions: some View {
        VStack(spacing: 12) {
            if case .refused(let line) = model.posters {
                LineLabel(line: line, systemImage: "exclamationmark.triangle", tone: .incomplete)
                    .font(.subheadline)
                if model.demoBuild {
                    Button("Install demo posters") {
                        Task { await model.installDemoPosters() }
                    }
                    .buttonStyle(.bordered)
                }
            }
            Button {
                model.analyze()
            } label: {
                Label("Analyze on this device", systemImage: "lock.shield")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .disabled(!model.canAnalyze)
            .accessibilityIdentifier("analyze")
            Button {
                model.startOver()
            } label: {
                Text("Choose another file")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .controlSize(.large)
        }
        .padding(.top, 4)
    }
}

/// The file: thumbnail, name, kind, size or length, when it was recorded, and its warnings.
private struct FileCard: View {
    let review: Review
    @ScaledMetric private var side: CGFloat = 76

    var body: some View {
        Card {
            HStack(alignment: .top, spacing: 14) {
                Thumbnail(url: review.file.url, side: side)
                VStack(alignment: .leading, spacing: 4) {
                    Text(review.file.name)
                        .font(.headline)
                        .fixedSize(horizontal: false, vertical: true)
                    if let media = review.media {
                        Text(summary(media))
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                        if let recorded = media.recorded {
                            Text("Recorded \(recorded.formatted(date: .abbreviated, time: .shortened))")
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                        }
                    } else if case .loading = review.estimate {
                        Text("Reading the file…")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            .accessibilityElement(children: .combine)
            ForEach(review.warnings, id: \.self) { warning in
                LineLabel(line: warning, systemImage: "exclamationmark.triangle", tone: .incomplete)
                    .font(.subheadline)
            }
            if review.media?.countIsEstimate == true {
                Label("The frame count was estimated from the length. If it is off, the result is Incomplete.", systemImage: "info.circle")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func summary(_ media: MediaSummary) -> String {
        let size = "\(media.facts.width) × \(media.facts.height)"
        switch media.kind {
        case .photo:
            return String(localized: "Photo · \(size)")
        case .animation:
            return String(localized: "Animation · \(media.facts.frames.formatted()) frames · \(size)")
        case .video:
            let length = Duration.seconds(media.facts.durationSec).formatted(.time(pattern: media.facts.durationSec >= 3600 ? .hourMinuteSecond : .minuteSecond))
            return String(localized: "Video · \(length) · \(size)")
        }
    }
}

/// The estimate: the time, large, then frames and detection size, then device notes.
private struct EstimateCard: View {
    let estimate: EstimateState

    var body: some View {
        switch estimate {
        case .loading:
            Card {
                HStack(spacing: 10) {
                    ProgressView()
                    Text("Estimating…").foregroundStyle(.secondary)
                }
            }
        case .refused(let line):
            Card(tone: .error) {
                LineLabel(line: line, systemImage: "xmark.octagon", tone: .error)
                    .font(.headline)
                Text("Nothing was scanned. Choose another file.")
                    .foregroundStyle(Tone.error.text)
            }
        case .ready(let estimate):
            Card {
                Text(estimate.human.localized())
                    .font(.largeTitle.weight(.bold))
                    .accessibilityAddTraits(.isHeader)
                Text("\(estimate.framesAnalyzed.formatted()) frames · faces looked for at \(estimate.detectionLongSide) px")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                if let banner = estimate.coverageBanner {
                    LineLabel(line: banner, systemImage: "exclamationmark.triangle", tone: .incomplete)
                        .font(.subheadline)
                }
                ForEach(estimate.notes, id: \.self) { note in
                    LineLabel(line: note, systemImage: noteSymbol(note.code))
                        .font(.subheadline)
                }
                Text(estimate.caveat.localized())
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

/// A Missing or Wanted chip that turns a class on or off.
private struct ClassToggle: View {
    let title: String
    let count: Int?
    let isOn: Bool
    let toggle: () -> Void

    var body: some View {
        Button(action: toggle) {
            HStack(spacing: 6) {
                Image(systemName: isOn ? "checkmark.circle.fill" : "circle")
                    .foregroundStyle(isOn ? Color.accentColor : Color.secondary)
                Text(title)
                if let count {
                    Text(count.formatted())
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                }
            }
            .font(.body.weight(.medium))
            .padding(.horizontal, 14)
            .padding(.vertical, 8)
            .background(Capsule().fill(isOn ? Color.accentColor.opacity(0.14) : Color.secondary.opacity(0.1)))
            .overlay(Capsule().strokeBorder(isOn ? Color.accentColor.opacity(0.6) : Color.clear))
        }
        .buttonStyle(.plain)
        .accessibilityLabel(count.map { String(localized: "\(title), \($0) posters") } ?? title)
        .accessibilityAddTraits(isOn ? .isSelected : [])
    }
}

/// One choice in a list, with a checkmark when selected.
private struct ChoiceRow<Content: View>: View {
    let selected: Bool
    let action: () -> Void
    @ViewBuilder var content: Content

    var body: some View {
        Button(action: action) {
            HStack(alignment: .firstTextBaseline, spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    content
                }
                .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
                Image(systemName: "checkmark")
                    .font(.body.weight(.semibold))
                    .foregroundStyle(Color.accentColor)
                    .opacity(selected ? 1 : 0)
                    .accessibilityHidden(true)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

/// A Quick Look thumbnail, made off the main thread.
private struct Thumbnail: View {
    let url: URL
    let side: CGFloat
    @Environment(\.displayScale) private var scale
    @State private var image: LoadedImage?

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 8, style: .continuous).fill(.quaternary)
            if let image {
                Image(decorative: image.cgImage, scale: 1)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
            } else {
                Image(systemName: "photo").foregroundStyle(.secondary)
            }
        }
        .frame(width: side, height: side)
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        .accessibilityHidden(true)
        .task(id: url) {
            let request = QLThumbnailGenerator.Request(fileAt: url, size: CGSize(width: side, height: side), scale: scale, representationTypes: .thumbnail)
            if let made = try? await QLThumbnailGenerator.shared.generateBestRepresentation(for: request) {
                image = LoadedImage(cgImage: made.cgImage)
            }
        }
    }
}
