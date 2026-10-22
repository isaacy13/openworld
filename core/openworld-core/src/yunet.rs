// SPDX-License-Identifier: Apache-2.0

//! YuNet box decode, as OpenCV's `FaceDetectorYN` reads the 2023 export.
//! The network weights are not in this file.
//!
//! One anchor per cell. The score is the geometric mean of the class and
//! objectness outputs. Centers and landmarks are offsets from the cell corner
//! in stride units, and width and height are a log of the stride.

use crate::scrfd::{nms, FaceBox};
use crate::geom::Rect;

pub const STRIDES: [u32; 3] = [8, 16, 32];

/// One output plane per stride, in the order of `STRIDES`.
pub struct Planes<'a> {
    pub cls: [&'a [f32]; 3],
    pub obj: [&'a [f32]; 3],
    pub bbox: [&'a [f32]; 3],
    pub kps: [&'a [f32]; 3],
}

pub fn decode(input_w: u32, input_h: u32, planes: &Planes<'_>, threshold: f32) -> Vec<FaceBox> {
    let mut faces = Vec::new();
    for (level, stride) in STRIDES.iter().copied().enumerate() {
        let cols = input_w / stride;
        let rows = input_h / stride;
        let cells = (cols * rows) as usize;
        if planes.cls[level].len() < cells
            || planes.obj[level].len() < cells
            || planes.bbox[level].len() < cells * 4
            || planes.kps[level].len() < cells * 10
        {
            continue;
        }
        let s = stride as f32;
        for r in 0..rows {
            for c in 0..cols {
                let index = (r * cols + c) as usize;
                let cls = planes.cls[level][index].clamp(0.0, 1.0);
                let obj = planes.obj[level][index].clamp(0.0, 1.0);
                let score = (cls * obj).sqrt();
                if !(score >= threshold) {
                    continue;
                }
                let d = &planes.bbox[level][index * 4..index * 4 + 4];
                let cx = (c as f32 + d[0]) * s;
                let cy = (r as f32 + d[1]) * s;
                let w = d[2].exp() * s;
                let h = d[3].exp() * s;
                if !w.is_finite() || !h.is_finite() {
                    continue;
                }
                let Some(rect) = clamp_rect(cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0, input_w, input_h) else {
                    continue;
                };
                let k = &planes.kps[level][index * 10..index * 10 + 10];
                let mut points = [(0f32, 0f32); 5];
                for (p, point) in points.iter_mut().enumerate() {
                    *point = ((k[p * 2] + c as f32) * s, (k[p * 2 + 1] + r as f32) * s);
                }
                faces.push(FaceBox { rect, score, landmarks: Some(points) });
            }
        }
    }
    nms(&mut faces, 0.3);
    faces
}

fn clamp_rect(x1: f32, y1: f32, x2: f32, y2: f32, width: u32, height: u32) -> Option<Rect> {
    let left = x1.clamp(0.0, width as f32);
    let top = y1.clamp(0.0, height as f32);
    let right = x2.clamp(0.0, width as f32);
    let bottom = y2.clamp(0.0, height as f32);
    let w = (right - left).round() as i64;
    let h = (bottom - top).round() as i64;
    if w < 1 || h < 1 {
        return None;
    }
    Some(Rect { x: left.round() as u32, y: top.round() as u32, w: w as u32, h: h as u32 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planes_for(side: u32) -> [Vec<f32>; 12] {
        let mut out: [Vec<f32>; 12] = Default::default();
        for (level, stride) in STRIDES.iter().enumerate() {
            let cells = ((side / stride) * (side / stride)) as usize;
            out[level] = vec![0.0; cells];
            out[3 + level] = vec![0.0; cells];
            out[6 + level] = vec![0.0; cells * 4];
            out[9 + level] = vec![0.0; cells * 10];
        }
        out
    }

    fn decode_all(side: u32, raw: &[Vec<f32>; 12], threshold: f32) -> Vec<FaceBox> {
        let planes = Planes {
            cls: [&raw[0], &raw[1], &raw[2]],
            obj: [&raw[3], &raw[4], &raw[5]],
            bbox: [&raw[6], &raw[7], &raw[8]],
            kps: [&raw[9], &raw[10], &raw[11]],
        };
        decode(side, side, &planes, threshold)
    }

    #[test]
    fn a_cell_decodes_to_a_box_around_its_offset_center() {
        // 64 px input. Stride 16 is a 4x4 grid. Cell (row 1, col 2) is index 6.
        let mut raw = planes_for(64);
        raw[1][6] = 0.81;
        raw[4][6] = 1.0;
        raw[7][24..28].copy_from_slice(&[0.5, 0.5, 1.0f32.ln(), 2.0f32.ln()]);
        for p in 0..5 {
            raw[10][60 + p * 2] = 0.25;
            raw[10][60 + p * 2 + 1] = 0.75;
        }
        let faces = decode_all(64, &raw, 0.5);
        assert_eq!(faces.len(), 1);
        // Center (2.5, 1.5) * 16 = (40, 24). Width 16, height 32.
        assert_eq!(faces[0].rect, Rect { x: 32, y: 8, w: 16, h: 32 });
        assert!((faces[0].score - 0.9).abs() < 1e-5);
        let points = faces[0].landmarks.unwrap();
        assert_eq!(points[0], (36.0, 28.0));
    }

    #[test]
    fn a_low_score_is_not_a_face() {
        let mut raw = planes_for(64);
        raw[0][0] = 0.5;
        raw[3][0] = 0.5;
        assert!(decode_all(64, &raw, 0.6).is_empty());
        assert_eq!(decode_all(64, &raw, 0.5).len(), 1);
    }
}
