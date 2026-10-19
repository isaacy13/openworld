// SPDX-License-Identifier: Apache-2.0

//! Desktop decoding: every picture a player shows reaches the scan, turned and
//! sized as shown, and nothing that is not shown does.
//!
//! A marker on one picture of an animation or video is a track of one frame.
//! It is decoded, compared, and passes the cutoff, and the video rule calls it
//! too brief to be a candidate. These tests accept either status as proof that
//! the picture was read; where a marker spans three or more frames they ask for
//! a candidate.

mod common;

use common::*;
use image::RgbImage;
use openworld_core::copy;
use openworld_core::decode::{self, container_created, gif_delays, picture_delays};
use openworld_core::estimate::Coverage;
use openworld_core::geom::{frames_on_screen, sample_indices};
use openworld_core::report::TrackStatus;
use openworld_core::scan::estimate_for;
use openworld_core::timeutil::parse_rfc3339;
use std::fs;
use std::path::{Path, PathBuf};

fn h264(lab: &Lab, frames: &[RgbImage], fps: u32, name: &str) -> PathBuf {
    let out = lab.path(name);
    encode(lab.dir.path(), frames, fps, H264, &out);
    out
}

/// A palette GIF of `frames`, each shown for its delay in seconds.
fn gif(lab: &Lab, frames: &[RgbImage], delays: &[f64], name: &str) -> PathBuf {
    let pattern = png_frames(lab.dir.path(), frames);
    let folder = pattern.parent().unwrap();
    let mut script = String::from("ffconcat version 1.0\n");
    for (i, delay) in delays.iter().enumerate() {
        script.push_str(&format!("file '{}'\nduration {delay}\n", folder.join(format!("f{i:03}.png")).display()));
    }
    script.push_str(&format!("file '{}'\n", folder.join(format!("f{:03}.png", delays.len() - 1)).display()));
    let list = folder.join("list.txt");
    fs::write(&list, script).unwrap();
    let palette = folder.join("palette.png");
    let count = frames.len().to_string();
    let concat = |extra: &[&str], out: &Path| {
        let mut args: Vec<std::ffi::OsString> = ["-f", "concat", "-safe", "0", "-i"].iter().map(Into::into).collect();
        args.push(list.clone().into());
        args.extend(extra.iter().map(Into::into));
        args.push(out.into());
        ffmpeg(args);
    };
    concat(&["-frames:v", &count, "-vf", "palettegen=stats_mode=full:max_colors=32"], &palette);
    let out = lab.path(name);
    let palette_arg = palette.to_string_lossy().to_string();
    concat(&["-i", &palette_arg, "-frames:v", &count, "-lavfi", "paletteuse=dither=none", "-loop", "0"], &out);
    out
}

#[test]
fn a_lossless_video_is_read_frame_by_frame() {
    let lab = Lab::new();
    let (shown, _) = scene(MISSING_ID, &frames(0..3));
    let video = lab.path("clip.mkv");
    encode(lab.dir.path(), &sequence(6, 0..3, &shown), 30, FFV1, &video);
    let report = lab.scan(&video);
    assert_candidate(&report, MISSING_POSTER);
    assert_eq!((report.counts.frames_decoded, report.counts.frames_analyzed), (6, 6), "{}", describe(&report));
    assert_eq!(report.counts.frames_declared, Some(6));
    assert!(report.coverage_banner.is_none());
    let track = &report.tracks[0];
    assert_eq!((track.first_frame, track.last_frame, track.quality_frames), (0, 2, 3), "{}", describe(&report));
    assert!(track.frames.text == "Frames 1 to 3.", "{:?}", track.frames);
}

#[test]
fn a_one_frame_gif_is_not_repeated_into_extra_frames() {
    let lab = Lab::new();
    let still = lab.png("blue.png", &blank(64, 64));
    let gif = lab.path("blue.gif");
    ffmpeg(["-i".as_ref(), still.as_os_str(), "-frames:v".as_ref(), "1".as_ref(), gif.as_os_str()]);
    let report = lab.scan(&gif);
    assert_no_candidate(&report);
    assert_eq!(report.counts.frames_decoded, 1);
    assert!(report.tracks.is_empty());
}

#[test]
fn a_later_gif_image_is_scanned_once() {
    // FFmpeg's GIF decoder repeats each image until the next one. Stopping after
    // the container's count kept two copies of the first image, and a face on the
    // second image was never read.
    let lab = Lab::new();
    let (shown, _) = scene(MISSING_ID, &[1]);
    let gif = gif(&lab, &[plain(0), shown], &[0.04, 0.04], "later.gif");
    let report = lab.scan(&gif);
    assert_eq!(report.counts.frames_decoded, 2, "{}", describe(&report));
    let track = assert_passed_at(&report, 1);
    assert_eq!(track.last_frame, 1);
}

#[test]
fn an_animated_png_keeps_a_face_that_is_not_on_the_first_frame() {
    let lab = Lab::new();
    let (shown, _) = scene(MISSING_ID, &[1]);
    let apng = lab.path("later.apng");
    encode(lab.dir.path(), &sequence(3, 1..2, &shown), 5, &["-plays", "1", "-f", "apng"], &apng);
    let report = lab.scan(&apng);
    assert_eq!(report.counts.frames_decoded, 3, "{}", describe(&report));
    let track = assert_passed_at(&report, 1);
    assert_eq!(track.last_frame, 1);
    assert_eq!(track.frames.text, "Frame 2.");
    assert_eq!(report.tracks.len(), 1, "{}", describe(&report));
}

#[test]
fn a_held_gif_frame_is_read_at_the_tick_it_is_on_screen() {
    // Twenty images; the tenth stays up for a second. The average rate steps past
    // it. Measured coverage reads the picture on screen at each 5 Hz tick.
    let lab = Lab::new();
    let (shown, _) = scene(MISSING_ID, &[9]);
    let images = sequence(20, 9..10, &shown);
    let delays: Vec<f64> = (0..20).map(|i| if i == 9 { 1.0 } else { 0.04 }).collect();
    let gif = gif(&lab, &images, &delays, "hold.gif");
    let read = gif_delays(&gif).expect("delays");
    assert_eq!(read.len(), 20);
    let on_screen = frames_on_screen(&read);
    assert!(on_screen.contains(&9), "{on_screen:?}");
    let report = lab.scan_measured(&gif);
    assert_passed_at(&report, 9);
    assert_eq!(report.counts.frames_analyzed, on_screen.len() as u64, "{}", describe(&report));
    assert!(report.coverage_banner.as_ref().is_some_and(|l| l.is(copy::BRIEF_FACE)));
}

#[test]
fn a_held_gif_estimate_counts_the_frames_on_screen() {
    let lab = Lab::new();
    let images: Vec<RgbImage> = (0..20).map(plain).collect();
    let delays: Vec<f64> = (0..20).map(|i| if i == 9 { 1.0 } else { 0.04 }).collect();
    let gif = gif(&lab, &images, &delays, "hold.gif");
    let on_screen = frames_on_screen(&gif_delays(&gif).unwrap());
    let probed = decode::probe(&gif).unwrap();
    let by_rate = sample_indices(probed.frames, probed.fps, true);
    assert_ne!(on_screen, by_rate);
    let mut req = lab.request(&gif);
    req.coverage = Coverage::Measured;
    let estimate = estimate_for(&req).expect("estimate");
    assert_eq!(estimate.frames_analyzed, on_screen.len() as u64, "by rate {by_rate:?} on screen {on_screen:?}");
    assert!(estimate.coverage_banner.is_some());
}

#[test]
fn a_picture_after_a_held_frame_is_scanned() {
    // The first picture stays up a second. FFmpeg repeats it to fill a steady
    // rate; stopping after the container's count kept those repeats and never
    // read the last picture.
    let lab = Lab::new();
    let (shown, _) = scene(MISSING_ID, &[19]);
    let steady = lab.path("steady.mp4");
    let mut options = H264_INTRA.to_vec();
    options.extend(["-video_track_timescale", "12800"]);
    encode(lab.dir.path(), &sequence(20, 19..20, &shown), 25, &options, &steady);
    let held = lab.path("held.mp4");
    remux(&steady, &["-c", "copy", "-bsf:v", &hold_before(1, 12800)], &held);
    let report = lab.scan(&held);
    assert_eq!(report.counts.frames_decoded, 20, "{}", describe(&report));
    assert_passed_at(&report, 19);
}

/// Twenty pictures at 25 fps; the tenth stays up a second.
fn held_tenth(lab: &Lab, extension: &str) -> PathBuf {
    let (shown, _) = scene(MISSING_ID, &[9]);
    let steady = lab.path(&format!("steady.{extension}"));
    let mut options = H264_INTRA.to_vec();
    if extension == "mp4" {
        options.extend(["-video_track_timescale", "12800"]);
    }
    encode(lab.dir.path(), &sequence(20, 9..10, &shown), 25, &options, &steady);
    let held = lab.path(&format!("held.{extension}"));
    remux(&steady, &["-c", "copy", "-bsf:v", &hold_before(10, 12800)], &held);
    held
}

#[test]
fn a_held_video_frame_is_read_at_the_tick_it_is_on_screen() {
    let lab = Lab::new();
    let held = held_tenth(&lab, "mp4");
    let delays = picture_delays(&held).expect("timestamps");
    assert!(frames_on_screen(&delays).contains(&9));
    let mut req = lab.request(&held);
    req.coverage = Coverage::Measured;
    let report = run(&req);
    assert_eq!(report.counts.frames_decoded, 20, "{}", describe(&report));
    assert_passed_at(&report, 9);
    assert_eq!(estimate_for(&req).unwrap().frames_analyzed, report.counts.frames_analyzed);
}

#[test]
fn a_held_matroska_picture_is_on_screen_without_a_frame_count() {
    // Matroska has no frame count and keeps the nominal rate.
    let lab = Lab::new();
    let held = held_tenth(&lab, "mkv");
    let mut req = lab.request(&held);
    req.coverage = Coverage::Measured;
    let report = run(&req);
    assert_eq!(report.counts.frames_decoded, 20, "{}", describe(&report));
    assert_passed_at(&report, 9);
    assert_eq!(estimate_for(&req).unwrap().frames_analyzed, report.counts.frames_analyzed);
}

#[test]
fn an_avi_hold_is_the_picture_on_screen() {
    // The AVI header counts a nominal rate, so eight stored pictures look like
    // dozens. Measured coverage keeps the held picture; a full scan finishes.
    let lab = Lab::new();
    let (shown, _) = scene(MISSING_ID, &[3]);
    let steady = lab.path("steady.mp4");
    encode(lab.dir.path(), &sequence(8, 3..4, &shown), 25, H264_INTRA, &steady);
    let held = lab.path("held.mp4");
    remux(&steady, &["-c", "copy", "-bsf:v", &hold_before(4, 12800)], &held);
    let avi = lab.path("held.avi");
    remux(&held, &["-c", "copy"], &avi);
    for coverage in [Coverage::Measured, Coverage::Complete] {
        let mut req = lab.request(&avi);
        req.coverage = coverage;
        let report = run(&req);
        assert_eq!(report.counts.frames_decoded, 8, "{coverage:?} {}", describe(&report));
        assert_passed_at(&report, 3);
        assert_eq!(estimate_for(&req).unwrap().frames_analyzed, report.counts.frames_analyzed, "{coverage:?}");
    }
}

#[test]
fn a_theora_file_with_no_average_rate_is_scanned() {
    // Ogg Theora leaves avg_frame_rate at 0/0 and puts the rate on r_frame_rate.
    let lab = Lab::new();
    let (shown, _) = scene(MISSING_ID, &frames(0..4));
    let ogg = lab.path("scene.ogg");
    encode(lab.dir.path(), &sequence(4, 0..4, &shown), 10, &["-c:v", "libtheora", "-q:v", "8"], &ogg);
    let probed = decode::probe(&ogg).unwrap();
    assert!((probed.fps - 10.0).abs() < 0.01, "{probed:?}");
    let report = lab.scan(&ogg);
    assert_candidate(&report, MISSING_POSTER);
}

#[test]
fn container_clocks_are_read_from_mp4_and_matroska() {
    let lab = Lab::new();
    let make = |name: &str, codec: &str, clock: Option<&str>| {
        let path = lab.path(name);
        let mut args = vec!["-f", "lavfi", "-i", "color=c=blue:s=64x64", "-frames:v", "1", "-r", "1", "-an", "-c:v", codec];
        let tag = clock.map(|c| format!("creation_time={c}"));
        if let Some(tag) = &tag {
            args.extend(["-metadata", tag.as_str()]);
        }
        let mut args: Vec<std::ffi::OsString> = args.into_iter().map(Into::into).collect();
        args.push(path.clone().into());
        ffmpeg(args);
        path
    };
    let clock = parse_rfc3339("2020-01-01T00:00:00Z").unwrap();
    let dated = make("dated.mp4", "libx264", Some("2020-01-01T00:00:00Z"));
    let plain_mp4 = make("plain.mp4", "libx264", None);
    let mkv = make("dated.mkv", "ffv1", Some("2020-01-01T00:00:00Z"));
    for path in [&dated, &mkv] {
        assert_eq!(container_created(path), Some(clock), "{}", path.display());
        assert_eq!(decode::probe(path).unwrap().container_created, Some(clock), "{}", path.display());
    }
    assert!(container_created(&plain_mp4).is_none());
    assert!(decode::probe(&plain_mp4).unwrap().container_created.is_none());

    // A clock before 1970 is still a clock, and it disagrees with today's file time.
    let old = make("old.mkv", "ffv1", Some("1960-01-01T00:00:00Z"));
    let sixties = parse_rfc3339("1960-01-01T00:00:00Z").unwrap();
    assert_eq!(container_created(&old), Some(sixties));
    assert_eq!(decode::probe(&old).unwrap().container_created, Some(sixties));
    let report = lab.scan(&old);
    assert_complete(&report);
    assert!(codes(&report.warnings).contains(&"timestamps_disagree"), "{:?}", report.warnings);
    let recent = lab.scan(&plain_mp4);
    assert!(!codes(&recent.warnings).contains(&"timestamps_disagree"), "{:?}", recent.warnings);
}

// ---------------------------------------------------------------------------
// Camera orientation tags on stills.

/// The scene as shown, and as stored under orientation 6 (turned a quarter clockwise to show).
fn turned_scene() -> (RgbImage, RgbImage) {
    let (shown, _) = scene(MISSING_ID, &[0]);
    let stored = image::imageops::rotate270(&shown);
    (shown, stored)
}

/// The tagged file is scanned as shown and finds the marker. The same pixels
/// without the tag are the marker on its side, and nothing passes.
fn check_turned(name: &str, make: impl Fn(&RgbImage, u8) -> Vec<u8>) {
    let lab = Lab::new();
    let (shown, stored) = turned_scene();
    let side = lab.write(&format!("side.{name}"), &make(&stored, 6));
    let raw = lab.write(&format!("raw.{name}"), &make(&stored, 1));
    let probed = decode::probe(&side).unwrap();
    assert_eq!((probed.width, probed.height), shown.dimensions(), "{name}");
    assert!(!probed.video, "{name}");
    assert_candidate(&lab.scan(&side), MISSING_POSTER);
    let sideways = lab.scan(&raw);
    assert_no_candidate(&sideways);
}

#[test]
fn a_png_with_camera_orientation_is_scanned_as_shown() {
    check_turned("png", |stored, tag| png_with_orientation(&png_bytes(stored), tag));
}

#[test]
fn a_jpeg_with_camera_orientation_is_scanned_as_shown() {
    check_turned("jpg", jpeg_with_orientation);
    let lab = Lab::new();
    let upright = lab.write("upright.jpg", &jpeg_bytes(&turned_scene().0));
    assert_candidate(&lab.scan(&upright), MISSING_POSTER);
}

#[test]
fn a_webp_with_camera_orientation_is_scanned_as_shown() {
    check_turned("webp", webp_with_orientation);
}

#[test]
fn a_tiff_with_camera_orientation_is_scanned_as_shown() {
    check_turned("tif", |stored, tag| tiff(&[TiffPage::rgb(stored, u16::from(tag))]));
}

#[test]
fn a_cmyk_tiff_is_scanned_as_shown() {
    check_turned("tif", |stored, tag| tiff(&[TiffPage::cmyk(stored, u16::from(tag))]));
}

#[test]
fn a_sixteen_bit_tiff_is_scanned_as_shown() {
    check_turned("tif", |stored, tag| tiff(&[TiffPage::rgb16(stored, u16::from(tag))]));
}

#[test]
fn a_ycbcr_tiff_is_scanned_as_shown() {
    check_turned("tif", |stored, tag| tiff(&[TiffPage::ycbcr(stored, u16::from(tag))]));
}

#[test]
fn a_palette_tiff_is_scanned_as_shown() {
    check_turned("tif", |stored, tag| tiff(&[TiffPage::palette(stored, u16::from(tag))]));
}

#[test]
fn every_page_of_a_tiff_is_scanned() {
    let lab = Lab::new();
    let (shown, _) = scene(MISSING_ID, &[1]);
    let pages = lab.write("pages.tif", &tiff(&[TiffPage::rgb(&blank(8, 8), 1), TiffPage::rgb(&shown, 1)]));
    let probed = decode::probe(&pages).unwrap();
    assert!(probed.video);
    assert_eq!((probed.frames, probed.fps), (2, 1.0));
    for report in [lab.scan(&pages), lab.scan_measured(&pages)] {
        assert_eq!(report.counts.frames_decoded, 2, "{}", describe(&report));
        let track = assert_passed_at(&report, 1);
        assert_eq!(track.frames.text, "Frame 2.");
        assert!(report.tracks.iter().all(|t| t.first_frame == 1), "{}", describe(&report));
    }
}

#[test]
fn a_multipage_tiff_whose_pages_cannot_all_be_read_is_refused() {
    let lab = Lab::new();
    let (shown, _) = scene(MISSING_ID, &[0]);
    let pages = lab.write("palette-pages.tif", &tiff(&[TiffPage::palette(&shown, 1), TiffPage::palette(&blank(8, 8), 1)]));
    assert_refused(&lab.scan(&pages), copy::R_BAD_CODEC);
}

// ---------------------------------------------------------------------------
// Video tracks, rotation, pixel shape, and edit lists.

#[test]
fn a_larger_video_track_is_scanned_as_shown() {
    // The first track is a small blank. A player shows the larger one, the scene.
    let lab = Lab::new();
    let small = lab.path("blank.mp4");
    ffmpeg(["-f", "lavfi", "-i", "color=c=black:s=64x64:r=10:d=0.1", "-an", "-c:v", "libx264", "-pix_fmt", "yuv420p", small.to_str().unwrap()]);
    let (shown, _) = scene(MISSING_ID, &frames(0..6));
    let movie = h264(&lab, &sequence(8, 0..6, &shown), 10, "scene.mp4");
    let both = lab.path("both.mp4");
    ffmpeg(["-i", small.to_str().unwrap(), "-i", movie.to_str().unwrap(), "-map", "0:v:0", "-map", "1:v:0", "-c", "copy", both.to_str().unwrap()]);
    let report = lab.scan(&both);
    assert_eq!(report.counts.frames_decoded, 8, "{}", describe(&report));
    assert_candidate(&report, MISSING_POSTER);
}

#[test]
fn a_video_with_display_rotation_is_scanned_as_shown() {
    let lab = Lab::new();
    let (shown, _) = scene(MISSING_ID, &frames(0..6));
    let stored = lab.path("stored.mp4");
    let mut options = vec!["-vf", "transpose=2"];
    options.extend(H264);
    encode(lab.dir.path(), &sequence(8, 0..6, &shown), 10, &options, &stored);
    let tagged = lab.path("shown.mp4");
    remux(&stored, &["-an", "-c:v", "copy", "-bsf:v", "h264_metadata=display_orientation=insert:rotate=-90"], &tagged);
    let probed = decode::probe(&tagged).unwrap();
    assert_eq!((probed.width, probed.height), (W, H));
    assert_candidate(&lab.scan(&tagged), MISSING_POSTER);
    assert_no_candidate(&lab.scan(&stored));
}

#[test]
fn a_video_with_non_square_pixels_is_scanned_as_shown() {
    let lab = Lab::new();
    let (shown, _) = scene(MISSING_ID, &frames(0..4));
    let pictures = sequence(4, 0..4, &shown);
    let make = |name: &str, filter: &str| {
        let out = lab.path(name);
        let mut options = vec!["-vf", filter];
        options.extend(H264);
        encode(lab.dir.path(), &pictures, 10, &options, &out);
        out
    };
    let wide = make("wide.mp4", "scale=320:480,setsar=2/1");
    let squashed = make("squashed.mp4", "scale=320:480,setsar=1");
    let probed = decode::probe(&wide).unwrap();
    assert_eq!((probed.width, probed.height), (W, H));
    assert!(probed.square_pixels);
    assert_candidate(&lab.scan(&wide), MISSING_POSTER);
    assert_no_candidate(&lab.scan(&squashed));
}

#[test]
fn a_turned_video_with_non_square_pixels_is_scanned_as_shown() {
    let lab = Lab::new();
    let (shown, _) = scene(MISSING_ID, &frames(0..4));
    let pictures = sequence(4, 0..4, &shown);
    let make = |name: &str, filter: &str| {
        let out = lab.path(name);
        let mut options = vec!["-vf", filter];
        options.extend(H264);
        encode(lab.dir.path(), &pictures, 10, &options, &out);
        out
    };
    let tag = |src: &Path, name: &str| {
        let out = lab.path(name);
        remux(src, &["-an", "-c:v", "copy", "-bsf:v", "h264_metadata=display_orientation=insert:rotate=-90"], &out);
        out
    };
    let stored = make("stored.mp4", "scale=320:480,setsar=2/1,transpose=2");
    let tagged = tag(&stored, "shown.mp4");
    let squeezed = make("squeezed.mp4", "scale=320:480,setsar=2/1");
    let sideways = tag(&squeezed, "sideways.mp4");
    let probed = decode::probe(&tagged).unwrap();
    assert_eq!((probed.width, probed.height), (W, H));
    assert!(probed.square_pixels);
    assert_candidate(&lab.scan(&tagged), MISSING_POSTER);
    assert_no_candidate(&lab.scan(&stored));
    assert_no_candidate(&lab.scan(&sideways));
}

#[test]
fn a_video_with_audio_past_the_pictures_stays_complete() {
    let lab = Lab::new();
    let (shown, _) = scene(MISSING_ID, &frames(0..6));
    let pictures = lab.path("pictures.mkv");
    encode(lab.dir.path(), &sequence(8, 0..6, &shown), 10, H264, &pictures);
    let with_audio = lab.path("audio-tail.mkv");
    ffmpeg([
        "-i",
        pictures.to_str().unwrap(),
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=30",
        "-c:v",
        "copy",
        "-c:a",
        "aac",
        "-map",
        "0:v",
        "-map",
        "1:a",
        with_audio.to_str().unwrap(),
    ]);
    let probed = decode::probe(&with_audio).unwrap();
    assert!(probed.frames_exact, "the packets are the pictures; the audio is not a picture");
    assert!(probed.duration_sec < 2.0, "picture duration {}", probed.duration_sec);
    let mut req = lab.request(&with_audio);
    let report = run(&req);
    assert_eq!(probed.frames, report.counts.frames_decoded);
    assert_candidate(&report, MISSING_POSTER);
    req.out = lab.out();
    let estimate = estimate_for(&req).unwrap();
    assert_eq!(estimate.frames_analyzed, report.counts.frames_decoded);
    assert!(!estimate.suggest_computer);
}

#[test]
fn an_edit_list_scans_the_frames_a_player_shows() {
    let lab = Lab::new();
    // The marker is on the first six scene frames; the small one stays on all eight.
    let at = spot_in(MISSING_ID, SIDE, &frames(0..6), 16..360, 16..260);
    let mut picture = scene_at(MISSING_ID, at);
    place(&mut picture, WANTED_ID, SMALL, (520, 400));
    let mut small_only = blank(W, H);
    place(&mut small_only, WANTED_ID, SMALL, (520, 400));
    let mut pictures = sequence(8, 0..6, &picture);
    pictures[6] = small_only.clone();
    pictures[7] = small_only;
    let scene_video = h264(&lab, &pictures, 10, "scene.mp4");
    let blank_video = h264(&lab, &(0..4).map(plain).collect::<Vec<_>>(), 10, "blank.mp4");
    let joined = |name: &str, first: &Path, second: &Path| {
        let out = lab.path(name);
        ffmpeg([
            "-i",
            first.to_str().unwrap(),
            "-i",
            second.to_str().unwrap(),
            "-filter_complex",
            "[0:v][1:v]concat=n=2:v=1:a=0",
            "-an",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            out.to_str().unwrap(),
        ]);
        out
    };
    let trim = |src: &Path, start: &str, name: &str| {
        let out = lab.path(name);
        ffmpeg(["-ss", start, "-i", src.to_str().unwrap(), "-c", "copy", out.to_str().unwrap()]);
        out
    };
    let hidden_blank = joined("blank-then-scene.mp4", &blank_video, &scene_video);
    let hidden_scene = joined("scene-then-blank.mp4", &scene_video, &blank_video);
    let show_scene = trim(&hidden_blank, "0.4", "show-scene.mp4");
    let show_blank = trim(&hidden_scene, "0.8", "show-blank.mp4");

    let whole = decode::probe(&scene_video).unwrap();
    assert!(whole.frames_exact);
    assert_eq!(whole.frames, 8);
    let plain_scan = lab.scan(&scene_video);
    assert_eq!(plain_scan.counts.frames_decoded, 8);
    assert_candidate(&plain_scan, MISSING_POSTER);
    assert_eq!(plain_scan.candidates.len(), 1, "{}", describe(&plain_scan));
    // One row per face: the compared marker and the small one that is only seen.
    assert_eq!(plain_scan.tracks.len(), 2, "{}", describe(&plain_scan));
    let small = tracks_with(&plain_scan, TrackStatus::NotCompared);
    assert_eq!(small.len(), 1, "{}", describe(&plain_scan));
    assert!(small[0].reason.as_ref().is_some_and(|l| l.is(copy::WHY_SMALL)));

    let shown_probe = decode::probe(&show_scene).unwrap();
    let hidden_probe = decode::probe(&hidden_blank).unwrap();
    assert!(shown_probe.frames_exact);
    assert!(shown_probe.frames < hidden_probe.frames, "{shown_probe:?} {hidden_probe:?}");
    let kept = lab.scan(&show_scene);
    assert_eq!(kept.counts.frames_decoded, shown_probe.frames, "{}", describe(&kept));
    assert_candidate(&kept, MISSING_POSTER);

    let dropped = decode::probe(&show_blank).unwrap();
    let full = decode::probe(&hidden_scene).unwrap();
    assert!(dropped.frames < full.frames);
    let gone = lab.scan(&show_blank);
    assert_no_candidate(&gone);
    assert_eq!(gone.counts.frames_decoded, dropped.frames);
}

#[test]
fn an_edit_list_hold_stays_on_the_pictures_a_player_shows() {
    // The hold makes the average rate the header count over the duration, and the
    // edit list hides a packet. The finished file used to read as incomplete.
    let lab = Lab::new();
    let (shown, _) = scene(MISSING_ID, &frames(0..4));
    let steady = lab.path("steady.mp4");
    encode(lab.dir.path(), &sequence(8, 3..4, &shown), 25, H264_INTRA, &steady);
    let held = lab.path("held.mp4");
    remux(&steady, &["-c", "copy", "-bsf:v", &hold_before(4, 12800)], &held);
    let trimmed = lab.path("trimmed.mp4");
    ffmpeg(["-ss", "0.10", "-i", held.to_str().unwrap(), "-c", "copy", trimmed.to_str().unwrap()]);
    let report = lab.scan(&trimmed);
    assert_eq!(report.counts.frames_decoded, decode::probe(&trimmed).unwrap().frames);
    assert_passed_at(&report, 0);
}

#[test]
fn a_fractional_sample_aspect_is_scanned() {
    // 45:44 lands between two pixels. Asking for one more pixel than the decoder
    // writes used to refuse the file.
    let lab = Lab::new();
    let movie = lab.path("wide.mp4");
    let mut options = vec!["-vf", "setsar=45/44"];
    options.extend(H264);
    encode(lab.dir.path(), &(0..2).map(plain).collect::<Vec<_>>(), 10, &options, &movie);
    let report = lab.scan(&movie);
    assert_complete(&report);
    assert_eq!(report.counts.frames_decoded, 2);
}

#[test]
fn a_cropped_frame_is_scanned_as_shown() {
    // The header crop is smaller than the coded picture. Allocating the header
    // size used to refuse a frame a player shows.
    let lab = Lab::new();
    // FFmpeg skips a left crop that is not aligned in memory and keeps the right
    // one, so the marker is placed where it passes with or without the left crop.
    let shown = frames(0..4);
    let (x, y) = (16..W - SIDE - 32)
        .step_by(16)
        .flat_map(|x| (16..H - SIDE - 32).step_by(16).map(move |y| (x, y)))
        .find(|&(x, y)| shown.iter().all(|&f| passes(MISSING_ID, (x, y), SIDE, f) && passes(MISSING_ID, (x + 8, y), SIDE, f)))
        .expect("a spot that passes either way");
    let coded = scene_at(MISSING_ID, (x + 8, y + 8));
    let stored = h264(&lab, &sequence(4, 0..4, &coded), 10, "stored.mp4");
    let cropped = lab.path("cropped.mp4");
    remux(&stored, &["-an", "-c", "copy", "-bsf:v", "h264_metadata=crop_left=8:crop_right=8:crop_top=8:crop_bottom=8"], &cropped);
    let probed = decode::probe(&cropped).unwrap();
    assert_eq!(probed.height, H - 16);
    assert!(probed.width == W - 16 || probed.width == W - 8, "{probed:?}");
    assert_candidate(&lab.scan(&cropped), MISSING_POSTER);
}

#[test]
fn an_animated_png_without_an_average_rate_is_scanned() {
    // The muxer leaves the average at 0/0 and writes a timebase on r_frame_rate.
    let lab = Lab::new();
    let (shown, _) = scene(MISSING_ID, &frames(0..3));
    let stored = h264(&lab, &sequence(3, 0..3, &shown), 2, "stored.mp4");
    let apng = lab.path("clip.apng");
    remux(&stored, &["-plays", "1", "-f", "apng"], &apng);
    let report = lab.scan(&apng);
    assert_eq!(report.counts.frames_decoded, 3, "{}", describe(&report));
    assert_candidate(&report, MISSING_POSTER);
}

#[test]
fn an_animated_png_with_camera_orientation_is_scanned_as_shown() {
    // The still path turns a PNG by its tag. An animated PNG skips that path so
    // every frame is read, and the tag used to stay on the stored pixels.
    let lab = Lab::new();
    let (shown, _) = scene(MISSING_ID, &frames(0..3));
    let stored = image::imageops::rotate270(&shown);
    let movie = lab.path("clip.apng");
    encode(lab.dir.path(), &[stored.clone(), stored.clone(), stored], 2, &["-plays", "1", "-f", "apng"], &movie);
    let tagged = lab.write("tagged.apng", &png_with_orientation(&fs::read(&movie).unwrap(), 6));
    let turned = lab.scan(&tagged);
    assert_eq!(turned.counts.frames_decoded, 3, "{}", describe(&turned));
    assert_candidate(&turned, MISSING_POSTER);
    assert_no_candidate(&lab.scan(&movie));
}

#[test]
fn an_animated_webp_keeps_a_face_that_is_not_on_the_first_frame() {
    // FFmpeg's WebP decoder skips an animation. A player shows every frame.
    let lab = Lab::new();
    let (shown, _) = scene(MISSING_ID, &[1]);
    let movie = lab.path("clip.webp");
    encode(lab.dir.path(), &[blank(W, H), shown, blank(W, H)], 2, &["-c:v", "libwebp_anim", "-lossless", "1"], &movie);
    for report in [lab.scan(&movie), lab.scan_measured(&movie)] {
        assert_eq!(report.counts.frames_decoded, 3, "{}", describe(&report));
        assert_passed_at(&report, 1);
    }
}

// ---------------------------------------------------------------------------
// Motion photos: a still with a clip appended. The clip comes first, the cover
// is one more picture after it.

/// Eight frames at 10 fps with the marker on the first six.
fn scene_clip(lab: &Lab) -> Vec<u8> {
    let (shown, _) = scene(MISSING_ID, &frames(0..6));
    fs::read(h264(lab, &sequence(8, 0..6, &shown), 10, "scene.mp4")).unwrap()
}

/// A blank clip of `count` frames at `fps`.
fn blank_clip(lab: &Lab, count: u32, fps: u32) -> Vec<u8> {
    let frames: Vec<RgbImage> = (0..count).map(|i| {
        let mut f = blank(64, 64);
        f.put_pixel(i, 0, image::Rgb([0, 0, 0]));
        f
    }).collect();
    fs::read(h264(lab, &frames, fps, &format!("blank-{count}.mp4"))).unwrap()
}

fn blank_cover() -> Vec<u8> {
    jpeg_bytes(&blank(64, 64))
}

/// The cover with the marker passing on `index`, the picture number it gets after the clip.
fn marked_cover(index: u64) -> Vec<u8> {
    jpeg_bytes(&scene(MISSING_ID, &[index]).0)
}

#[test]
fn a_motion_photo_keeps_a_face_that_is_not_on_the_still() {
    let lab = Lab::new();
    let clip = scene_clip(&lab);
    let xmp = xmp_packet(&format!(
        "<rdf:Description xmlns:GCamera=\"http://ns.google.com/photos/1.0/camera/\" \
         GCamera:MotionPhoto=\"1\" GCamera:MicroVideo=\"1\" GCamera:MicroVideoOffset=\"{}\"/>",
        clip.len()
    ));
    let photo = lab.write("motion.jpg", &jpeg_with_xmp(&blank_cover(), xmp.as_bytes(), &clip));
    let probed = decode::probe(&photo).unwrap();
    assert!(probed.video && probed.motion_cover, "{probed:?}");
    let report = lab.scan(&photo);
    assert_eq!(report.counts.frames_decoded, 9, "{}", describe(&report));
    assert_candidate(&report, MISSING_POSTER);
}

#[test]
fn a_motion_photo_directory_keeps_a_face_that_is_not_on_the_still() {
    // The cover item's length is 0; the video item's length is the clip at the end.
    let lab = Lab::new();
    let clip = scene_clip(&lab);
    let xmp = motion_directory("1", "image/jpeg", clip.len(), "GCamera:MotionPhotoPresentationTimestampUs=\"0\"");
    let photo = lab.write("motion.jpg", &jpeg_with_xmp(&blank_cover(), xmp.as_bytes(), &clip));
    let report = lab.scan(&photo);
    assert_eq!(report.counts.frames_decoded, 9, "{}", describe(&report));
    assert_candidate(&report, MISSING_POSTER);
}

#[test]
fn a_samsung_motion_photo_keeps_a_face_that_is_not_on_the_still() {
    // The clip sits before an SEFT trailer, so it is not the last bytes of the file.
    let lab = Lab::new();
    let clip = scene_clip(&lab);
    let xmp = xmp_packet(
        "<rdf:Description xmlns:GCamera=\"http://ns.google.com/photos/1.0/camera/\" \
         GCamera:MotionPhoto=\"1\" GCamera:MotionPhotoVersion=\"1\"/>",
    );
    let photo = lab.write("motion.jpg", &jpeg_with_xmp(&blank_cover(), xmp.as_bytes(), &samsung_trailer(&clip)));
    let report = lab.scan(&photo);
    assert_eq!(report.counts.frames_decoded, 9, "{}", describe(&report));
    assert_candidate(&report, MISSING_POSTER);
}

#[test]
fn a_motion_photo_offset_wins_over_a_stale_micro_video_offset() {
    // Mid-era Pixel files store the clip length in MotionPhotoOffset. A leftover
    // MicroVideoOffset of 100 does not land on the movie.
    let lab = Lab::new();
    let clip = scene_clip(&lab);
    let xmp = xmp_packet(&format!(
        "<rdf:Description xmlns:GCamera=\"http://ns.google.com/photos/1.0/camera/\" \
         GCamera:MotionPhoto=\"1\" GCamera:MicroVideoOffset=\"100\" GCamera:MotionPhotoOffset=\"{}\"/>",
        clip.len()
    ));
    let photo = lab.write("motion.jpg", &jpeg_with_xmp(&blank_cover(), xmp.as_bytes(), &clip));
    let report = lab.scan(&photo);
    assert_eq!(report.counts.frames_decoded, 9, "{}", describe(&report));
    assert_candidate(&report, MISSING_POSTER);
}

#[test]
fn a_motion_photo_directory_wins_over_a_stale_offset() {
    let lab = Lab::new();
    let clip = scene_clip(&lab);
    let xmp = motion_directory("1", "image/jpeg", clip.len(), "GCamera:MicroVideo=\"1\" GCamera:MicroVideoOffset=\"100\"");
    let photo = lab.write("motion.jpg", &jpeg_with_xmp(&blank_cover(), xmp.as_bytes(), &clip));
    let report = lab.scan(&photo);
    assert_eq!(report.counts.frames_decoded, 9, "{}", describe(&report));
    assert_candidate(&report, MISSING_POSTER);
}

#[test]
fn an_avif_motion_photo_keeps_a_face_that_is_not_on_the_still() {
    // The still is an AVIF and the clip an `mpvd` box after it.
    let lab = Lab::new();
    let clip = scene_clip(&lab);
    let still = lab.path("blank.avif");
    ffmpeg(["-f", "lavfi", "-i", "color=c=black:s=64x64", "-frames:v", "1", "-c:v", "libaom-av1", "-still-picture", "1", still.to_str().unwrap()]);
    let photo = lab.write("motion.avif", &avif_motion(&fs::read(&still).unwrap(), &clip));
    let report = lab.scan(&photo);
    assert_eq!(report.counts.frames_decoded, 9, "{}", describe(&report));
    assert_candidate(&report, MISSING_POSTER);
}

#[test]
fn a_stripped_motion_photo_is_the_still_it_shows() {
    // Editors keep the directory after the video is gone. The length no longer
    // lands on a movie, so the file is the still, and the still has the face.
    let lab = Lab::new();
    let xmp = motion_directory("1", "image/jpeg", 20_000, "");
    let photo = lab.write("stripped.jpg", &jpeg_with_xmp(&marked_cover(0), xmp.as_bytes(), &[]));
    let report = lab.scan(&photo);
    assert_eq!(report.counts.frames_decoded, 1, "{}", describe(&report));
    assert_candidate(&report, MISSING_POSTER);
}

#[test]
fn a_motion_photo_marked_off_is_the_still_it_shows() {
    // Any MotionPhoto value other than 1 is a still, even with a video appended.
    for flag in ["0", "2"] {
        let lab = Lab::new();
        let clip = blank_clip(&lab, 2, 10);
        let xmp = motion_directory(flag, "image/jpeg", clip.len(), "");
        let photo = lab.write("off.jpg", &jpeg_with_xmp(&marked_cover(0), xmp.as_bytes(), &clip));
        let report = lab.scan(&photo);
        assert_eq!(report.counts.frames_decoded, 1, "flag {flag}: {}", describe(&report));
        assert_candidate(&report, MISSING_POSTER);
    }
}

#[test]
fn a_motion_photo_cover_is_scanned_after_its_clip() {
    // The cover is the scene and the clip is blank. A scan of the clip alone
    // finishes with nothing found. The cover is picture 3, after two clip frames.
    let lab = Lab::new();
    let clip = blank_clip(&lab, 2, 10);
    let xmp = motion_directory("1", "image/jpeg", clip.len(), "");
    let photo = lab.write("cover.jpg", &jpeg_with_xmp(&marked_cover(2), xmp.as_bytes(), &clip));
    let report = lab.scan(&photo);
    assert_eq!(report.counts.frames_decoded, 3, "{}", describe(&report));
    assert_passed_at(&report, 2);
}

#[test]
fn a_measured_motion_photo_always_reads_its_cover() {
    // Two clip frames at 30 fps, then the cover. A 5 Hz tick lands on frame 0
    // only; the cover is read anyway, and the estimate counts it.
    let lab = Lab::new();
    let clip = blank_clip(&lab, 2, 30);
    let xmp = motion_directory("1", "image/jpeg", clip.len(), "GCamera:MotionPhotoPresentationTimestampUs=\"0\"");
    let photo = lab.write("cover.jpg", &jpeg_with_xmp(&marked_cover(2), xmp.as_bytes(), &clip));
    let mut req = lab.request(&photo);
    req.coverage = Coverage::Measured;
    let report = run(&req);
    assert_eq!((report.counts.frames_decoded, report.counts.frames_analyzed), (3, 2), "{}", describe(&report));
    assert_passed_at(&report, 2);
    assert_eq!(estimate_for(&req).unwrap().frames_analyzed, report.counts.frames_analyzed);
}
