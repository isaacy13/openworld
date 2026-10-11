// SPDX-License-Identifier: Apache-2.0
import Foundation

public enum Copy {
    public static let possible = "Possible candidate. Not an identification."
    public static let notCompared = "Not compared."
    public static let incomplete = "Incomplete."
    public static let clearance = "No candidate is not a clearance."
    public static let leaving = "You are leaving OpenWorld."
    public static let brief = "A brief face can be missed."
    public static let sizeHint = "Smaller frames are a resize of each decoded frame in memory. A face under 64 px on that image is left out. Evidence crops come from the original frame. If that crop is under 112 px on the short side, the label is \"Not compared.\" Full resolution is slower."
    public static let phoneImport = "Import a file you already have, or receive it from the share sheet. There is no camera."
    public static let computerImport = "Import a file you already have. Drop it here, or use Choose File. There is no camera."
    public static let onDevice = "This file stays on this device."

    /// The phone receives a file from the share sheet. The Mac window also accepts a drop.
    public static func importHint(phone: Bool) -> String {
        phone ? phoneImport : computerImport
    }
    public static let oldFile = "This file is older than about 30 days."
    public static let disagree = "The file timestamps disagree."
    public static let disclosure = [
        "Nothing is uploaded.",
        "Nobody is enrolled.",
        "OpenWorld does not train on this file.",
        "OpenWorld does not contact an agency.",
        "A candidate is not an identification.",
        clearance,
        "This file is not authenticated.",
        "On-device does not mean the file is real.",
    ]
}
