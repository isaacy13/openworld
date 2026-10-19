// SPDX-License-Identifier: Apache-2.0
import Foundation

/// The core's fixed sentences by code (`{"v":1,"op":"copy"}`).
public struct CopyTable: Equatable, Sendable {
    public var texts: [String: String]

    public init(texts: [String: String] = [:]) {
        self.texts = texts
    }

    public func line(_ code: String) -> Line {
        Line(code: code, text: texts[code] ?? code)
    }

    /// The disclosure in the core's order (copy::DISCLOSURE). Shown before Analyze; a
    /// finished report carries its own.
    public static let disclosureCodes = [
        "on_device", "nothing_uploaded", "nobody_enrolled", "no_training", "no_agency",
        "not_identification", "no_clearance", "unauthenticated", "not_real_proof",
    ]

    public var disclosure: [Line] {
        Self.disclosureCodes.map(line)
    }
}

/// The file's age warnings before Analyze. Same rules as the core's `media_warnings`,
/// which reports them only once a scan runs: older than about 30 days, or the modified
/// time and the recording time more than a day apart.
public enum FileWarnings {
    public static func lines(modified: Date?, recorded: Date?, now: Date, copy: CopyTable) -> [Line] {
        guard let modified else { return [] }
        var lines: [Line] = []
        if now.timeIntervalSince(modified) > 30 * 24 * 3600 {
            lines.append(copy.line("old_file"))
        }
        if let recorded, abs(modified.timeIntervalSince(recorded)) > 24 * 3600 {
            lines.append(copy.line("timestamps_disagree"))
        }
        return lines
    }
}

/// When the device itself ends a scan: serious heat, or a low battery that is not charging.
public enum StopPolicy {
    public static let lowBattery = 0.10

    public static func reason(for device: DeviceFacts) -> StopReason? {
        if device.thermal == "serious" || device.thermal == "critical" {
            return .heat
        }
        if let level = device.battery, level >= 0, level < lowBattery, device.charging == false {
            return .battery
        }
        return nil
    }
}
