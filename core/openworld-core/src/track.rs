// SPDX-License-Identifier: Apache-2.0

//! A small ByteTrack for faces: confident detections are matched first, then
//! the weak leftovers, each against where the track is predicted to be.
//!
//! A track is kept for one second after its face is last seen, measured in
//! file time, so `measured` coverage (five frames a second) keeps the same
//! track across samples. A walking face is followed by its velocity, and by
//! its center when the boxes stop overlapping between samples.

use crate::geom::Rect;

/// A face lost for longer than this, in seconds of file time, ends its track.
pub const LOST_SECONDS: f64 = 1.0;
const IOU_MIN: f32 = 0.3;
/// Center distance, in face widths, accepted when boxes no longer overlap.
const CENTER_REACH: f32 = 0.6;

#[derive(Clone, Debug)]
pub struct TrackDet {
    pub rect: Rect,
    pub score: f32,
}

#[derive(Clone, Debug)]
struct Track {
    id: u64,
    /// Center and size, in pixels, at `seen`.
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    /// Pixels per second.
    vx: f32,
    vy: f32,
    seen: f64,
}

impl Track {
    fn predicted(&self, t: f64) -> Rect {
        let dt = (t - self.seen).clamp(0.0, LOST_SECONDS) as f32;
        let cx = self.cx + self.vx * dt;
        let cy = self.cy + self.vy * dt;
        Rect {
            x: (cx - self.w / 2.0).max(0.0) as u32,
            y: (cy - self.h / 2.0).max(0.0) as u32,
            w: self.w.max(1.0) as u32,
            h: self.h.max(1.0) as u32,
        }
    }

    fn update(&mut self, rect: Rect, t: f64) {
        let (cx, cy) = center(rect);
        let dt = (t - self.seen) as f32;
        if dt > 0.0 {
            // A light filter: half the new motion, half the old.
            self.vx = 0.5 * self.vx + 0.5 * (cx - self.cx) / dt;
            self.vy = 0.5 * self.vy + 0.5 * (cy - self.cy) / dt;
        }
        self.cx = cx;
        self.cy = cy;
        self.w = 0.7 * rect.w as f32 + 0.3 * self.w;
        self.h = 0.7 * rect.h as f32 + 0.3 * self.h;
        self.seen = t;
    }
}

fn center(rect: Rect) -> (f32, f32) {
    (rect.x as f32 + rect.w as f32 / 2.0, rect.y as f32 + rect.h as f32 / 2.0)
}

/// What one frame did to the tracks.
#[derive(Debug, Default)]
pub struct Update {
    /// The track id of each detection, in order.
    pub ids: Vec<u64>,
    /// Tracks that ended on this frame. Their faces are gone.
    pub ended: Vec<u64>,
}

pub struct ByteTrack {
    next_id: u64,
    tracks: Vec<Track>,
    high: f32,
}

impl ByteTrack {
    /// `high` splits confident detections from weak ones for the first pass.
    pub fn new(high: f32) -> Self {
        Self { next_id: 1, tracks: Vec::new(), high }
    }

    /// `t` is the frame's time in seconds of file time.
    pub fn update(&mut self, dets: &[TrackDet], t: f64) -> Update {
        let mut ids = vec![0u64; dets.len()];
        let mut used_tracks = vec![false; self.tracks.len()];
        let mut used_dets = vec![false; dets.len()];
        let high = self.high;
        self.associate(dets, t, &mut ids, &mut used_tracks, &mut used_dets, |d| d.score >= high);
        self.associate(dets, t, &mut ids, &mut used_tracks, &mut used_dets, |_| true);
        for (i, det) in dets.iter().enumerate() {
            if used_dets[i] {
                continue;
            }
            let id = self.next_id;
            self.next_id += 1;
            let (cx, cy) = center(det.rect);
            self.tracks.push(Track { id, cx, cy, w: det.rect.w as f32, h: det.rect.h as f32, vx: 0.0, vy: 0.0, seen: t });
            ids[i] = id;
        }
        let mut ended = Vec::new();
        self.tracks.retain(|track| {
            let keep = t - track.seen <= LOST_SECONDS;
            if !keep {
                ended.push(track.id);
            }
            keep
        });
        Update { ids, ended }
    }

    /// Every track still open. The scan calls this at the end of the file.
    pub fn close_all(&mut self) -> Vec<u64> {
        self.tracks.drain(..).map(|track| track.id).collect()
    }

    fn associate(
        &mut self,
        dets: &[TrackDet],
        t: f64,
        ids: &mut [u64],
        used_tracks: &mut [bool],
        used_dets: &mut [bool],
        eligible: impl Fn(&TrackDet) -> bool,
    ) {
        // (cost, track, detection). Lower cost is a better match.
        let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
        for (ti, track) in self.tracks.iter().enumerate() {
            if used_tracks[ti] {
                continue;
            }
            let predicted = track.predicted(t);
            for (di, det) in dets.iter().enumerate() {
                if used_dets[di] || !eligible(det) {
                    continue;
                }
                let iou = predicted.iou(det.rect);
                if iou >= IOU_MIN {
                    pairs.push((1.0 - iou, ti, di));
                    continue;
                }
                // Between sparse samples a moving face may not overlap its prediction.
                let size = det.rect.w.max(det.rect.h) as f32;
                let ratio = det.rect.w as f32 / track.w.max(1.0);
                let (dx, dy) = (center(det.rect).0 - center(predicted).0, center(det.rect).1 - center(predicted).1);
                let reach = (dx * dx + dy * dy).sqrt() / size.max(1.0);
                if reach <= CENTER_REACH && (0.67..=1.5).contains(&ratio) {
                    pairs.push((1.0 + reach, ti, di));
                }
            }
        }
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_cost, ti, di) in pairs {
            if used_tracks[ti] || used_dets[di] {
                continue;
            }
            used_tracks[ti] = true;
            used_dets[di] = true;
            self.tracks[ti].update(dets[di].rect, t);
            ids[di] = self.tracks[ti].id;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn det(x: u32, y: u32, w: u32) -> TrackDet {
        TrackDet { rect: Rect { x, y, w, h: w }, score: 0.95 }
    }

    #[test]
    fn overlapping_faces_keep_an_id() {
        let mut tracker = ByteTrack::new(0.8);
        let a = tracker.update(&[det(10, 10, 80)], 0.0);
        let b = tracker.update(&[det(18, 14, 80)], 1.0 / 30.0);
        assert_eq!(a.ids[0], b.ids[0]);
        assert!(b.ended.is_empty());
    }

    #[test]
    fn a_walking_face_keeps_its_id_between_measured_samples() {
        // 150 px a second, sampled five times a second: 30 px between samples on a 60 px face.
        let mut tracker = ByteTrack::new(0.8);
        let mut ids = Vec::new();
        for k in 0..10u32 {
            let update = tracker.update(&[det(20 + k * 30, 100, 60)], f64::from(k) / 5.0);
            ids.push(update.ids[0]);
        }
        assert!(ids.iter().all(|id| *id == ids[0]), "{ids:?}");
    }

    #[test]
    fn a_track_ends_after_a_second_without_its_face() {
        let mut tracker = ByteTrack::new(0.8);
        let first = tracker.update(&[det(10, 10, 80)], 0.0).ids[0];
        assert!(tracker.update(&[], 0.9).ended.is_empty());
        assert_eq!(tracker.update(&[], 1.1).ended, vec![first]);
        let next = tracker.update(&[det(10, 10, 80)], 1.2).ids[0];
        assert_ne!(first, next);
        assert_eq!(tracker.close_all(), vec![next]);
    }

    #[test]
    fn two_faces_side_by_side_keep_separate_ids() {
        let mut tracker = ByteTrack::new(0.8);
        let a = tracker.update(&[det(0, 0, 100), det(300, 0, 100)], 0.0).ids;
        let b = tracker.update(&[det(305, 0, 100), det(5, 0, 100)], 0.04).ids;
        assert_eq!(a[0], b[1]);
        assert_eq!(a[1], b[0]);
    }
}
