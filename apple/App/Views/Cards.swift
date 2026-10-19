// SPDX-License-Identifier: Apache-2.0
import OpenWorldKit
import SwiftUI

/// A possible candidate: the crop from the file beside the poster photo, the poster's
/// title and class, the band, and the way to its FBI page. No score.
struct CandidateCard: View {
    let candidate: Candidate
    let crop: URL?
    let showFrame: () -> Void
    let open: () -> Void

    var body: some View {
        Card(tone: .candidate) {
            Label(candidate.headline.localized(), systemImage: "person.crop.square")
                .font(.headline)
                .foregroundStyle(Tone.candidate.text)
            HStack(alignment: .top, spacing: 12) {
                evidence(String(localized: "From your file"), url: crop)
                evidence(String(localized: "Poster"), url: candidate.poster.image.map { URL(fileURLWithPath: $0) })
            }
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(candidate.poster.title)
                    .font(.title3.weight(.semibold))
                    .fixedSize(horizontal: false, vertical: true)
                Chip(text: candidate.poster.classLabel, tone: .neutral)
            }
            LineLabel(line: candidate.band, systemImage: "ruler")
                .font(.subheadline)
            HStack {
                Text(candidate.frameLabel.localized())
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Spacer()
                Button("Show frame", action: showFrame)
                    .disabled(candidate.frame == nil)
            }
            Button(action: open) {
                Label("Open FBI page", systemImage: "arrow.up.forward.square")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .controlSize(.large)
            .disabled(candidate.poster.fbiUrl == nil)
        }
    }

    private func evidence(_ title: String, url: URL?) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            FileImage(url: url, maxPixels: 800)
                .aspectRatio(1, contentMode: .fit)
                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            Text(title)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text("\(title): \(candidate.poster.title)"))
    }
}

/// A face on the scan screen or in "Other faces": its crop and what was decided.
struct FaceCell: Identifiable {
    let id: UInt64
    let crop: URL?
    let version: Int
    let frameNumber: Int
    let status: TrackItem.Status?
    let label: String
    let reason: String?

    init(tile: FaceTile) {
        id = tile.id
        crop = tile.crop
        version = tile.version
        frameNumber = tile.frameLabel.int("frame") ?? tile.frameLabel.int("first") ?? Int(tile.frame) + 1
        status = tile.track?.status
        label = tile.track?.label.localized() ?? String(localized: "Looking")
        reason = tile.track?.reason?.localized()
    }

    init(track: TrackItem, crop: URL?) {
        id = track.trackId
        self.crop = crop
        version = 0
        frameNumber = Int(track.firstFrame) + 1
        status = track.status
        label = track.label.localized()
        reason = track.reason?.localized()
    }
}

/// Faces as they are found. Each fades in, unless Reduce Motion is on.
struct FaceGrid: View {
    let cells: [FaceCell]
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @ScaledMetric private var side: CGFloat = 96

    var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: side, maximum: side * 1.6), spacing: 12, alignment: .top)], alignment: .leading, spacing: 14) {
            ForEach(cells) { cell in
                FaceCellView(cell: cell)
                    .transition(reduceMotion ? .identity : .opacity)
            }
        }
        .animation(reduceMotion ? nil : .easeOut(duration: 0.3), value: cells.map(\.id))
    }
}

private struct FaceCellView: View {
    let cell: FaceCell

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            FileImage(url: cell.crop, version: cell.version)
                .aspectRatio(1, contentMode: .fit)
                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                .overlay {
                    if cell.status == .candidate {
                        RoundedRectangle(cornerRadius: 8, style: .continuous)
                            .strokeBorder(Tone.candidate.text, lineWidth: 2)
                    }
                }
            Chip(text: cell.label, systemImage: cell.status?.symbol ?? "magnifyingglass", tone: cell.status?.tone)
                .font(.caption)
            if let reason = cell.reason {
                Text(reason)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text("Face from frame \(cell.frameNumber). \(cell.label)") + Text(cell.reason.map { " \($0)" } ?? ""))
    }
}
