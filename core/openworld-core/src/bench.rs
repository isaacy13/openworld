// SPDX-License-Identifier: Apache-2.0

//! Time a bundle on this device, on each provider that registers, and check
//! that an accelerated provider scores like the CPU.
//!
//! The cutoff was measured on the CPU. A neural engine or GPU that runs the
//! embedder in half precision can move scores. This reports, for each
//! provider, the lowest cosine between its embedding and the CPU's for the
//! same aligned crops. A provider whose scores drift is a provider the curve
//! does not describe.

use crate::bundle::Bundle;
use crate::onnx_exec;
use crate::perception::{auto_long_side, cosine, Aligned, Perception};
use image::{Rgb, RgbImage};
use serde::Serialize;

/// The files a phone or a camera makes: 1080p, 4K, and a 12-megapixel photo.
pub const SIZES: [(u32, u32); 3] = [(1920, 1080), (3840, 2160), (4032, 3024)];

#[derive(Debug, Serialize)]
pub struct ProviderRun {
    pub provider: String,
    pub execution: String,
    pub sizes: Vec<SizeRun>,
    pub embed_ms: f64,
    /// Lowest cosine between this provider's embedding and the CPU's, over the test crops.
    pub min_cosine_vs_cpu: Option<f32>,
}

#[derive(Debug, Serialize)]
pub struct SizeRun {
    pub width: u32,
    pub height: u32,
    pub detection_long_side: u32,
    pub detect_ms: f64,
    /// Frames a second this device can analyze at this size.
    pub frames_per_second: f64,
}

#[derive(Debug, Serialize)]
pub struct Bench {
    pub schema: String,
    pub bundle: String,
    pub os: String,
    pub arch: String,
    pub runs: Vec<ProviderRun>,
}

/// Deterministic textured crops: the drift check needs the same input on each provider.
fn crops() -> Vec<Aligned> {
    (0..6u32)
        .map(|k| {
            Aligned::Face(RgbImage::from_fn(112, 112, |x, y| {
                let v = ((x * (3 + k) + y * (5 + 2 * k) + (x * y) % (7 + k)) % 256) as u8;
                Rgb([v, v.wrapping_mul(3), v.wrapping_add(90)])
            }))
        })
        .collect()
}

pub fn bench(bundle: &Bundle) -> Result<Bench, String> {
    let mut runs = Vec::new();
    let mut cpu_vectors: Option<Vec<Vec<f32>>> = None;
    for accelerate in [false, true] {
        let mut models = onnx_exec::load(bundle, accelerate)?;
        if accelerate && models.provider() == "CPUExecutionProvider" {
            break;
        }
        let mut sizes = Vec::new();
        let mut embed_ms = 0.0;
        for (width, height) in SIZES {
            let det_long = auto_long_side(width.max(height), models.min_face_px());
            let timing = models.time(det_long).ok_or("The networks did not run. Refusing.")?;
            embed_ms = timing.embed_ms;
            sizes.push(SizeRun {
                width,
                height,
                detection_long_side: det_long,
                detect_ms: timing.detect_ms,
                frames_per_second: 1000.0 / timing.detect_ms.max(1e-3),
            });
        }
        let vectors: Vec<Vec<f32>> = crops().iter().map(|crop| models.embed(crop)).collect::<Result<_, _>>()?;
        let min_cosine_vs_cpu = cpu_vectors.as_ref().map(|cpu| {
            cpu.iter().zip(&vectors).filter_map(|(a, b)| cosine(a, b)).fold(f32::MAX, f32::min)
        });
        if !accelerate {
            cpu_vectors = Some(vectors);
        }
        runs.push(ProviderRun {
            provider: models.provider().to_string(),
            execution: crate::runtime::execution(models.provider()).as_str().into(),
            sizes,
            embed_ms,
            min_cosine_vs_cpu,
        });
    }
    Ok(Bench {
        schema: "openworld.bench.v1".into(),
        bundle: format!("{} {}", bundle.id, bundle.version),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        runs,
    })
}
