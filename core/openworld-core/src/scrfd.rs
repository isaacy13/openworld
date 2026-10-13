// SPDX-License-Identifier: Apache-2.0

//! SCRFD box decode. The network weights are not in this file.
//!
//! Centers sit on the stride grid, without the extra half-stride. Two anchors
//! share each cell. Distances are left, top, right, bottom in stride units.
//! Scores above 1 or below 0 are logits and get a sigmoid.

use crate::geom::Rect;

pub const STRIDES: [u32; 3] = [8, 16, 32];
pub const ANCHORS: u32 = 2;

#[derive(Clone, Debug)]
pub struct FaceBox {
    pub rect: Rect,
    pub score: f32,
    pub landmarks: Option<[(f32, f32); 5]>,
}

pub fn decode(
    input_w: u32,
    input_h: u32,
    scores: [&[f32]; 3],
    distances: [&[f32]; 3],
    keypoints: [Option<&[f32]>; 3],
    threshold: f32,
) -> Vec<FaceBox> {
    let mut faces = Vec::new();
    for (level, stride) in STRIDES.iter().copied().enumerate() {
        let fw = input_w / stride;
        let fh = input_h / stride;
        let expect = (fw * fh * ANCHORS) as usize;
        if scores[level].len() < expect || distances[level].len() < expect * 4 {
            continue;
        }
        for y in 0..fh {
            for x in 0..fw {
                for anchor in 0..ANCHORS {
                    let index = ((y * fw + x) * ANCHORS + anchor) as usize;
                    let score = probability(scores[level][index]);
                    if score < threshold {
                        continue;
                    }
                    let d = &distances[level][index * 4..index * 4 + 4];
                    let cx = x as f32 * stride as f32;
                    let cy = y as f32 * stride as f32;
                    let x1 = cx - d[0] * stride as f32;
                    let y1 = cy - d[1] * stride as f32;
                    let x2 = cx + d[2] * stride as f32;
                    let y2 = cy + d[3] * stride as f32;
                    let rect = rect_from(x1, y1, x2, y2, input_w, input_h);
                    let Some(rect) = rect else { continue };
                    let landmarks = keypoints[level].and_then(|kps| {
                        if kps.len() < (index + 1) * 10 {
                            return None;
                        }
                        let raw = &kps[index * 10..index * 10 + 10];
                        let mut points = [(0f32, 0f32); 5];
                        for p in 0..5 {
                            points[p] = (cx + raw[p * 2] * stride as f32, cy + raw[p * 2 + 1] * stride as f32);
                        }
                        Some(points)
                    });
                    faces.push(FaceBox { rect, score, landmarks });
                }
            }
        }
    }
    nms(&mut faces, 0.4);
    faces
}

fn probability(value: f32) -> f32 {
    if (0.0..=1.0).contains(&value) {
        value
    } else {
        1.0 / (1.0 + (-value).exp())
    }
}

fn rect_from(x1: f32, y1: f32, x2: f32, y2: f32, width: u32, height: u32) -> Option<Rect> {
    let left = x1.max(0.0).min(width as f32);
    let top = y1.max(0.0).min(height as f32);
    let right = x2.max(0.0).min(width as f32);
    let bottom = y2.max(0.0).min(height as f32);
    let w = (right - left).round() as i32;
    let h = (bottom - top).round() as i32;
    if w < 1 || h < 1 {
        return None;
    }
    Some(Rect { x: left.round() as u32, y: top.round() as u32, w: w as u32, h: h as u32 })
}

fn nms(faces: &mut Vec<FaceBox>, iou_threshold: f32) {
    faces.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<FaceBox> = Vec::new();
    for face in faces.drain(..) {
        if kept.iter().any(|other| iou(face.rect, other.rect) >= iou_threshold) {
            continue;
        }
        kept.push(face);
    }
    *faces = kept;
}

fn iou(a: Rect, b: Rect) -> f32 {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.w).min(b.x + b.w);
    let y2 = (a.y + a.h).min(b.y + b.h);
    let iw = x2.saturating_sub(x1);
    let ih = y2.saturating_sub(y1);
    let inter = (iw * ih) as f32;
    let union = (a.w * a.h + b.w * b.h) as f32 - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

/// Map a box from the square network input back onto the detection image.
#[derive(Clone, Copy, Debug)]
pub struct Letterbox {
    pub scale: f32,
    pub pad_x: f32,
    pub pad_y: f32,
}

impl Letterbox {
    pub fn fit(src_w: u32, src_h: u32, dst: u32) -> Self {
        let scale = (dst as f32 / src_w as f32).min(dst as f32 / src_h as f32);
        let pad_x = (dst as f32 - src_w as f32 * scale) / 2.0;
        let pad_y = (dst as f32 - src_h as f32 * scale) / 2.0;
        Self { scale, pad_x, pad_y }
    }

    pub fn to_source(&self, rect: Rect) -> Option<Rect> {
        if self.scale <= 0.0 {
            return None;
        }
        let x1 = (rect.x as f32 - self.pad_x) / self.scale;
        let y1 = (rect.y as f32 - self.pad_y) / self.scale;
        let x2 = (rect.x as f32 + rect.w as f32 - self.pad_x) / self.scale;
        let y2 = (rect.y as f32 + rect.h as f32 - self.pad_y) / self.scale;
        rect_from(x1, y1, x2, y2, u32::MAX, u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_are_in_stride_units_from_the_cell_origin() {
        let mut scores = [vec![], vec![], vec![]];
        let mut boxes = [vec![], vec![], vec![]];
        // stride 16, one cell, two anchors. Only the first anchor scores.
        scores[1] = vec![0.9, 0.01];
        boxes[1] = vec![2.0, 0.5, 1.0, 3.0, 0.0, 0.0, 0.0, 0.0];
        // Make the grid 1x1 by using input 16x16. Other levels have empty scores and are skipped.
        // stride 8 on 16x16 expects 2*2*2=8 scores. Provide zeros so they are skipped by threshold.
        scores[0] = vec![0.0; 8];
        boxes[0] = vec![0.0; 32];
        scores[2] = vec![0.0; 2];
        boxes[2] = vec![0.0; 8];
        // input 16 is not divisible... 16/32 = 0, expect 0, empty is ok if we pass empty and expect 0.
        scores[2] = vec![];
        boxes[2] = vec![];
        let faces = decode(16, 16, [&scores[0], &scores[1], &scores[2]], [&boxes[0], &boxes[1], &boxes[2]], [None, None, None], 0.5);
        // stride 16 grid is 1x1, anchor 0: cx=0, cy=0 because x=0,y=0. That's the corner cell only if fw=1.
        // 16/16=1. cx=0. x1=0-2*16=-32 -> 0, y1=0-0.5*16=-8 -> 0, x2=0+1*16=16, y2=0+3*16=48 -> 16.
        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0].rect, Rect { x: 0, y: 0, w: 16, h: 16 });
    }

    #[test]
    fn a_cell_off_the_origin_uses_stride_times_index() {
        // 32x32 input. stride 16 => 2x2 cells, 8 anchors. Put the score on x=1,y=0, anchor 0.
        // index = (y * 2 + x) * 2 + anchor = 2.
        let mut score = vec![0f32; 8];
        score[2] = 0.8;
        let mut dist = vec![0f32; 32];
        dist[8..12].copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
        let zeros_s: Vec<f32> = vec![];
        let zeros_b: Vec<f32> = vec![];
        let s8 = vec![0f32; 32];
        let b8 = vec![0f32; 128];
        let faces = decode(32, 32, [&s8, &score, &zeros_s], [&b8, &dist, &zeros_b], [None, None, None], 0.5);
        assert_eq!(faces.len(), 1);
        // cx = 1*16 = 16, cy = 0. x1=0, y1=0, x2=32, y2=16.
        assert_eq!(faces[0].rect, Rect { x: 0, y: 0, w: 32, h: 16 });
    }

    #[test]
    fn overlapping_boxes_keep_the_higher_score() {
        let mut score = vec![0f32; 8];
        score[0] = 0.95;
        score[1] = 0.6;
        let dist = vec![1f32; 32];
        let s8 = vec![0f32; 32];
        let b8 = vec![0f32; 128];
        let empty_s = vec![];
        let empty_b = vec![];
        let faces = decode(32, 32, [&s8, &score, &empty_s], [&b8, &dist, &empty_b], [None, None, None], 0.5);
        assert_eq!(faces.len(), 1);
        assert!((faces[0].score - 0.95).abs() < 1e-5);
    }

    #[test]
    fn letterbox_undoes_the_pad() {
        let fit = Letterbox::fit(640, 480, 640);
        assert!((fit.scale - 1.0).abs() < 1e-5);
        assert!((fit.pad_y - 80.0).abs() < 1e-3);
        let source = fit.to_source(Rect { x: 80, y: 80, w: 64, h: 64 }).unwrap();
        assert_eq!(source.y, 0);
        assert_eq!(source.x, 80);
    }
}
