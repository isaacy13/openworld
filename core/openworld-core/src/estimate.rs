// SPDX-License-Identifier: Apache-2.0

use crate::copy::{BATTERY_NOTE, ESTIMATE_CAVEAT, HEAT_NOTE, SUGGEST_COMPUTER};
use crate::hardware::Execution;
use serde::{Deserialize, Serialize};

/// Long estimates on a phone suggest a computer. Complete is still allowed.
const LONG_SECONDS: f64 = 180.0;
/// Reference: milliseconds per analyzed frame at 640 px long side, Fast, CPU.
const MS_AT_640_FAST_CPU: f64 = 40.0;

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionSize {
    Px(u32),
    Full,
}

impl DetectionSize {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "full" => Some(DetectionSize::Full),
            "320" => Some(DetectionSize::Px(320)),
            "480" => Some(DetectionSize::Px(480)),
            "640" => Some(DetectionSize::Px(640)),
            _ => None,
        }
    }

    pub fn as_str(self) -> String {
        match self {
            DetectionSize::Full => "full".into(),
            DetectionSize::Px(px) => px.to_string(),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DetectionSize::Px(320) => "320 px on the long side",
            DetectionSize::Px(480) => "480 px on the long side",
            DetectionSize::Px(640) => "640 px on the long side",
            DetectionSize::Full => "Full resolution",
            DetectionSize::Px(_) => "Custom",
        }
    }
}

/// Smaller than full resolution starts selected.
pub fn default_detection_size() -> DetectionSize {
    DetectionSize::Px(640)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormFactor {
    Phone,
    Computer,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Estimate {
    pub seconds: f64,
    pub human: String,
    pub detection: String,
    pub coverage: String,
    pub frames_analyzed: u64,
    pub suggest_computer: bool,
    pub suggest_computer_text: Option<String>,
    pub device_note: Option<String>,
    pub heat_note: Option<String>,
    pub battery_note: Option<String>,
    pub caveat: String,
}

#[derive(Clone, Debug)]
pub struct EstimateInput {
    pub frames: u64,
    pub fps: f64,
    pub duration_sec: f64,
    pub original_long_side: u32,
    pub detection: DetectionSize,
    pub coverage: Coverage,
    pub bundle_factor: f64,
    pub execution: Execution,
    pub form_factor: FormFactor,
}

pub fn estimate(input: &EstimateInput) -> Estimate {
    let det_long = match input.detection {
        DetectionSize::Px(px) => px.min(input.original_long_side.max(1)),
        DetectionSize::Full => input.original_long_side.max(1),
    };
    let frames_analyzed = analyzed_frames(input);
    let provider = match input.execution {
        Execution::Cpu => 1.0,
        Execution::Gpu => 0.4,
        Execution::Neural => 0.2,
    };
    let ms = frames_analyzed as f64
        * MS_AT_640_FAST_CPU
        * (det_long as f64 / 640.0).powi(2)
        * provider
        * input.bundle_factor.max(0.1);
    let seconds = ms / 1000.0;
    let long = seconds >= LONG_SECONDS;
    let suggest = long && input.form_factor == FormFactor::Phone;
    Estimate {
        seconds,
        human: human_duration(seconds),
        detection: input.detection.as_str(),
        coverage: input.coverage.as_str().to_string(),
        frames_analyzed,
        suggest_computer: suggest,
        suggest_computer_text: suggest.then(|| SUGGEST_COMPUTER.to_string()),
        device_note: input.execution.device_note().map(str::to_string),
        heat_note: (input.form_factor == FormFactor::Phone).then(|| HEAT_NOTE.to_string()),
        battery_note: (input.form_factor == FormFactor::Phone).then(|| BATTERY_NOTE.to_string()),
        caveat: ESTIMATE_CAVEAT.to_string(),
    }
}

fn analyzed_frames(input: &EstimateInput) -> u64 {
    let frames = input.frames.max(1);
    match input.coverage {
        Coverage::Complete => frames,
        Coverage::Measured => {
            let duration = if input.duration_sec > 0.0 {
                input.duration_sec
            } else if input.fps > 0.0 {
                frames as f64 / input.fps
            } else {
                0.0
            };
            let n = (duration * 5.0).round() as u64;
            n.clamp(1, frames)
        }
    }
}

pub fn human_duration(seconds: f64) -> String {
    if seconds < 1.0 {
        "Less than a second".into()
    } else if seconds < 90.0 {
        format!("About {} seconds", seconds.round() as u64)
    } else if seconds < 90.0 * 60.0 {
        format!("About {} minutes", (seconds / 60.0).round() as u64)
    } else {
        format!("About {} hours", (seconds / 3600.0).round() as u64)
    }
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
            detection: DetectionSize::Px(640),
            coverage: Coverage::Complete,
            bundle_factor: 1.0,
            execution: Execution::Cpu,
            form_factor: FormFactor::Computer,
        }
    }

    #[test]
    fn smaller_is_faster_than_full_and_a_photo_is_short() {
        let mut small = base();
        small.frames = 300;
        small.fps = 30.0;
        small.duration_sec = 10.0;
        small.detection = DetectionSize::Px(320);
        let mut mid = small.clone();
        mid.detection = DetectionSize::Px(640);
        let mut full = small.clone();
        full.detection = DetectionSize::Full;
        let a = estimate(&small).seconds;
        let b = estimate(&mid).seconds;
        let c = estimate(&full).seconds;
        assert!(a < b && b < c, "{a} < {b} < {c}");
        let photo = estimate(&base());
        assert!(photo.seconds < 1.0);
        assert!(!photo.suggest_computer);
        assert!(photo.device_note.unwrap().contains("CPU"));
        assert!(photo.heat_note.is_none());
    }

    #[test]
    fn a_long_phone_scan_suggests_a_computer_and_still_explains_heat() {
        let mut input = base();
        input.frames = 30 * 60 * 10;
        input.fps = 30.0;
        input.duration_sec = 600.0;
        input.form_factor = FormFactor::Phone;
        input.detection = DetectionSize::Px(640);
        let est = estimate(&input);
        assert!(est.suggest_computer);
        assert!(est.suggest_computer_text.unwrap().contains("complete"));
        assert!(est.heat_note.is_some());
        assert!(est.battery_note.is_some());
        input.execution = Execution::Neural;
        let neural = estimate(&input);
        assert!(neural.device_note.is_none());
        input.execution = Execution::Gpu;
        assert!(estimate(&input).device_note.unwrap().contains("GPU"));
    }

    #[test]
    fn measured_analyzes_fewer_frames_than_complete() {
        let mut input = base();
        input.frames = 300;
        input.fps = 30.0;
        input.duration_sec = 10.0;
        let complete = estimate(&input);
        input.coverage = Coverage::Measured;
        let measured = estimate(&input);
        assert!(measured.frames_analyzed < complete.frames_analyzed);
        assert!(measured.seconds < complete.seconds);
    }
}
