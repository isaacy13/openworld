// SPDX-License-Identifier: Apache-2.0
import Foundation

enum Copy {
    static let possible = "Possible candidate. Not an identification."
    static let notCompared = "Not compared."
    static let incomplete = "Incomplete."
    static let clearance = "No candidate is not a clearance."
    static let leaving = "You are leaving OpenWorld."
    static let brief = "A brief face can be missed."
    static let onDevice = "This file stays on this device."
    static let oldFile = "This file is older than about 30 days."
    static let disclosure = [
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
