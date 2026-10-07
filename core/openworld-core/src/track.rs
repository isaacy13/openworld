// SPDX-License-Identifier: Apache-2.0

//! A small ByteTrack: high-score matches first, then low-score leftovers.
//! Tracks are class-specific so a plate never inherits a face id.

use crate::geom::Rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackClass {
    Face,
    Vehicle,
    Plate,
}

#[derive(Clone, Debug)]
pub struct TrackDet {
    pub class: TrackClass,
    pub rect: Rect,
    pub score: f32,
}

#[derive(Clone, Debug)]
struct Track {
    id: u64,
    class: TrackClass,
    rect: Rect,
    lost: u32,
}

pub struct ByteTrack {
    next_id: u64,
    tracks: Vec<Track>,
    max_lost: u32,
    iou_min: f32,
}

impl ByteTrack {
    pub fn new() -> Self {
        Self { next_id: 1, tracks: Vec::new(), max_lost: 2, iou_min: 0.3 }
    }

    pub fn update(&mut self, dets: &[TrackDet]) -> Vec<u64> {
        let mut ids = vec![0u64; dets.len()];
        let mut used_tracks = vec![false; self.tracks.len()];
        let mut used_dets = vec![false; dets.len()];

        self.associate(dets, &mut ids, &mut used_tracks, &mut used_dets, 0.5);
        self.associate(dets, &mut ids, &mut used_tracks, &mut used_dets, 0.1);

        for (i, det) in dets.iter().enumerate() {
            if used_dets[i] {
                continue;
            }
            let id = self.next_id;
            self.next_id += 1;
            self.tracks.push(Track { id, class: det.class, rect: det.rect, lost: 0 });
            ids[i] = id;
        }

        for (t, used) in self.tracks.iter_mut().zip(used_tracks.iter()) {
            if !used {
                t.lost = t.lost.saturating_add(1);
            }
        }
        self.tracks.retain(|t| t.lost <= self.max_lost);
        ids
    }

    fn associate(
        &mut self,
        dets: &[TrackDet],
        ids: &mut [u64],
        used_tracks: &mut [bool],
        used_dets: &mut [bool],
        min_score: f32,
    ) {
        let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
        for (ti, track) in self.tracks.iter().enumerate() {
            if used_tracks[ti] {
                continue;
            }
            for (di, det) in dets.iter().enumerate() {
                if used_dets[di] || det.score < min_score || det.class != track.class {
                    continue;
                }
                let iou = track.rect.iou(det.rect);
                if iou >= self.iou_min {
                    pairs.push((iou, ti, di));
                }
            }
        }
        pairs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (_iou, ti, di) in pairs {
            if used_tracks[ti] || used_dets[di] {
                continue;
            }
            used_tracks[ti] = true;
            used_dets[di] = true;
            self.tracks[ti].rect = dets[di].rect;
            self.tracks[ti].lost = 0;
            ids[di] = self.tracks[ti].id;
        }
    }
}

impl Default for ByteTrack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_faces_keep_an_id() {
        let mut tracker = ByteTrack::new();
        let a = TrackDet {
            class: TrackClass::Face,
            rect: Rect { x: 10, y: 10, w: 80, h: 80 },
            score: 0.99,
        };
        let ids1 = tracker.update(&[a.clone()]);
        let b = TrackDet {
            class: TrackClass::Face,
            rect: Rect { x: 18, y: 14, w: 80, h: 80 },
            score: 0.99,
        };
        let ids2 = tracker.update(&[b]);
        assert_eq!(ids1[0], ids2[0]);
        let plate = TrackDet {
            class: TrackClass::Plate,
            rect: Rect { x: 18, y: 14, w: 80, h: 40 },
            score: 0.99,
        };
        let ids3 = tracker.update(&[plate]);
        assert_ne!(ids3[0], ids1[0]);
    }
}
