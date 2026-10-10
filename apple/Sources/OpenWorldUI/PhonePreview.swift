// SPDX-License-Identifier: Apache-2.0
import Foundation
import OpenWorldContract

/// The words the Linux phone preview prints, in the same order as the phone column.
/// Every line is kept. A short list used to drop the size rules and Analyze.
@MainActor
public enum PhonePreview {
    public static func lines(screen: String, model: FlowModel) -> [String] {
        var lines = column(screen: screen, model: model)
        if model.showsTopError, let error = model.error {
            lines.insert(error, at: 0)
        }
        return lines
    }

    /// The page itself. A refusal the page does not already print is placed above these lines.
    private static func column(screen: String, model: FlowModel) -> [String] {
        switch screen {
        case "device":
            var lines = ["Back", Copy.onDevice, model.file?.lastPathComponent ?? ""]
            if model.oldFile { lines.append(Copy.oldFile) }
            lines.append(contentsOf: Copy.disclosure)
            lines.append("Fixture posters. Real FBI photos stay off.")
            lines.append("Continue")
            return lines
        case "bundle":
            var lines = ["Back", model.bundleHeadline]
            if !model.bundles.isEmpty {
                lines.append("Scores are not comparable across bundles. Results name the bundle you pick.")
                for row in model.bundles {
                    lines.append(markedChoice(row.name, selected: row.id == model.bundleID))
                    lines.append(row.bestFor)
                    lines.append(row.curveLine)
                }
            }
            lines.append("Continue")
            return lines
        case "size":
            var lines = ["Back", "Detection size", Copy.sizeHint]
            for value in ["320", "480", "640", "full"] {
                lines.append(detectionChoice(value, selected: model.longSide))
            }
            lines.append("Coverage")
            lines.append(coverageChoice("complete", selected: model.coverage))
            lines.append(coverageChoice("measured", selected: model.coverage))
            if model.coverage == "measured" {
                lines.append(Copy.brief)
            }
            lines.append("Continue")
            return lines
        case "estimate":
            var lines = ["Back", model.estimateHeadline]
            if model.scanning, !model.liveCrops.isEmpty {
                lines.append("Crops from this file.")
                for crop in model.liveCrops {
                    lines.append(crop.frameLabel)
                    lines.append(crop.label)
                }
            }
            if let estimate = model.estimate {
                if let warning = model.estimateWarning { lines.append(warning) }
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
            lines.append(contentsOf: contextLines(model.report))
            lines.append(contentsOf: model.report?.warnings ?? [])
            for candidate in model.report?.candidates ?? [] {
                lines.append(candidate.wording)
                if candidate.crop?.isEmpty == false { lines.append("Crop") }
                if candidate.frame?.isEmpty == false { lines.append(candidate.frameLabel) }
                if !candidate.uncertainty.isEmpty { lines.append(candidate.uncertainty) }
                if !candidate.posterLine.isEmpty { lines.append(candidate.posterLine) }
                if !candidate.fbiUrl.isEmpty { lines.append("Open FBI page") }
            }
            if let items = model.report?.inventory, !items.isEmpty {
                lines.append("Crops from this file.")
                for item in items {
                    lines.append(item.frameLabel)
                    lines.append(item.label)
                }
            }
            lines.append(contentsOf: disclosureLines(model))
            if let notice = model.leaveError { lines.append(notice) }
            if model.resultDirectory != nil {
                lines.append("Delete")
            }
            if let notice = model.deleteNotice { lines.append(notice) }
            lines.append("Choose another file")
            if screen == "leaving" {
                lines.append(Copy.leaving)
                if let page = model.leavingURL?.absoluteString { lines.append(page) }
                lines.append("Open")
                lines.append("Stay")
            }
            return lines
        default:
            return [
                "Choose a photo or video",
                Copy.importHint(phone: model.phone),
                "Choose File",
            ]
        }
    }

    /// Disclosure from the report. A refusal that never became a report still names what stays on the device, and it leaves out the clearance sentence.
    public static func disclosureLines(_ model: FlowModel) -> [String] {
        if let report = model.report {
            return report.disclosure
        }
        if let error = model.error, error.contains("Refusing.") {
            return Copy.disclosure.filter { $0 != Copy.clearance }
        }
        return []
    }

    /// Lines under the headline. A refusal leaves the bundle and the perception line empty, so those stay off the screen.
    public static func contextLines(_ report: ScanReport?) -> [String] {
        guard let report else { return [] }
        var lines: [String] = []
        func keep(_ value: String?) {
            if let value, !value.isEmpty { lines.append(value) }
        }
        keep(report.coverageBanner)
        keep(report.framesNote)
        keep(report.classNote)
        if let name = report.bundleName, !name.isEmpty {
            lines.append("Bundle: \(name)")
        }
        keep(report.detectionNote)
        keep(report.coverageNote)
        keep(report.perceptionNote)
        return lines
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
