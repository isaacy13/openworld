// SPDX-License-Identifier: Apache-2.0

//! Every sentence a person reads, by code.
//!
//! The core returns a [`Line`]: a stable code, the English text, and any
//! values the sentence names. Shells translate by code and fall back to the
//! text. Shells never compare text to decide what to do; they read codes.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub code: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub args: Map<String, Value>,
}

impl Line {
    pub fn of((code, text): Copy) -> Self {
        Self { code: code.into(), text: text.into(), args: Map::new() }
    }

    /// A sentence with values. `text` is the English sentence with those values in it.
    pub fn with(code: &str, text: String, args: &[(&str, Value)]) -> Self {
        let args = args.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        Self { code: code.into(), text, args }
    }

    pub fn is(&self, (code, _): Copy) -> bool {
        self.code == code
    }
}

/// (code, English).
pub type Copy = (&'static str, &'static str);

// Results.
pub const POSSIBLE_CANDIDATE: Copy = ("possible_candidate", "Possible candidate. Not an identification.");
pub const NO_CLEARANCE: Copy = ("no_clearance", "No candidate is not a clearance.");
pub const INCOMPLETE: Copy = ("incomplete", "Incomplete.");
pub const NOT_COMPARED: Copy = ("not_compared", "Not compared.");
pub const BELOW_CUTOFF: Copy = ("below_cutoff", "Below the locked cutoff. Not a candidate.");
pub const FACE_UNSCORED: Copy = ("face_unscored", "This face could not be scored.");
pub const TOO_BRIEF: Copy = ("too_brief", "On screen too briefly to be a candidate. Not a clearance.");
pub const FIXTURE_MARKERS: Copy = ("fixture_markers", "Demo posters. These are synthetic markers, not faces.");
pub const NO_CLASS: Copy = ("no_class", "No class was on.");
pub const CHOOSE_CLASS: Copy = ("choose_class", "Choose missing, wanted, or both.");
pub const CANCELLED: Copy = ("cancelled", "You stopped the scan before every frame was analyzed.");
pub const STOPPED_HEAT: Copy = ("stopped_heat", "The device got too hot, so the scan stopped.");
pub const STOPPED_BATTERY: Copy = ("stopped_battery", "The battery ran low, so the scan stopped.");
pub const STOPPED_BACKGROUND: Copy = ("stopped_background", "The app left the screen, so the scan stopped.");
pub const NOT_FULLY_DECODED: Copy = ("not_fully_decoded", "The file was not fully decoded.");
pub const MOTION_CLIP_UNREAD: Copy = ("motion_clip_unread", "The video in this motion photo was not scanned.");
/// For the shells: a scan the system ended, found at the next launch.
pub const SCAN_KILLED: Copy = ("scan_killed", "The system closed OpenWorld before the scan finished.");
pub const RESULT_UNWRITTEN: Copy = ("result_unwritten", "The result could not be written.");

// Why a face was not compared.
pub const WHY_SMALL: Copy = ("why_small", "The face is under 112 pixels across in the original file.");
pub const WHY_UNALIGNED: Copy = ("why_unaligned", "The eyes, nose, and mouth could not be found.");
pub const WHY_CONFIDENCE: Copy = ("why_confidence", "The detector was not sure this is a face.");
pub const WHY_POSE: Copy = ("why_pose", "The face is turned too far from the camera.");
pub const WHY_BLUR: Copy = ("why_blur", "The face is too blurred.");
pub const WHY_EXPOSURE: Copy = ("why_exposure", "The face is too dark or too bright.");

// Bands. A score is never shown; the band says how far past the locked cutoff it is.
pub const BAND_NEAR: Copy = ("band_near", "Just past the locked cutoff.");
pub const BAND_ABOVE: Copy = ("band_above", "Clearly past the locked cutoff.");
pub const BAND_WELL: Copy = ("band_well", "Far past the locked cutoff.");

// Leaving.
pub const LEAVING: Copy = ("leaving", "You are leaving OpenWorld.");
pub const NOT_SENDING: Copy = ("not_sending", "OpenWorld is not sending a report.");
pub const LEAVE_WANTED: Copy = ("leave_wanted", "This opens an FBI wanted poster. Do not approach, follow, or confront anyone. If someone is in immediate danger, call 911. Wanted posters typically list 1-800-CALL-FBI (1-800-225-5324) or tips.fbi.gov.");
pub const LEAVE_MISSING: Copy = ("leave_missing", "This opens an FBI missing-person poster. It is not a criminal accusation. Do not approach, follow, or try to hold anyone, especially a child. Missing-child posters typically list NCMEC at 1-800-THE-LOST (1-800-843-5678).");
pub const LOOKALIKE: Copy = ("lookalike", "Look-alikes, relatives, and old photos can pass the cutoff.");
pub const DO_NOT_SHARE: Copy = ("do_not_share", "Do not post or share this result. A possible candidate can be a stranger who looks alike.");
pub const NETWORK_NOTE: Copy = ("network_note", "Opening a poster visits fbi.gov, and updating posters downloads them from api.fbi.gov. Those sites see your device's internet address, as any website does.");
pub const ONLY_FBI: Copy = ("only_fbi", "OpenWorld only opens an FBI page.");
pub const UNAUTHENTICATED: Copy = ("unauthenticated", "This file is not authenticated.");

// Disclosure.
pub const ON_DEVICE: Copy = ("on_device", "This file stays on this device.");
pub const NOTHING_UPLOADED: Copy = ("nothing_uploaded", "Nothing is uploaded.");
pub const NOBODY_ENROLLED: Copy = ("nobody_enrolled", "Nobody is enrolled.");
pub const NO_TRAINING: Copy = ("no_training", "OpenWorld does not train on this file.");
pub const NO_AGENCY: Copy = ("no_agency", "OpenWorld does not report anything to an agency.");
pub const NOT_AFFILIATED: Copy = ("not_affiliated", "OpenWorld is not part of, or endorsed by, the FBI or any government agency.");
pub const LAWFUL_USE: Copy = ("lawful_use", "Only scan files you have the right to use. Some places limit scanning other people's faces.");
pub const GROUPS_UNMEASURED: Copy = ("groups_unmeasured", "How often this misidentifies people of different ages, sexes, and skin tones is not measured yet.");
pub const NOT_IDENTIFICATION: Copy = ("not_identification", "A candidate is not an identification.");
pub const NOT_REAL_PROOF: Copy = ("not_real_proof", "On-device does not mean the file is real.");
pub const DISCLOSURE: &[Copy] = &[
    ON_DEVICE,
    NOTHING_UPLOADED,
    NOBODY_ENROLLED,
    NOT_AFFILIATED,
    NO_TRAINING,
    NO_AGENCY,
    NOT_IDENTIFICATION,
    NO_CLEARANCE,
    DO_NOT_SHARE,
    GROUPS_UNMEASURED,
    LAWFUL_USE,
    UNAUTHENTICATED,
    NOT_REAL_PROOF,
    NETWORK_NOTE,
];

// First run and the policies (`legal`).
pub const WELCOME: Copy = ("welcome", "OpenWorld compares faces in a photo or video you already have with public FBI missing-person and wanted-person posters, on this device.");
pub const DO_NOT_APPROACH: Copy = ("do_not_approach", "Do not approach, follow, confront, or try to hold anyone. If someone is in immediate danger, call 911.");
pub const ADULTS_US: Copy = ("adults_us", "For adults in the United States.");
pub const ACCEPT_TERMS: Copy = ("accept_terms", "By continuing, you agree to the Terms of Use, including binding individual arbitration and a class-action waiver, and to the Privacy Policy and the Biometric Data Policy.");
pub const TERMS_CHANGED: Copy = ("terms_changed", "The terms have changed. Read them, then continue to keep using OpenWorld.");
pub const DOC_TERMS: Copy = ("doc_terms", "Terms of Use");
pub const DOC_PRIVACY: Copy = ("doc_privacy", "Privacy Policy");
pub const DOC_BIOMETRIC: Copy = ("doc_biometric", "Biometric Data Policy");
pub const DOC_HEALTH_DATA: Copy = ("doc_health_data", "Consumer Health Data Privacy Policy");
pub const DOC_LICENSES: Copy = ("doc_licenses", "Licenses");
pub const FIRST_RUN: &[Copy] = &[WELCOME, NOT_AFFILIATED, NOT_IDENTIFICATION, NO_CLEARANCE, DO_NOT_APPROACH, DO_NOT_SHARE, NO_AGENCY, NOTHING_UPLOADED, ADULTS_US];

// File warnings.
pub const OLD_FILE: Copy = ("old_file", "This file is older than about 30 days.");
pub const TIMESTAMP_DISAGREE: Copy = ("timestamps_disagree", "The file timestamps disagree.");
pub const POSTERS_STALE: Copy = ("posters_stale", "The posters were fetched more than a day ago.");

// Device and estimate.
pub const CPU_NOTE: Copy = ("cpu_note", "This scan runs on the CPU. It will be slower, warmer, and use more battery.");
pub const GPU_NOTE: Copy = ("gpu_note", "This scan runs on the GPU and may warm the device.");
pub const HEAT_NOTE: Copy = ("heat_note", "This phone may get hot. If heat or the system stops the scan, the result is Incomplete.");
pub const ALREADY_WARM: Copy = ("already_warm", "This phone is already warm. The scan may stop early.");
pub const BATTERY_NOTE: Copy = ("battery_note", "A long scan uses a lot of battery.");
pub const SUGGEST_COMPUTER: Copy = ("suggest_computer", "A computer will finish this sooner. You can still run a complete scan on this phone.");
pub const ESTIMATE_CAVEAT: Copy = ("estimate_caveat", "This is a planning estimate, not a thermal measurement.");
pub const ESTIMATE_ROUGH: Copy = ("estimate_rough", "The model could not be timed on this device, so this estimate is rough.");
pub const BRIEF_FACE: Copy = ("brief_face", "A brief face can be missed.");
pub const COVERAGE_COMPLETE: Copy = ("coverage_complete", "Every decoded frame.");
pub const COVERAGE_MEASURED: Copy = ("coverage_measured", "5 frames a second, plus the tracker.");
pub const NOT_MEASURED: Copy = ("not_measured", "Not measured yet.");
pub const CHANCE_UNKNOWN: Copy = ("chance_unknown", "How often a stranger passes this cutoff is not measured yet.");
pub const BUNDLE_NOTE: Copy = ("bundle_note", "Scores are not comparable across bundles. Results name the bundle you pick.");
pub const NO_CAMERA: Copy = ("no_camera", "Import a file you already have. There is no camera.");

// Refusals.
pub const R_UNREADABLE: Copy = ("unreadable", "The file could not be read. Refusing.");
pub const R_BAD_CODEC: Copy = ("bad_codec", "Bad codec or unreadable file. Refusing.");
pub const R_NO_FPS: Copy = ("no_fps", "The decoder did not report a frame rate. Refusing.");
pub const R_TOO_LARGE: Copy = ("too_large", "This picture is too large to scan safely. Refusing.");
pub const R_NO_PROGRAM: Copy = ("missing_program", "The scan program is not on this device. Refusing.");
pub const R_CATALOG: Copy = ("catalog", "The bundle catalog could not be read. Refusing.");
pub const R_NO_BUNDLE: Copy = ("bundle_not_found", "That bundle is not in the catalog. Refusing.");
pub const R_NO_WEIGHTS: Copy = ("missing_weights", "The bundle weights are not installed, or their SHA-256 is not pinned. Refusing.");
pub const R_RUNTIME: Copy = ("runtime", "ONNX Runtime did not load. Refusing.");
pub const R_PACK_MISSING: Copy = ("missing_pack", "The poster pack is missing. Refusing.");
pub const R_PACK_HASH: Copy = ("bad_hash", "The poster pack hash does not match. Refusing.");
pub const R_PACK_EXPIRED: Copy = ("expired_pack", "The poster pack is expired. Refusing.");
pub const R_PACK_OLD: Copy = ("old_pack", "The posters are more than 7 days old. Update the posters. Refusing.");
pub const R_PACK_FUTURE: Copy = ("future_pack", "The poster pack is dated in the future. Check this device's clock. Refusing.");
pub const R_PACK_UNREADABLE: Copy = ("unreadable_pack", "The poster pack could not be read. Refusing.");
pub const R_PACK_VECTORS: Copy = ("bad_vectors", "The poster pack vectors are not valid for this bundle. Refusing.");
pub const R_PACK_EMBEDDER: Copy = ("embedder_mismatch", "The poster pack was made with another embedder. Refusing.");
pub const R_PACK_PERCEPTION: Copy = ("unknown_perception", "The poster pack perception is not recognized. Refusing.");
pub const R_PACK_NO_CLASS: Copy = ("no_posters_in_class", "The poster pack has no posters in the classes you chose. Refusing.");
pub const R_INTERNAL: Copy = ("internal", "The scan library failed. Refusing.");
pub const R_FIXTURE_OFF: Copy = ("fixture_off", "This build does not read demo posters. Refusing.");
pub const R_OUT_CREATE: Copy = ("out_create", "The output directory could not be created. Refusing.");
pub const R_OUT_FOREIGN: Copy = ("out_foreign", "That folder is not empty and is not an OpenWorld result. Choose an empty folder. Refusing.");
pub const R_OUT_OVERLAP: Copy = ("out_overlap", "The result folder cannot hold the file, the posters, or the bundles. Refusing.");
pub const R_OUT_REPLACE: Copy = ("out_replace", "The output directory could not be replaced. Refusing.");
pub const R_BAD_REQUEST: Copy = ("bad_request", "The scan request was not understood. Refusing.");
pub const R_REAL_POSTERS_OFF: Copy = ("real_posters_off", "Real FBI photos stay off. No bundle has a curve that allows them.");
pub const R_DELETE_FOREIGN: Copy = ("delete_foreign", "Refusing to delete a directory that is not an OpenWorld result.");
pub const R_DELETE_FAILED: Copy = ("delete_failed", "The result could not be deleted.");
pub const DELETED: Copy = ("deleted", "Deleted.");
pub const R_NO_DOCUMENT: Copy = ("no_document", "That document is not in this build. Refusing.");

// Update posters. A failed update leaves the posters as they were.
pub const R_NETWORK_OFF: Copy = ("network_off", "This build has no network code. The posters were not changed.");
pub const R_POSTERS_FETCH: Copy = ("posters_fetch_failed", "The FBI poster list could not be fetched. The posters were not changed.");
pub const R_POSTERS_BAD_LIST: Copy = ("posters_bad_list", "The FBI poster list was not in the expected form. The posters were not changed.");
pub const R_POSTERS_RUNTIME: Copy = ("posters_runtime", "The face models stopped while checking poster photos. The posters were not changed.");
pub const R_POSTERS_NONE: Copy = ("posters_none", "No poster photo passed the checks. The posters were not changed.");
pub const R_POSTERS_FOREIGN: Copy = ("posters_foreign", "That folder is not empty and is not a poster pack. Choose an empty folder. Refusing.");
pub const R_POSTERS_UNWRITTEN: Copy = ("posters_unwritten", "The new posters could not be written. The posters were not changed.");
// Why a poster was left out. A face on its photo that fails a gate uses the `why_*` line.
pub const SKIP_NOT_OPEN: Copy = ("skip_not_open", "Not an open, named missing or wanted poster.");
pub const SKIP_NO_PHOTO: Copy = ("skip_no_photo", "The poster has no photo at an FBI address.");
pub const SKIP_PHOTO_UNAVAILABLE: Copy = ("skip_photo_unavailable", "The poster photo could not be downloaded.");
pub const SKIP_PHOTO_TOO_LARGE: Copy = ("skip_photo_too_large", "The poster photo is too large to check safely.");
pub const SKIP_NOT_PICTURE: Copy = ("skip_not_picture", "The poster photo is not a picture OpenWorld reads.");
pub const SKIP_NO_FACE: Copy = ("skip_no_face", "No face was found on the poster photo.");
pub const SKIP_SEVERAL_FACES: Copy = ("skip_several_faces", "The poster photo shows more than one face.");

/// The classes compared for this scan. A clearance names them so both are not assumed.
pub fn class_note(missing: bool, wanted: bool) -> Copy {
    match (missing, wanted) {
        (true, true) => ("classes_both", "Missing and wanted."),
        (true, false) => ("classes_missing", "Missing."),
        (false, true) => ("classes_wanted", "Wanted."),
        (false, false) => NO_CLASS,
    }
}

/// The class line before Analyze. Both off keeps Analyze from starting.
pub fn estimate_class_line(missing: bool, wanted: bool) -> Copy {
    if missing || wanted {
        class_note(missing, wanted)
    } else {
        CHOOSE_CLASS
    }
}

/// Lines on the way out to a poster. Only recency and timestamp warnings travel with them.
pub fn leave_notes(wanted: bool, warnings: &[Line]) -> Vec<Line> {
    let mut notes = vec![Line::of(NOT_SENDING), Line::of(NOT_AFFILIATED), Line::of(if wanted { LEAVE_WANTED } else { LEAVE_MISSING })];
    notes.extend(warnings.iter().filter(|w| w.is(OLD_FILE) || w.is(TIMESTAMP_DISAGREE)).cloned());
    notes.extend([UNAUTHENTICATED, LOOKALIKE, DO_NOT_SHARE, NETWORK_NOTE].map(Line::of));
    notes
}

/// The frame a card came from, counting from 1. Index 0 is the first frame.
pub fn frame_label(index: u64) -> Line {
    Line::with("frame", format!("Frame {}.", index + 1), &[("frame", (index + 1).into())])
}

/// The frames a face was on screen, counting from 1.
pub fn frames_span(first: u64, last: u64) -> Line {
    if first == last {
        return frame_label(first);
    }
    Line::with(
        "frames_span",
        format!("Frames {} to {}.", first + 1, last + 1),
        &[("first", (first + 1).into()), ("last", (last + 1).into())],
    )
}

/// How much of the file was analyzed. A refusal decoded nothing and has no line.
pub fn frames_note(analyzed: u64, decoded: u64) -> Option<Line> {
    if decoded == 0 {
        return None;
    }
    let args = [("analyzed", analyzed.into()), ("decoded", decoded.into())];
    Some(if analyzed == decoded {
        if decoded == 1 {
            Line::with("frames_one", "1 frame analyzed.".into(), &args)
        } else {
            Line::with("frames_all", format!("{decoded} frames analyzed."), &args)
        }
    } else {
        Line::with("frames_some", format!("{analyzed} of {decoded} frames analyzed."), &args)
    })
}

/// The detection size this scan used, chosen so a face 112 px across in the original is found.
pub fn detection_line(long_side: u32, original_long: u32) -> Line {
    let args = [("long_side", long_side.into()), ("original", original_long.into())];
    if long_side >= original_long {
        Line::with("detect_full", "Faces were looked for at full resolution.".into(), &args)
    } else {
        Line::with(
            "detect_scaled",
            format!("Faces were looked for at {long_side} px, sized so a face 112 px across in the file is found."),
            &args,
        )
    }
}

/// Faces that could not be compared, so the result cannot be read as covering them.
pub fn faces_line(seen: u64, compared: u64) -> Line {
    let args = [("seen", seen.into()), ("compared", compared.into())];
    match (seen, compared) {
        (0, _) => Line::with("faces_none", "No face was found.".into(), &args),
        (s, c) if s == c => Line::with("faces_all", format!("{s} {} found and compared.", plural(s, "face", "faces")), &args),
        (s, c) => Line::with(
            "faces_some",
            format!("{s} {} found. {c} could be compared. The rest are not cleared.", plural(s, "face", "faces")),
            &args,
        ),
    }
}

/// How many false candidates chance alone would produce in this scan.
pub fn chance_line(expected: f64, faces: u64, posters: usize, source: &str) -> Line {
    let rounded = if expected < 0.01 { "under 0.01".to_string() } else { format!("about {expected:.2}") };
    Line::with(
        "chance",
        format!(
            "With {faces} {} compared to {posters} {}, chance alone gives {rounded} false {} (measured on {source}). The rate on your file can be higher, and can differ between groups of people.",
            plural(faces, "face", "faces"),
            plural(posters as u64, "poster", "posters"),
            if expected > 0.0 && expected < 1.5 { "candidate" } else { "candidates" }
        ),
        &[("expected", expected.into()), ("faces", faces.into()), ("posters", posters.into()), ("source", source.into())],
    )
}

pub fn plural<'a>(n: u64, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

/// The whole table, for shells that build a string catalog from it.
pub fn product_copy() -> Value {
    let all: &[Copy] = &[
        POSSIBLE_CANDIDATE, NO_CLEARANCE, INCOMPLETE, NOT_COMPARED, BELOW_CUTOFF, FACE_UNSCORED, TOO_BRIEF, FIXTURE_MARKERS,
        NO_CLASS, CHOOSE_CLASS, CANCELLED, STOPPED_HEAT, STOPPED_BATTERY, STOPPED_BACKGROUND, NOT_FULLY_DECODED,
        MOTION_CLIP_UNREAD, SCAN_KILLED, RESULT_UNWRITTEN, WHY_SMALL, WHY_UNALIGNED, WHY_CONFIDENCE, WHY_POSE, WHY_BLUR, WHY_EXPOSURE, BAND_NEAR, BAND_ABOVE,
        BAND_WELL, LEAVING, NOT_SENDING, LEAVE_WANTED, LEAVE_MISSING, ONLY_FBI, UNAUTHENTICATED, ON_DEVICE, NOTHING_UPLOADED,
        NOBODY_ENROLLED, NO_TRAINING, NO_AGENCY, NOT_AFFILIATED, LAWFUL_USE, GROUPS_UNMEASURED, LOOKALIKE, DO_NOT_SHARE,
        NETWORK_NOTE, NOT_IDENTIFICATION, NOT_REAL_PROOF, OLD_FILE, TIMESTAMP_DISAGREE,
        POSTERS_STALE, CPU_NOTE, GPU_NOTE, HEAT_NOTE, ALREADY_WARM, BATTERY_NOTE, SUGGEST_COMPUTER, ESTIMATE_CAVEAT,
        ESTIMATE_ROUGH, BRIEF_FACE, COVERAGE_COMPLETE, COVERAGE_MEASURED, NOT_MEASURED, CHANCE_UNKNOWN, BUNDLE_NOTE, NO_CAMERA,
        R_UNREADABLE, R_BAD_CODEC, R_NO_FPS, R_TOO_LARGE, R_NO_PROGRAM, R_CATALOG, R_NO_BUNDLE, R_NO_WEIGHTS, R_RUNTIME,
        R_PACK_MISSING, R_PACK_HASH, R_PACK_EXPIRED, R_PACK_OLD, R_PACK_FUTURE, R_PACK_UNREADABLE, R_PACK_VECTORS,
        R_PACK_NO_CLASS, R_INTERNAL,
        R_PACK_EMBEDDER, R_PACK_PERCEPTION, R_FIXTURE_OFF, R_OUT_CREATE, R_OUT_FOREIGN, R_OUT_OVERLAP, R_OUT_REPLACE,
        R_BAD_REQUEST, R_REAL_POSTERS_OFF, R_DELETE_FOREIGN, R_DELETE_FAILED, DELETED,
        R_NETWORK_OFF, R_POSTERS_FETCH, R_POSTERS_BAD_LIST, R_POSTERS_RUNTIME, R_POSTERS_NONE, R_POSTERS_FOREIGN,
        R_POSTERS_UNWRITTEN, SKIP_NOT_OPEN, SKIP_NO_PHOTO, SKIP_PHOTO_UNAVAILABLE, SKIP_PHOTO_TOO_LARGE, SKIP_NOT_PICTURE,
        SKIP_NO_FACE, SKIP_SEVERAL_FACES,
        WELCOME, DO_NOT_APPROACH, ADULTS_US, ACCEPT_TERMS, TERMS_CHANGED, DOC_TERMS, DOC_PRIVACY, DOC_BIOMETRIC, DOC_HEALTH_DATA,
        DOC_LICENSES, R_NO_DOCUMENT,
        class_note(true, true), class_note(true, false), class_note(false, true),
    ];
    let mut map = Map::new();
    for (code, text) in all {
        map.insert((*code).into(), (*text).into());
    }
    Value::Object(map)
}
