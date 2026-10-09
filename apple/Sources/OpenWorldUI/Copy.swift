// SPDX-License-Identifier: Apache-2.0
import Foundation

public enum Copy {
    public static let possible = "Possible candidate. Not an identification."
    public static let notCompared = "Not compared."
    public static let incomplete = "Incomplete."
    public static let clearance = "No candidate is not a clearance."
    public static let leaving = "You are leaving OpenWorld."
    public static let brief = "A brief face can be missed."
    public static let onDevice = "This file stays on this device."
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
