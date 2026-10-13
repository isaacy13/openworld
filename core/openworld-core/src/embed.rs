// SPDX-License-Identifier: Apache-2.0

//! Fixture embeddings. These are not ArcFace features.
//!
//! A locked cosine still runs on them so the Fast cutoff has a measured curve.
//! The residual depends on where the marker was placed, which lets repeated
//! trials cover a range of genuine scores. It is not a claim about real faces.

pub const EMBED_DIM: usize = 512;

#[derive(Clone, Copy)]
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    fn next_unit(&mut self) -> f32 {
        let n = (self.next() >> 40) as f32;
        n / (1u32 << 24) as f32 - 0.5
    }
}

pub fn unit_embedding(id: u16) -> Vec<f32> {
    let mut rng = SplitMix(0x0EED_F00D ^ u64::from(id).wrapping_mul(0x9E37));
    let mut v = vec![0f32; EMBED_DIM];
    for x in &mut v {
        *x = rng.next_unit();
    }
    normalize(&mut v);
    v
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let mut d = 0f32;
    for i in 0..n {
        d += a[i] * b[i];
    }
    d
}

/// Probe embedding for one fixture face, plus the cosine against the clean poster vector.
pub fn fixture_probe(id: u16, x: u32, y: u32, w: u32, h: u32, frame: u64) -> (Vec<f32>, f32) {
    let base = unit_embedding(id);
    let salt = placement_salt(x, y, w, h, frame);
    let alpha = (salt % 100) as f32 / 99.0 * 0.85;
    let mut rng = SplitMix(0x51A7 ^ u64::from(salt));
    let mut noise = vec![0f32; EMBED_DIM];
    for v in &mut noise {
        *v = rng.next_unit();
    }
    let dot = cosine(&base, &noise);
    for i in 0..EMBED_DIM {
        noise[i] -= dot * base[i];
    }
    normalize(&mut noise);
    let mut out = vec![0f32; EMBED_DIM];
    for i in 0..EMBED_DIM {
        out[i] = (1.0 - alpha) * base[i] + alpha * noise[i];
    }
    normalize(&mut out);
    let score = cosine(&out, &base);
    (out, score)
}

pub fn placement_salt(x: u32, y: u32, w: u32, h: u32, frame: u64) -> u32 {
    let mut hash = 0x811c9dc5u32;
    for v in [x, y, w, h, frame as u32, (frame >> 32) as u32] {
        hash ^= v;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

fn normalize(v: &mut [f32]) {
    let mut n = 0f32;
    for x in v.iter() {
        n += x * x;
    }
    let n = n.sqrt().max(1e-8);
    for x in v.iter_mut() {
        *x /= n;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_posters_score_one() {
        let a = unit_embedding(7);
        assert!((cosine(&a, &a) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn probe_cosine_matches_the_mix() {
        let (_probe, score) = fixture_probe(7, 20, 24, 128, 128, 0);
        assert!(score > 0.1 && score <= 1.0);
    }
}
