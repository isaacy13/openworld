// SPDX-License-Identifier: Apache-2.0
import Foundation
import OpenWorldKit
#if os(iOS)
import UIKit
#endif

/// What the device says about itself, for the estimate and the stop rules.
@MainActor
enum DeviceState {
    static func facts() -> DeviceFacts {
        #if os(iOS)
        let device = UIDevice.current
        device.isBatteryMonitoringEnabled = true
        let level = device.batteryLevel
        let charging: Bool?
        switch device.batteryState {
        case .charging, .full: charging = true
        case .unplugged: charging = false
        default: charging = nil
        }
        return DeviceFacts(kind: "phone", thermal: thermal(), battery: level >= 0 ? Double(level) : nil, charging: charging)
        #else
        // A computer. Heat and battery stops are a phone rule (docs/ux.md).
        return .computer
        #endif
    }

    static func thermal() -> String {
        switch ProcessInfo.processInfo.thermalState {
        case .nominal: return "nominal"
        case .fair: return "fair"
        case .serious: return "serious"
        case .critical: return "critical"
        @unknown default: return "nominal"
        }
    }
}

/// Keeps the screen on while any window scans, and lets a stopped scan write its report
/// after the app leaves the screen.
@MainActor
final class ScanKeeper {
    static let shared = ScanKeeper()
    private var scanning = 0
    #if os(iOS)
    private var task: UIBackgroundTaskIdentifier = .invalid
    #endif

    func began() {
        scanning += 1
        update()
    }

    func ended() {
        scanning = max(0, scanning - 1)
        update()
    }

    /// Just long enough to finish the report of a scan that was stopped for the background.
    func enteredBackground() {
        #if os(iOS)
        guard scanning > 0, task == .invalid else { return }
        task = UIApplication.shared.beginBackgroundTask(withName: "Finish the OpenWorld report") {
            MainActor.assumeIsolated { ScanKeeper.shared.endBackgroundTask() }
        }
        #endif
    }

    private func update() {
        #if os(iOS)
        UIApplication.shared.isIdleTimerDisabled = scanning > 0
        if scanning == 0 { endBackgroundTask() }
        #endif
    }

    private func endBackgroundTask() {
        #if os(iOS)
        if task != .invalid {
            UIApplication.shared.endBackgroundTask(task)
            task = .invalid
        }
        #endif
    }
}
