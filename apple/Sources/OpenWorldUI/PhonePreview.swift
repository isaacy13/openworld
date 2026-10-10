// SPDX-License-Identifier: Apache-2.0
import Foundation

/// The words the Linux phone preview prints, in the same order as the phone column.
/// Every line is kept. A short list used to drop the size rules and Analyze.
@MainActor
public enum PhonePreview {
    public static func lines(screen: String, model: FlowModel) -> [String] {
        switch screen {
        case "device":
            var lines = ["Back", Copy.onDevice, model.file?.lastPathComponent ?? ""]
            if model.oldFile { lines.append(Copy.oldFile) }
            lines.append(contentsOf: Copy.disclosure)
            lines.append("Fixture posters. Real FBI photos stay off.")
            lines.append("Continue")
            return lines
        case "bundle":
            var lines = [
                "Back",
                "Model bundle",
                "Scores are not comparable across bundles. Results name the bundle you pick.",
            ]
            for row in model.bundles {
                lines.append(markedChoice(row.name, selected: row.id == model.bundleID))
                lines.append(row.bestFor)
                lines.append(row.curveLine)
            }
            if model.bundles.isEmpty, let error = model.error {
                lines.append(error)
            }
            lines.append("Continue")
            return lines
        case "size":
            var lines = ["Back", "Detection size", Copy.sizeHint]
            for (value, title) in [
                ("320", "320 px on the long side"),
                ("480", "480 px on the long side"),
                ("640", "640 px on the long side"),
                ("full", "Full resolution"),
            ] {
                lines.append(markedChoice(title, selected: model.longSide == value))
            }
            lines.append(markedChoice("Complete. Every decoded frame.", selected: model.coverage == "complete"))
            lines.append(markedChoice("Measured. 5 frames a second, plus the tracker.", selected: model.coverage == "measured"))
            if model.coverage == "measured" {
                lines.append(Copy.brief)
            }
            lines.append("Continue")
            return lines
        case "estimate":
            var lines = ["Back", model.scanning ? "Scanning" : "Estimate"]
            if model.scanning, !model.liveCrops.isEmpty {
                lines.append("Crops from this file.")
                lines.append(contentsOf: model.liveCrops.map(\.label))
            }
            if let warning = model.estimateWarning { lines.append(warning) }
            if let estimate = model.estimate {
                lines.append(estimate.human)
                lines.append(estimate.caveat)
                if let note = estimate.deviceNote { lines.append(note) }
                if let note = estimate.heatNote { lines.append(note) }
                if let note = estimate.batteryNote { lines.append(note) }
                if let note = estimate.suggestComputerText { lines.append(note) }
                lines.append(model.choiceLine)
                if model.showBriefOnEstimate {
                    lines.append(Copy.brief)
                }
                lines.append(model.classLine)
                lines.append(markedChoice("Missing", selected: model.includeMissing))
                lines.append(markedChoice("Wanted", selected: model.includeWanted))
            } else if let error = model.error {
                lines.append(error)
            }
            if model.showsAnalyze {
                lines.append(model.scanning ? "Scanning" : "Analyze")
            }
            if let notice = model.deleteNotice {
                lines.append(notice)
            }
            return lines
        case "results", "leaving":
            var lines = ["Back", model.report?.summary ?? model.error ?? Copy.incomplete]
            if model.report?.status == "incomplete",
               let reason = model.report?.message,
               !reason.isEmpty,
               reason != model.report?.summary {
                lines.append(reason)
            }
            if let banner = model.report?.coverageBanner { lines.append(banner) }
            if let frames = model.report?.framesNote { lines.append(frames) }
            if let classes = model.report?.classNote { lines.append(classes) }
            if let name = model.report?.bundleName { lines.append("Bundle: \(name)") }
            if let size = model.report?.detectionNote { lines.append(size) }
            if let coverage = model.report?.coverageNote { lines.append(coverage) }
            if let note = model.report?.perceptionNote { lines.append(note) }
            lines.append(contentsOf: model.report?.warnings ?? [])
            for candidate in model.report?.candidates ?? [] {
                lines.append(candidate.wording)
                if candidate.crop?.isEmpty == false { lines.append("Crop") }
                if candidate.frame?.isEmpty == false { lines.append(candidate.frameLabel) }
                if !candidate.uncertainty.isEmpty { lines.append(candidate.uncertainty) }
                lines.append(candidate.posterLine)
                lines.append("Open FBI page")
            }
            if let items = model.report?.inventory, !items.isEmpty {
                lines.append("Crops from this file.")
                for item in items {
                    lines.append(item.frameLabel)
                    lines.append(item.label)
                }
            }
            lines.append(contentsOf: model.report?.disclosure ?? [])
            if let notice = model.leaveError { lines.append(notice) }
            if model.resultDirectory != nil {
                lines.append("Delete")
            }
            if let notice = model.deleteNotice { lines.append(notice) }
            lines.append("Choose another file")
            if screen == "leaving" {
                lines.append(Copy.leaving)
                if let page = model.leavingURL?.absoluteString { lines.append(page) }
                lines.append("Stay")
            }
            return lines
        default:
            var lines = [
                "Choose a photo or video",
                "Import a file you already have. There is no camera.",
                "Choose File",
            ]
            if let error = model.error { lines.append(error) }
            return lines
        }
    }

    /// Wrapped the way the terminal preview prints a phone column. Nothing is dropped.
    public static func wrapped(screen: String, model: FlowModel, width: Int = 34) -> [String] {
        lines(screen: screen, model: model).flatMap { wrap($0, width: width) }
    }

    static func wrap(_ text: String, width: Int) -> [String] {
        if text.isEmpty { return [""] }
        var lines: [String] = []
        var line = ""
        for word in text.split(separator: " ") {
            let word = String(word)
            if line.isEmpty {
                line = word
            } else if line.count + 1 + word.count <= width {
                line += " " + word
            } else {
                lines.append(line)
                line = word
            }
        }
        if !line.isEmpty { lines.append(line) }
        return lines
    }
}
