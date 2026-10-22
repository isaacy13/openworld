// SPDX-License-Identifier: Apache-2.0
import Foundation

/// A file the person chose, copied into the app's imports folder.
public struct ImportedFile: Equatable, Hashable, Sendable {
    public enum Origin: String, Sendable {
        case photos, files, share, drop, open
    }

    /// Our copy. The original is never changed.
    public var url: URL
    /// The name the person knows it by.
    public var name: String
    /// The original's modified time, when it was known. A Photos pick has none.
    public var modified: Date?
    public var origin: Origin

    public init(url: URL, name: String, modified: Date?, origin: Origin) {
        self.url = url
        self.name = name
        self.modified = modified
        self.origin = origin
    }
}

public enum MediaKind: String, Sendable {
    case photo, animation, video
}

/// What the platform decoder learned about a file before Analyze.
public struct ProbedMedia: Sendable {
    public var facts: MediaFacts
    public var kind: MediaKind
    /// When it was recorded (EXIF or the container's creation time).
    public var recorded: Date?
    /// The frame count is duration times rate, not counted from the file. A wrong count
    /// makes the scan Incomplete, never a silent pass.
    public var countIsEstimate: Bool
    public var plan: any DecodePlan

    public init(facts: MediaFacts, kind: MediaKind, recorded: Date?, countIsEstimate: Bool = false, plan: any DecodePlan) {
        self.facts = facts
        self.kind = kind
        self.recorded = recorded
        self.countIsEstimate = countIsEstimate
        self.plan = plan
    }
}

/// A file the platform cannot decode. Each maps to the core's refusal sentence.
public enum ProbeFailure: Error, Equatable, Sendable {
    case unreadable
    case badCodec
    case tooLarge

    public var code: String {
        switch self {
        case .unreadable: return "unreadable"
        case .badCodec: return "bad_codec"
        case .tooLarge: return "too_large"
        }
    }
}

public protocol MediaReader: Sendable {
    func probe(_ file: ImportedFile) async throws -> ProbedMedia
}

/// Frame timing of an animated image (GIF, APNG, WebP, HEICS) from its per-frame delays.
public enum AnimationTiming {
    /// A delay this short is drawn at 0.1 s, as browsers and ImageIO's clamped delay do.
    public static func delay(_ seconds: Double?) -> Double {
        guard let seconds, seconds.isFinite, seconds > 0.01 else { return 0.1 }
        return seconds
    }

    /// Frames a second over the whole animation, and its length.
    public static func rate(delays: [Double?]) -> (fps: Double, duration: Double) {
        let total = delays.map(delay).reduce(0, +)
        guard !delays.isEmpty, total > 0 else { return (0, 0) }
        return (Double(delays.count) / total, total)
    }
}

/// The largest picture the core takes (decode::MAX_PIXELS). A bigger one is refused
/// before its pixels are allocated.
public let maxPixels = 120_000_000

/// EXIF `DateTimeOriginal` ("2024:05:01 14:03:22") with its `OffsetTimeOriginal`
/// ("+02:00"). Without an offset the time is read in `zone`.
public enum ExifDate {
    public static func parse(_ text: String, offset: String? = nil, zone: TimeZone = .current) -> Date? {
        let parts = text.trimmingCharacters(in: .whitespacesAndNewlines).split(whereSeparator: { $0 == ":" || $0 == " " })
        guard parts.count >= 6 else { return nil }
        let numbers = parts.prefix(6).compactMap { Int($0) }
        guard numbers.count == 6, numbers[0] > 1900 else { return nil }
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = offset.flatMap(Self.zone(fromOffset:)) ?? zone
        let components = DateComponents(year: numbers[0], month: numbers[1], day: numbers[2], hour: numbers[3], minute: numbers[4], second: numbers[5])
        guard let date = calendar.date(from: components),
              calendar.component(.month, from: date) == numbers[1],
              calendar.component(.day, from: date) == numbers[2] else { return nil }
        return date
    }

    static func zone(fromOffset text: String) -> TimeZone? {
        let trimmed = text.trimmingCharacters(in: .whitespaces)
        guard let sign = trimmed.first, sign == "+" || sign == "-" else { return nil }
        let digits = trimmed.dropFirst().split(separator: ":")
        guard digits.count == 2, let hours = Int(digits[0]), let minutes = Int(digits[1]), hours <= 14, minutes < 60 else { return nil }
        let seconds = (hours * 3600 + minutes * 60) * (sign == "-" ? -1 : 1)
        return TimeZone(secondsFromGMT: seconds)
    }
}

/// Which YCbCr matrix 8-bit video uses, from the buffer's or track's color tags
/// (the string values of `kCVImageBufferYCbCrMatrix_*` and `kCVImageBufferColorPrimaries_*`).
public enum ColorMatrix {
    public static func isBT709(matrix: String?, primaries: String?, height: Int) -> Bool {
        switch matrix {
        case "ITU_R_709_2": return true
        case "ITU_R_601_4", "SMPTE_240M_1995": return false
        // BT.2020 video comes out of the 8-bit decoder close to BT.709; the core knows only 601 and 709.
        case "ITU_R_2020": return true
        default: break
        }
        switch primaries {
        case "EBU_3213", "SMPTE_C": return false
        case "ITU_R_709_2", "ITU_R_2020", "P3_D65", "DCI_P3": return true
        default: return height >= 720
        }
    }
}
