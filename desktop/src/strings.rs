// SPDX-License-Identifier: Apache-2.0

//! Words that belong to this shell: button titles, headings, short chips.
//!
//! Every sentence about a scan comes from the core as a `Line` and is shown with
//! [`say`]. Nothing here is compared to decide state. Each label has a stable id,
//! so a gettext catalog can replace the English table later without touching callers.

use openworld_core::Line;

/// A label id. The English is in [`text`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum S {
    AppName,
    Tagline,
    ChooseFile,
    OrDropHere,
    DropRelease,
    TrustOnDevice,
    TrustNothingUploaded,
    TrustNoCamera,
    NoPosters,
    InstallDemo,
    UpdatePosters,
    DemoBadge,
    ResultsKept,
    Review,
    Photo,
    Video,
    Estimating,
    CompareWith,
    Missing,
    Wanted,
    Options,
    ModelBundle,
    Coverage,
    EveryFrame,
    FiveASecond,
    WhatHappens,
    ReadAll,
    Disclosure,
    Analyze,
    ChooseAnother,
    Scanning,
    ScanningOnDevice,
    Faces,
    NoFacesYet,
    Stop,
    Stopping,
    StopTitle,
    StopBody,
    KeepScanning,
    Result,
    Candidates,
    ShowFrame,
    OpenFbi,
    NoFbiPage,
    Stay,
    Details,
    OtherFaces,
    DeleteResult,
    DeleteTitle,
    DeleteBody,
    Delete,
    Cancel,
    ScanAnother,
    Close,
    ChipLooking,
    ChipCompared,
    ChipNotCompared,
    ChipCandidate,
    ChipTooBrief,
    ChipUnscored,
    FileFilterMedia,
    FileFilterAll,
    Unofficial,
    DetailFrames,
    DetailCoverage,
    DetailDetection,
    DetailChance,
    DetailClasses,
    DetailBundle,
    DetailPosters,
    DetailDevice,
    DetailPerception,
    DetailWarnings,
    FileFrom,
    HintChooseFile,
    HintInstallDemo,
    HintCheckClock,
    HintInstallWeights,
    HintTryAgain,
    YourFile,
    Poster,
    Unopened,
    About,
    MainMenu,
    Welcome,
    Continue,
    Publisher,
}

/// The English label. Swap this table for a catalog to translate.
pub fn text(id: S) -> &'static str {
    match id {
        S::AppName => "OpenWorld",
        S::Tagline => "Check a photo or video you already have against public FBI missing and wanted posters, on this device.",
        S::ChooseFile => "Choose a photo or video",
        S::OrDropHere => "or drop it here",
        S::DropRelease => "Release to open this file",
        S::TrustOnDevice => "Stays on this device",
        S::TrustNothingUploaded => "Nothing uploaded",
        S::TrustNoCamera => "No camera",
        S::NoPosters => "No posters installed. Real FBI photos stay off.",
        S::InstallDemo => "Install demo posters",
        S::UpdatePosters => "Update posters",
        S::DemoBadge => "Demo",
        S::ResultsKept => "Results are deleted after 7 days.",
        S::Review => "Review",
        S::Photo => "Photo",
        S::Video => "Video",
        S::Estimating => "Estimating the time…",
        S::CompareWith => "Compare with",
        S::Missing => "Missing",
        S::Wanted => "Wanted",
        S::Options => "Options",
        S::ModelBundle => "Model bundle",
        S::Coverage => "Coverage",
        S::EveryFrame => "Every frame",
        S::FiveASecond => "5 frames a second",
        S::WhatHappens => "What happens to this file",
        S::ReadAll => "Read all",
        S::Disclosure => "What OpenWorld does and does not do",
        S::Analyze => "Analyze on this device",
        S::ChooseAnother => "Choose another file",
        S::Scanning => "Scanning",
        S::ScanningOnDevice => "Scanning on this device",
        S::Faces => "Faces",
        S::NoFacesYet => "Faces appear here as they are found.",
        S::Stop => "Stop",
        S::Stopping => "Stopping…",
        S::StopTitle => "Stop the scan?",
        S::StopBody => "A stopped scan is Incomplete. It is never a clearance.",
        S::KeepScanning => "Keep scanning",
        S::Result => "Result",
        S::Candidates => "Possible candidates",
        S::ShowFrame => "Show frame",
        S::OpenFbi => "Open FBI page",
        S::NoFbiPage => "This poster has no FBI page.",
        S::Stay => "Stay",
        S::Details => "Details",
        S::OtherFaces => "Other faces",
        S::DeleteResult => "Delete result",
        S::DeleteTitle => "Delete this result?",
        S::DeleteBody => "The crops, frames, and result file are removed from this device. Your original file is not touched.",
        S::Delete => "Delete",
        S::Cancel => "Cancel",
        S::ScanAnother => "Scan another file",
        S::Close => "Close",
        S::ChipLooking => "Looking",
        S::ChipCompared => "Compared",
        S::ChipNotCompared => "Not compared",
        S::ChipCandidate => "Possible candidate",
        S::ChipTooBrief => "Too brief",
        S::ChipUnscored => "Not scored",
        S::FileFilterMedia => "Photos and videos",
        S::FileFilterAll => "All files",
        S::Unofficial => "Unofficial",
        S::DetailFrames => "Frames",
        S::DetailCoverage => "Coverage",
        S::DetailDetection => "Detection size",
        S::DetailChance => "Chance",
        S::DetailClasses => "Classes",
        S::DetailBundle => "Model bundle",
        S::DetailPosters => "Poster pack",
        S::DetailDevice => "Ran on",
        S::DetailPerception => "Posters",
        S::DetailWarnings => "File",
        S::FileFrom => "From your file",
        S::HintChooseFile => "Choose another photo or video.",
        S::HintInstallDemo => "Go back to the start screen to install demo posters.",
        S::HintCheckClock => "Check this computer's date and time, then try again.",
        S::HintInstallWeights => "Install the bundle weights (scripts/fetch_weights.py), or use the demo posters.",
        S::HintTryAgain => "Try again, or choose another file.",
        S::YourFile => "Your file",
        S::Poster => "Poster",
        S::Unopened => "The page could not be opened.",
        S::About => "About OpenWorld",
        S::MainMenu => "Main menu",
        S::Welcome => "Welcome to OpenWorld",
        S::Continue => "Continue",
        S::Publisher => "OneX Engineering",
    }
}

/// A sentence from the core. Later this translates by `line.code` with `line.args`.
pub fn say(line: &Line) -> &str {
    &line.text
}

fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn plural(n: u64, one: &str, many: &str) -> String {
    format!("{} {}", grouped(n), if n == 1 { one } else { many })
}

/// "1,800 frames · faces looked for at 960 px".
pub fn estimate_summary(frames: u64, detection_long: u32, full: bool) -> String {
    let size = if full { "faces looked for at full resolution".to_string() } else { format!("faces looked for at {detection_long} px") };
    format!("{} · {size}", plural(frames, "frame", "frames"))
}

/// "Frame 412 of 1,800".
pub fn progress(analyzed: u64, total: u64) -> String {
    format!("Frame {} of {}", grouped(analyzed.min(total)), grouped(total))
}

/// "About 1 minute left", from the core's duration line.
pub fn time_left(duration: &Line) -> String {
    format!("{} left", say(duration))
}

/// "Demo posters · 2 posters · fetched today".
pub fn pack_footer(demo: bool, posters: usize, days_old: Option<u64>) -> String {
    let kind = if demo { "Demo posters" } else { "FBI posters" };
    let fetched = match days_old {
        Some(0) => "fetched today".to_string(),
        Some(1) => "fetched yesterday".to_string(),
        Some(d) => format!("fetched {d} days ago"),
        None => "fetch date unknown".to_string(),
    };
    format!("{kind} · {} · {fetched}", plural(posters as u64, "poster", "posters"))
}

/// "Reading the FBI list · page 3 of 26", then "Checking poster photos · 120 of 900".
pub fn update_progress(page: u32, pages: Option<u32>, checked: u64, posters: u64) -> String {
    if posters > 0 {
        return format!("Checking poster photos · {} of {}", grouped(checked), grouped(posters));
    }
    match (page, pages) {
        (0, _) => "Reading the FBI list…".to_string(),
        (page, Some(pages)) => format!("Reading the FBI list · page {} of {}", grouped(page.into()), grouped(pages.into())),
        (page, None) => format!("Reading the FBI list · page {}", grouped(page.into())),
    }
}

/// "Kept 812 posters."
pub fn kept(n: u64) -> String {
    format!("Kept {}.", plural(n, "poster", "posters"))
}

/// "12 left out: No face was found on the poster photo."
pub fn skipped(count: u64, reason: &str) -> String {
    format!("{} left out: {reason}", grouped(count))
}

/// A class chip: "Missing · 1".
pub fn class_chip(class: S, count: usize) -> String {
    format!("{} · {}", text(class), grouped(count as u64))
}

/// "1 possible candidate to review".
pub fn candidate_count(n: usize) -> String {
    format!("{} to review.", plural(n as u64, "possible candidate", "possible candidates"))
}

/// The accessible name of a face crop: "Face from frame 12. Possible candidate."
pub fn face_label(frame: u64, status: S) -> String {
    format!("Face from frame {}. {}.", grouped(frame + 1), text(status))
}

/// "Photo · 640 × 480 · 3.1 KB" or "Video · 0:42 · 12.3 MB".
pub fn file_kind(video: bool, width: u32, height: u32, duration_sec: f64, bytes: u64) -> String {
    let size = file_size(bytes);
    if video {
        format!("{} · {} · {size}", text(S::Video), clock(duration_sec))
    } else {
        format!("{} · {width} × {height} · {size}", text(S::Photo))
    }
}

/// "0:42", "1:02:03".
pub fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub fn file_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} bytes");
    }
    let mut value = bytes as f64 / 1000.0;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// "Recorded 19 Oct 2026" from a date the caller formats for the locale.
pub fn recorded(date: &str) -> String {
    format!("Recorded {date}")
}

/// "Fast 0.2.0".
pub fn bundle_name(name: &str, version: &str) -> String {
    format!("{name} {version}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_naturally() {
        assert_eq!(progress(412, 1800), "Frame 412 of 1,800");
        assert_eq!(estimate_summary(1, 640, true), "1 frame · faces looked for at full resolution");
        assert_eq!(estimate_summary(1800, 960, false), "1,800 frames · faces looked for at 960 px");
        assert_eq!(pack_footer(true, 2, Some(0)), "Demo posters · 2 posters · fetched today");
        assert_eq!(update_progress(0, None, 0, 0), "Reading the FBI list…");
        assert_eq!(update_progress(3, Some(26), 0, 0), "Reading the FBI list · page 3 of 26");
        assert_eq!(update_progress(3, None, 0, 0), "Reading the FBI list · page 3");
        assert_eq!(update_progress(26, Some(26), 120, 1200), "Checking poster photos · 120 of 1,200");
        assert_eq!(kept(1), "Kept 1 poster.");
        assert_eq!(skipped(12, "No face was found on the poster photo."), "12 left out: No face was found on the poster photo.");
        assert_eq!(face_label(11, S::ChipCandidate), "Face from frame 12. Possible candidate.");
        assert_eq!(clock(42.4), "0:42");
        assert_eq!(clock(3723.0), "1:02:03");
        assert_eq!(file_size(3100), "3.1 KB");
        assert_eq!(file_size(12_300_000), "12.3 MB");
    }
}
