// SPDX-License-Identifier: Apache-2.0

//! The time a scan will take, before Analyze.
//!
//! The detector and embedder are timed on this device at this file's
//! detection size (a few passes, the median kept), and that time is
//! multiplied by the frames the coverage will read. Decoding is added at a
//! rate per megapixel. When the networks cannot be timed the estimate says
//! it is rough. It is a planning model, not a thermal table.

use crate::copy::{self, Line};
use crate::hardware::Execution;
use crate::perception::Timing;
use serde::{Deserialize, Serialize};

/// Long estimates on a phone suggest a computer. Complete is still allowed.
const LONG_SECONDS: f64 = 180.0;
/// Untimed fallback: milliseconds per detector pass at 640 px on a CPU.
const ROUGH_DETECT_MS_640: f64 = 40.0;
const ROUGH_EMBED_MS: f64 = 8.0;
/// Decoding, milliseconds per megapixel of each decoded frame.
const DECODE_MS_PER_MP: f64 = 2.5;
/// Embeddings expected per minute of video: a few faces, each a few frames.
const EMBEDS_PER_MINUTE: f64 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    Complete,
    Measured,
}

impl Coverage {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "complete" => Some(Coverage::Complete),
            "measured" => Some(Coverage::Measured),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Coverage::Complete => "complete",
            Coverage::Measured => "measured",
        }
    }

    pub fn line(self) -> Line {
        Line::of(match self {
            Coverage::Complete => copy::COVERAGE_COMPLETE,
            Coverage::Measured => copy::COVERAGE_MEASURED,
        })
    }
}

/// What a phone says about itself. A computer sends none.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Device {
    /// `phone` or `computer`.
    #[serde(default = "computer")]
    pub kind: String,
    /// The system's thermal state: nominal, fair, serious, critical.
    #[serde(default)]
    pub thermal: Option<String>,
    /// Battery level from 0 to 1.
    #[serde(default)]
    pub battery: Option<f32>,
    #[serde(default)]
    pub charging: Option<bool>,
}

fn computer() -> String {
    "computer".into()
}

impl Device {
    pub fn phone(&self) -> bool {
        self.kind == "phone"
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Estimate {
    pub seconds: f64,
    pub human: Line,
    pub frames_analyzed: u64,
    pub detection_long_side: u32,
    pub detection_line: Line,
    pub coverage: String,
    pub coverage_line: Line,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage_banner: Option<Line>,
    pub timed: bool,
    pub notes: Vec<Line>,
    pub suggest_computer: bool,
    pub caveat: Line,
    /// About the file itself: old, or its timestamps disagree.
    #[serde(default)]
    pub warnings: Vec<Line>,
}

pub struct EstimateInput {
    pub frames: u64,
    pub fps: f64,
    pub duration_sec: f64,
    pub original_long_side: u32,
    pub original_short_side: u32,
    pub detection_long_side: u32,
    pub coverage: Coverage,
    pub timing: Option<Timing>,
    pub execution: Execution,
    pub device: Device,
    /// The last frame is a motion-photo cover. Measured coverage reads it.
    pub cover: bool,
}

pub fn estimate(input: &EstimateInput) -> Estimate {
    let analyzed = analyzed_frames(input);
    let decoded = input.frames.max(1);
    let timed = input.timing.is_some();
    let timing = input.timing.unwrap_or_else(|| rough(input.detection_long_side, input.execution));
    let megapixels = f64::from(input.original_long_side) * f64::from(input.original_short_side.max(1)) / 1e6;
    let minutes = if input.duration_sec > 0.0 { input.duration_sec / 60.0 } else { 0.0 };
    let embeds = (EMBEDS_PER_MINUTE * minutes).max(1.0);
    let ms = analyzed as f64 * timing.detect_ms + embeds * timing.embed_ms + decoded as f64 * megapixels * DECODE_MS_PER_MP;
    let seconds = ms / 1000.0;
    let phone = input.device.phone();
    let suggest = phone && seconds >= LONG_SECONDS;
    let mut notes = Vec::new();
    if let Some(note) = input.execution.device_note() {
        notes.push(Line::of(note));
    }
    if phone {
        notes.push(Line::of(copy::HEAT_NOTE));
        if matches!(input.device.thermal.as_deref(), Some("serious") | Some("critical")) {
            notes.push(Line::of(copy::ALREADY_WARM));
        }
        if seconds >= LONG_SECONDS || input.device.battery.is_some_and(|b| b < 0.3) && input.device.charging != Some(true) {
            notes.push(Line::of(copy::BATTERY_NOTE));
        }
    }
    if suggest {
        notes.push(Line::of(copy::SUGGEST_COMPUTER));
    }
    if !timed {
        notes.push(Line::of(copy::ESTIMATE_ROUGH));
    }
    Estimate {
        seconds,
        human: human_duration(seconds),
        frames_analyzed: analyzed,
        detection_long_side: input.detection_long_side,
        detection_line: copy::detection_line(input.detection_long_side, input.original_long_side),
        coverage: input.coverage.as_str().into(),
        coverage_line: input.coverage.line(),
        coverage_banner: (input.coverage == Coverage::Measured).then(|| Line::of(copy::BRIEF_FACE)),
        timed,
        notes,
        suggest_computer: suggest,
        caveat: Line::of(copy::ESTIMATE_CAVEAT),
        warnings: Vec::new(),
    }
}

/// A rough timing when the networks cannot run here: area over 640², by provider.
fn rough(det_long: u32, execution: Execution) -> Timing {
    let provider = match execution {
        Execution::Cpu => 1.0,
        Execution::Gpu => 0.4,
        Execution::Neural => 0.25,
    };
    let area = (f64::from(det_long.max(1)) / 640.0).powi(2);
    Timing { detect_ms: ROUGH_DETECT_MS_640 * area * provider, embed_ms: ROUGH_EMBED_MS * provider }
}

fn analyzed_frames(input: &EstimateInput) -> u64 {
    let frames = input.frames.max(1);
    match input.coverage {
        Coverage::Complete => frames,
        // The same frames `take_measured_sample` keeps.
        Coverage::Measured => {
            let kept = crate::geom::sample_indices(frames, input.fps, true);
            let mut n = kept.len() as u64;
            if input.cover && kept.last().copied() != Some(frames - 1) {
                n += 1;
            }
            n.clamp(1, frames)
        }
    }
}

pub fn human_duration(seconds: f64) -> Line {
    let (n, unit) = if seconds < 1.0 {
        return Line::with("duration_under_second", "Less than a second".into(), &[]);
    } else if seconds < 59.5 {
        (seconds.round() as u64, "second")
    } else if seconds < 59.5 * 60.0 {
        ((seconds / 60.0).round().max(1.0) as u64, "minute")
    } else {
        ((seconds / 3600.0).round().max(1.0) as u64, "hour")
    };
    let n = n.max(1);
    let text = if n == 1 { format!("About 1 {unit}") } else { format!("About {n} {unit}s") };
    Line::with(&format!("duration_{unit}s"), text, &[("count", n.into())])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> EstimateInput {
        EstimateInput {
            frames: 1,
            fps: 0.0,
            duration_sec: 0.0,
            original_long_side: 1920,
            original_short_side: 1080,
            detection_long_side: 640,
            coverage: Coverage::Complete,
            timing: Some(Timing { detect_ms: 20.0, embed_ms: 5.0 }),
            execution: Execution::Cpu,
            device: Device::default(),
            cover: false,
        }
    }

    #[test]
    fn a_photo_is_short_and_a_long_phone_video_suggests_a_computer() {
        let photo = estimate(&base());
        assert!(photo.seconds < 1.0);
        assert_eq!(photo.human.text, "Less than a second");
        assert!(photo.notes.iter().any(|n| n.is(copy::CPU_NOTE)));
        assert!(!photo.notes.iter().any(|n| n.is(copy::HEAT_NOTE)));
        let mut long = base();
        long.frames = 30 * 60 * 10;
        long.fps = 30.0;
        long.duration_sec = 600.0;
        long.device = Device { kind: "phone".into(), thermal: Some("serious".into()), battery: Some(0.2), charging: Some(false) };
        let est = estimate(&long);
        assert!(est.suggest_computer);
        for note in [copy::HEAT_NOTE, copy::ALREADY_WARM, copy::BATTERY_NOTE, copy::SUGGEST_COMPUTER] {
            assert!(est.notes.iter().any(|n| n.is(note)), "{note:?}");
        }
    }

    #[test]
    fn measured_reads_fewer_frames_and_untimed_is_rough() {
        let mut input = base();
        input.frames = 300;
        input.fps = 30.0;
        input.duration_sec = 10.0;
        let complete = estimate(&input);
        input.coverage = Coverage::Measured;
        let measured = estimate(&input);
        assert_eq!(measured.frames_analyzed, 50);
        assert!(measured.seconds < complete.seconds);
        assert!(measured.coverage_banner.is_some());
        input.timing = None;
        assert!(estimate(&input).notes.iter().any(|n| n.is(copy::ESTIMATE_ROUGH)));
    }

    #[test]
    fn durations_read_naturally() {
        let text = |s: f64| human_duration(s).text;
        assert_eq!(text(1.4), "About 1 second");
        assert_eq!(text(2.0), "About 2 seconds");
        assert_eq!(text(59.6), "About 1 minute");
        assert_eq!(text(90.0), "About 2 minutes");
        assert_eq!(text(3570.0), "About 1 hour");
        assert_eq!(text(7200.0), "About 2 hours");
        assert_eq!(human_duration(7200.0).args["count"], 2);
    }
}
