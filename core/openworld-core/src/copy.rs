// SPDX-License-Identifier: Apache-2.0

/// Words the shells must show. A candidate is not an identification.
pub const POSSIBLE_CANDIDATE: &str = "Possible candidate. Not an identification.";
pub const NOT_COMPARED: &str = "Not compared.";
pub const INCOMPLETE: &str = "Incomplete.";
pub const NO_CLEARANCE: &str = "No candidate is not a clearance.";
pub const LEAVING: &str = "You are leaving OpenWorld.";
pub const BRIEF_FACE: &str = "A brief face can be missed.";
pub const VEHICLE_NOT_PERSON: &str = "A vehicle is not a person.";
pub const BELOW_CUTOFF: &str = "Below the locked cutoff. Not a candidate.";
pub const PLATE_NOT_ON_POSTER: &str = "This plate text is not published on a poster.";
pub const PLATE_UNREAD: &str = "The plate could not be read.";

pub const DISCLOSURE: &[&str] = &[
    "Nothing is uploaded.",
    "Nobody is enrolled.",
    "OpenWorld does not train on this file.",
    "OpenWorld does not contact an agency.",
    "A candidate is not an identification.",
    NO_CLEARANCE,
    "This file is not authenticated.",
    "On-device does not mean the file is real.",
];

pub const ON_DEVICE: &str = "This file stays on this device.";
pub const OLD_FILE: &str = "This file is older than about 30 days.";
pub const TIMESTAMP_DISAGREE: &str = "The file timestamps disagree.";
pub const CPU_NOTE: &str =
    "This scan runs on the CPU. It will be slower, warmer, and use more battery.";
pub const GPU_NOTE: &str = "This scan runs on the GPU and may warm the device.";
pub const HEAT_NOTE: &str =
    "This phone may get hot. If heat or the system stops the scan, the result is Incomplete.";
pub const BATTERY_NOTE: &str = "A long scan uses a lot of battery.";
pub const SUGGEST_COMPUTER: &str =
    "A computer will finish this sooner. You can still run a complete scan on this phone.";
pub const NOT_MEASURED: &str = "Not measured yet.";
pub const ESTIMATE_CAVEAT: &str = "This is a planning estimate, not a thermal measurement.";

pub fn product_copy() -> serde_json::Value {
    serde_json::json!({
        "possible_candidate": POSSIBLE_CANDIDATE,
        "not_compared": NOT_COMPARED,
        "incomplete": INCOMPLETE,
        "no_clearance": NO_CLEARANCE,
        "leaving": LEAVING,
        "brief_face": BRIEF_FACE,
        "vehicle_not_person": VEHICLE_NOT_PERSON,
        "below_cutoff": BELOW_CUTOFF,
        "plate_not_on_poster": PLATE_NOT_ON_POSTER,
        "plate_unread": PLATE_UNREAD,
        "disclosure": DISCLOSURE,
        "on_device": ON_DEVICE,
        "old_file": OLD_FILE,
        "timestamps_disagree": TIMESTAMP_DISAGREE,
        "cpu_note": CPU_NOTE,
        "gpu_note": GPU_NOTE,
        "heat_note": HEAT_NOTE,
        "battery_note": BATTERY_NOTE,
        "suggest_computer": SUGGEST_COMPUTER,
        "not_measured": NOT_MEASURED,
        "estimate_caveat": ESTIMATE_CAVEAT,
    })
}
