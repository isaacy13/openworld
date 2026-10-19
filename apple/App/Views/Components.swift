// SPDX-License-Identifier: Apache-2.0
import ImageIO
import OpenWorldKit
import SwiftUI

/// The color tokens of docs/ux.md, from the asset catalog, with light and dark values.
/// Status is never shown by color alone: each tone comes with an icon and words.
enum Tone {
    case candidate, incomplete, neutral, error, demo

    private var name: String {
        switch self {
        case .candidate: return "Candidate"
        case .incomplete: return "Incomplete"
        case .neutral: return "Neutral"
        case .error: return "Error"
        case .demo: return "Demo"
        }
    }

    var text: Color { Color("\(name)Text") }
    var fill: Color { Color("\(name)Fill") }
}

enum Metrics {
    #if os(iOS)
    static let gutter: CGFloat = 16
    #else
    static let gutter: CGFloat = 24
    #endif
    static let corner: CGFloat = 12
    static let readable: CGFloat = 720
}

extension Color {
    static var page: Color {
        #if os(iOS)
        Color(uiColor: .systemGroupedBackground)
        #else
        Color(nsColor: .windowBackgroundColor)
        #endif
    }

    static var card: Color {
        #if os(iOS)
        Color(uiColor: .secondarySystemGroupedBackground)
        #else
        Color(nsColor: .controlBackgroundColor)
        #endif
    }
}

/// A 12 pt card. A tone tints it and draws its edge.
struct Card<Content: View>: View {
    var tone: Tone?
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .background(tone?.fill ?? Color.card, in: RoundedRectangle(cornerRadius: Metrics.corner, style: .continuous))
        .overlay {
            if let tone {
                RoundedRectangle(cornerRadius: Metrics.corner, style: .continuous)
                    .strokeBorder(tone.text.opacity(0.35), lineWidth: 1)
            }
        }
    }
}

/// A scrolling column at a readable width, with the platform's gutters.
struct Page<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                content
            }
            .padding(.horizontal, Metrics.gutter)
            .padding(.vertical, 16)
            .frame(maxWidth: Metrics.readable)
            .frame(maxWidth: .infinity)
        }
        .background(Color.page)
    }
}

/// A core sentence with an icon, in a tone.
struct LineLabel: View {
    let line: Line
    let systemImage: String
    var tone: Tone?

    var body: some View {
        Label {
            Text(line.localized())
                .fixedSize(horizontal: false, vertical: true)
        } icon: {
            Image(systemName: systemImage)
        }
        .foregroundStyle(tone?.text ?? Color.primary)
    }
}

/// A small capsule: a trust chip, a class badge, a face status.
struct Chip: View {
    let text: String
    var systemImage: String?
    var tone: Tone?

    var body: some View {
        HStack(spacing: 4) {
            if let systemImage {
                Image(systemName: systemImage)
                    .imageScale(.small)
                    .accessibilityHidden(true)
            }
            Text(text)
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
        }
        .font(.subheadline)
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .foregroundStyle(tone?.text ?? Color.primary)
        .background(Capsule().fill(tone?.fill ?? Color.secondary.opacity(0.12)))
        .accessibilityElement(children: .combine)
    }
}

/// Lays chips out in rows that wrap.
struct FlowLayout: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        var x: CGFloat = 0
        var y: CGFloat = 0
        var row: CGFloat = 0
        var widest: CGFloat = 0
        for view in subviews {
            let size = fitted(view, width)
            if x > 0, x + size.width > width {
                x = 0
                y += row + spacing
                row = 0
            }
            x += size.width + spacing
            row = max(row, size.height)
            widest = max(widest, x - spacing)
        }
        return CGSize(width: proposal.width ?? widest, height: y + row)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX
        var y = bounds.minY
        var row: CGFloat = 0
        for view in subviews {
            let size = fitted(view, bounds.width)
            if x > bounds.minX, x + size.width > bounds.maxX {
                x = bounds.minX
                y += row + spacing
                row = 0
            }
            view.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
            x += size.width + spacing
            row = max(row, size.height)
        }
    }

    private func fitted(_ view: LayoutSubview, _ width: CGFloat) -> CGSize {
        let ideal = view.sizeThatFits(.unspecified)
        guard ideal.width > width, width.isFinite else { return ideal }
        return view.sizeThatFits(ProposedViewSize(width: width, height: nil))
    }
}

/// A picture file, loaded off the main thread. `version` reloads a rewritten crop.
struct FileImage: View {
    let url: URL?
    var version = 0
    var contentMode: ContentMode = .fill
    var maxPixels = 600
    @State private var image: LoadedImage?

    var body: some View {
        ZStack {
            Rectangle().fill(.quaternary)
            if let image {
                Image(decorative: image.cgImage, scale: 1)
                    .resizable()
                    .aspectRatio(contentMode: contentMode)
            } else {
                Image(systemName: "person.crop.square")
                    .font(.title2)
                    .foregroundStyle(.secondary)
            }
        }
        .clipped()
        .task(id: "\(url?.path ?? "")#\(version)") {
            image = await LoadedImage.load(url, maxPixels: maxPixels)
        }
    }
}

/// A decoded picture that crosses from the loading task to the view.
struct LoadedImage: @unchecked Sendable {
    let cgImage: CGImage

    static func load(_ url: URL?, maxPixels: Int) async -> LoadedImage? {
        guard let url else { return nil }
        return await Task.detached(priority: .utility) { () -> LoadedImage? in
            guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
            let options: [CFString: Any] = [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: maxPixels,
            ]
            return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary).map(LoadedImage.init)
        }.value
    }
}

/// How a result looks, from its status: no green, no alarm. Amber for a candidate,
/// grey for no candidate, orange for Incomplete, red only for a refusal.
extension Report {
    var tone: Tone {
        switch status {
        case .refused: return .error
        case .incomplete: return .incomplete
        case .complete: return candidates.isEmpty ? .neutral : .candidate
        }
    }

    var symbol: String {
        switch tone {
        case .candidate: return "person.crop.square"
        case .incomplete: return "exclamationmark.triangle"
        case .error: return "xmark.octagon"
        case .neutral, .demo: return "minus.circle"
        }
    }
}

extension TrackItem.Status {
    var tone: Tone { self == .candidate ? .candidate : .neutral }

    var symbol: String {
        switch self {
        case .candidate: return "person.crop.square"
        case .notCompared, .unscored: return "minus.circle"
        case .belowCutoff, .tooBrief: return "circle"
        }
    }
}

/// Icons for the estimate's device notes, by code.
func noteSymbol(_ code: String) -> String {
    switch code {
    case "cpu_note", "gpu_note": return "cpu"
    case "heat_note", "already_warm": return "thermometer.medium"
    case "battery_note": return "battery.25"
    case "suggest_computer": return "laptopcomputer"
    default: return "info.circle"
    }
}
