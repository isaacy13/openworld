// SPDX-License-Identifier: Apache-2.0

//! One synthetic still: a face large enough to compare, a face that is only
//! inventoried, a vehicle, and a plate. No photograph of a person.

use crate::embed::fixture_probe;
use crate::fiducial::{self, render_face_module, render_plate, render_vehicle};
use crate::geom::blank;
use image::RgbImage;

pub struct DemoLayout {
    pub image: RgbImage,
    pub compared_id: u16,
}

pub fn demo_scene(threshold: f32) -> DemoLayout {
    let compared_id = 7u16;
    let marker = render_face_module(compared_id, 16);
    let (mw, mh) = marker.dimensions();
    let (x, y) = passing_origin(compared_id, mw, mh, threshold);
    let mut image = blank(640, 480);
    fiducial::place(&mut image, &marker, x, y);
    let small = render_face_module(11, 8);
    fiducial::place(&mut image, &small, 420, 36);
    let vehicle = render_vehicle(8);
    fiducial::place(&mut image, &vehicle, 420, 180);
    if let Some(plate) = render_plate("FIX123", 8) {
        fiducial::place(&mut image, &plate, 36, 360);
    }
    DemoLayout { image, compared_id }
}

fn passing_origin(id: u16, w: u32, h: u32, threshold: f32) -> (u32, u32) {
    let mut fallback = (16u32, 16u32);
    for y in (12..70).step_by(2) {
        for x in (12..70).step_by(2) {
            let (_probe, score) = fixture_probe(id, x, y, w, h, 0);
            fallback = (x, y);
            if score >= threshold {
                return (x, y);
            }
        }
    }
    fallback
}
