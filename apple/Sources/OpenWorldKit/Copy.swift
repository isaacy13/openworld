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
