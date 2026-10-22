// SPDX-License-Identifier: Apache-2.0

//! What a scan decides and says: one row per face track, the 64 px and 112 px
//! gates, the locked demo cutoff, the video multi-frame rule, poster classes,
//! the way out to a poster, stops, measured coverage, and the result folder.

mod common;

use common::*;
use image::RgbImage;
use openworld_core::copy::{self, Line};
use openworld_core::engine::{Event, StopFlag, MIN_QUALITY_FRAMES};
use openworld_core::estimate::Coverage;
use openworld_core::geom::frames_on_screen;
use openworld_core::hardware::{execution_from_provider, Execution};
use openworld_core::outdir;
use openworld_core::report::{ScanReport, Status, TrackStatus};
use openworld_core::scan::{delete_output, leave_prompt, media_warnings, scan_path};
use openworld_core::timeutil::parse_rfc3339;
use openworld_core::StopReason;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

fn demo_scene() -> RgbImage {
    openworld_core::fixture::demo_scene().image
}

#[test]
fn the_demo_scene_compares_one_marker_and_sees_another() {
    let lab = Lab::new();
    let input = lab.png("scene.png", &demo_scene());
    let req = lab.request(&input);
    let mut events = Vec::new();
    let report = scan_path(&req, None, &mut |event| events.push(event));

    let card = assert_candidate(&report, MISSING_POSTER);
    assert_eq!(report.schema, "openworld.result.v2");
    assert_eq!(report.candidates.len(), 1, "{}", describe(&report));
    assert_eq!((card.poster.class.as_str(), card.poster.class_label.as_str()), ("missing", "Missing"));
    assert_eq!(card.poster.fbi_url.as_deref(), Some("https://www.fbi.gov/wanted"));
    assert!(card.poster.image.as_ref().is_some_and(|p| PathBuf::from(p).is_file()), "{:?}", card.poster.image);
    assert!(card.headline.is(copy::POSSIBLE_CANDIDATE));
    assert!([copy::BAND_NEAR, copy::BAND_ABOVE, copy::BAND_WELL].iter().any(|b| card.band.is(*b)), "{:?}", card.band);
    assert_eq!(card.frame_index, 0);
    assert_eq!(card.frame_label.text, "Frame 1.");
    assert!(card.leaving.is(copy::LEAVING));
    let crop = card.crop.as_ref().expect("a crop");
    assert!(req.out.join(crop).is_file(), "{crop}");
    assert!(req.out.join(card.frame.as_ref().expect("the frame")).is_file());

    // One row per face: the compared marker, and the 64 px one that is only seen.
    assert_eq!(report.tracks.len(), 2, "{}", describe(&report));
    let seen = &tracks_with(&report, TrackStatus::NotCompared)[0];
    assert_eq!(seen.face_px, 64);
    assert_eq!(seen.quality_frames, 0);
    assert!(seen.label.is(copy::NOT_COMPARED));
    assert!(seen.reason.as_ref().is_some_and(|l| l.is(copy::WHY_SMALL)));
    assert!(seen.crop.as_ref().is_some_and(|c| req.out.join(c).is_file()), "a seen face still shows its crop");
    assert!(report.tracks.iter().all(|t| t.frames.text == "Frame 1."));
    assert!(report.tracks.iter().all(|t| t.scores.is_empty()), "no raw score leaves a shipped build");

    assert_eq!(report.counts.faces_seen, 2);
    assert_eq!(report.counts.faces_compared, 1);
    assert_eq!(report.counts.faces_not_compared, 1);
    assert_eq!(report.counts.candidates, 1);
    assert_eq!(report.counts.frames_declared, None, "a still declares no frame count");
    assert_eq!(report.faces_line.as_ref().map(|l| l.code.as_str()), Some("faces_some"));
    assert_eq!(report.frames_line.as_ref().map(|l| l.code.as_str()), Some("frames_one"));
    assert!(report.coverage_line.as_ref().is_some_and(|l| l.is(copy::COVERAGE_COMPLETE)));
    assert!(report.coverage_banner.is_none());
    assert_eq!(report.classes.as_ref().map(|l| l.code.as_str()), Some("classes_both"));
    assert_eq!(report.detection_long_side, W);
    assert_eq!(report.detection_line.as_ref().map(|l| l.code.as_str()), Some("detect_full"));
    assert_eq!((report.bundle.id.as_str(), report.bundle.name.as_str()), ("fast", "Fast"));
    assert_eq!(report.perception, "fiducial-v1");
    assert!(report.perception_note.as_ref().is_some_and(|l| l.is(copy::FIXTURE_MARKERS)));
    assert_eq!(report.provider, "fixture");
    assert!(report.posters.demo);
    assert_eq!((report.posters.missing, report.posters.wanted), (1, 1));
    assert!(report.chance.is_none(), "a demo pack has no chance line");
    let disclosure = codes(&report.disclosure);
    let all: Vec<&str> = copy::DISCLOSURE.iter().map(|c| c.0).collect();
    assert_eq!(disclosure, all, "a finished scan keeps every disclosure line, the clearance sentence too");

    // Events: a first look at each face, then its row once it is decided.
    let faces = events.iter().filter(|e| matches!(e, Event::Face { .. })).count();
    let rows: Vec<_> = events.iter().filter_map(|e| if let Event::Track { track } = e { Some(track) } else { None }).collect();
    assert_eq!(faces, 2);
    assert_eq!(rows.len(), 2);
    assert!(events.iter().all(|e| !matches!(e, Event::Face { frame_label, .. } if frame_label.text != "Frame 1.")));

    // The result file is the report.
    let saved: Value = serde_json::from_slice(&fs::read(req.out.join(outdir::RESULT_FILE)).unwrap()).unwrap();
    assert_eq!(saved["status"], "complete");
    assert_eq!(saved["headline"]["code"], "possible_candidate");
    assert_eq!(saved["tracks"].as_array().unwrap().len(), 2);
    assert!(outdir::is_result(&req.out));

    // The shell entry gives the same answer.
    let call = json!({"v": 1, "op": "scan", "input": input, "bundles": catalog(), "posters": lab.posters, "out": lab.out()});
    let answer = openworld_core::invoke::call(&call.to_string(), None, &mut |_| {});
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["headline"]["code"], "possible_candidate");
}

#[test]
fn a_64_px_marker_is_seen_and_not_compared_and_a_128_px_marker_is_compared() {
    let lab = Lab::new();
    for module in [4, SMALL] {
        let mut image = blank(W, H);
        place(&mut image, MISSING_ID, module, (40, 40));
        let report = lab.scan(lab.png(&format!("small-{module}.png"), &image));
        assert_no_candidate(&report);
        assert_eq!(report.tracks.len(), 1, "{}", describe(&report));
        let track = &report.tracks[0];
        assert_eq!(track.status, TrackStatus::NotCompared);
        assert_eq!(track.face_px, 8 * module);
        assert!(track.reason.as_ref().is_some_and(|l| l.is(copy::WHY_SMALL)), "{track:?}");
        assert_eq!((report.counts.faces_seen, report.counts.faces_compared), (1, 0));
        assert_eq!(report.faces_line.as_ref().map(|l| l.code.as_str()), Some("faces_some"));
    }
    let (big, _) = scene(MISSING_ID, &[0]);
    let report = lab.scan(lab.png("big.png", &big));
    let card = assert_candidate(&report, MISSING_POSTER);
    assert_eq!(report.tracks.len(), 1);
    assert_eq!(report.tracks[0].face_px, SIDE);
    assert_eq!(report.tracks[0].quality_frames, 1);
    assert_eq!(card.track_id, report.tracks[0].track_id);
    assert_eq!(report.faces_line.as_ref().map(|l| l.code.as_str()), Some("faces_all"));

    // Markers are read at the original resolution: a large face in a large photo is compared.
    // An uncompressed TIFF keeps this quick in a debug build.
    for (w, h, module) in [(4032, 3024, 32), (3840, 2160, 48)] {
        let side = 8 * module;
        let at = spot_in(MISSING_ID, side, &[0], 600..1400, 300..h - side);
        let mut photo = blank(w, h);
        place(&mut photo, MISSING_ID, module, at);
        let report = lab.scan(lab.write(&format!("large-{w}.tif"), &tiff(&[TiffPage::rgb(&photo, 1)])));
        assert_candidate(&report, MISSING_POSTER);
        assert_eq!(report.tracks[0].face_px, side);
        assert_eq!(report.detection_long_side, w, "{}", describe(&report));
        assert_eq!(report.detection_line.as_ref().map(|l| l.code.as_str()), Some("detect_full"));
    }
}

#[test]
fn a_marker_for_no_poster_or_under_the_cutoff_is_below_it() {
    let lab = Lab::new();
    let stranger = scene_at(STRANGER_ID, (32, 32));
    let under = scene_at(MISSING_ID, miss_spot(MISSING_ID, 0));
    for (name, image) in [("stranger.png", stranger), ("under.png", under)] {
        let report = lab.scan(lab.png(name, &image));
        assert_no_candidate(&report);
        assert_eq!(report.tracks.len(), 1, "{name}: {}", describe(&report));
        let track = &report.tracks[0];
        assert_eq!(track.status, TrackStatus::BelowCutoff, "{name}");
        assert!(track.label.is(copy::BELOW_CUTOFF));
        assert!(track.reason.is_none());
        assert_eq!(report.counts.faces_compared, 1);
        assert_eq!(report.faces_line.as_ref().map(|l| l.code.as_str()), Some("faces_all"));
    }
    let report = lab.scan(lab.png("blank.png", &blank(400, 320)));
    assert_no_candidate(&report);
    assert!(report.tracks.is_empty());
    assert_eq!(report.faces_line.as_ref().map(|l| l.code.as_str()), Some("faces_none"));
}

fn decisions(report: &ScanReport) -> Value {
    let code = |line: &Option<Line>| line.as_ref().map(|l| l.code.clone());
    json!({
        "status": report.status,
        "headline": report.headline.code,
        "reason": code(&report.reason),
        "refusal": report.refusal,
        "tracks": report.tracks.iter().map(|t| json!({
            "frames": [t.first_frame, t.last_frame],
            "face_px": t.face_px,
            "quality_frames": t.quality_frames,
            "status": t.status,
            "label": t.label.code,
            "reason": code(&t.reason),
        })).collect::<Vec<_>>(),
        "candidates": report.candidates.iter().map(|c| json!({
            "poster": c.poster.id,
            "class": c.poster.class,
            "band": c.band.code,
            "frame": c.frame_index,
            "leaving": c.leaving.code,
            "leave_notes": codes(&c.leave_notes),
        })).collect::<Vec<_>>(),
        "counts": [report.counts.faces_seen, report.counts.faces_compared, report.counts.faces_not_compared, report.counts.candidates],
        "disclosure_has_no_clearance": report.disclosure.iter().any(|l| l.is(copy::NO_CLEARANCE)),
    })
}

#[test]
fn decision_snapshot_matches_the_locked_cutoff() {
    // Fixed placements, so each decision is the same every run. A change to a gate,
    // the cutoff, the bands, or the wording codes shows up here as a diff.
    let lab = Lab::new();
    let scan_image = |name: &str, image: &RgbImage| lab.scan(lab.png(name, image));
    let mut tiny = blank(400, 320);
    place(&mut tiny, MISSING_ID, 4, (16, 16));
    let fixed = |id: u16, at: (u32, u32)| {
        let mut image = blank(400, 320);
        place(&mut image, id, BIG, at);
        image
    };
    let brief: Vec<RgbImage> = sequence(30, 12..13, &scene_at(MISSING_ID, spot(MISSING_ID, &[12])));
    let got = json!({
        "demo_scene": decisions(&scan_image("scene.png", &demo_scene())),
        "both_classes": decisions(&scan_image("both.png", &both_classes())),
        "below_cutoff": decisions(&scan_image("below.png", &fixed(MISSING_ID, miss_spot(MISSING_ID, 0)))),
        "stranger": decisions(&scan_image("stranger.png", &fixed(STRANGER_ID, (16, 16)))),
        "blank": decisions(&scan_image("blank.png", &blank(400, 320))),
        "seen_32_px": decisions(&scan_image("tiny.png", &tiny)),
        "one_video_frame": decisions(&feed(&brief, Feed::default())),
        "measured_one_video_frame": decisions(&feed(&brief, Feed { coverage: Coverage::Measured, ..Feed::default() })),
    });
    let text = serde_json::to_string_pretty(&got).unwrap() + "\n";
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/decisions.json");
    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        fs::write(&path, &text).unwrap();
    }
    let expected = fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(text, expected, "decision snapshot drifted; set UPDATE_SNAPSHOTS=1 to rewrite {}", path.display());
}

#[test]
fn the_fixture_cutoff_applies_whatever_bundle_is_picked() {
    // Demo markers are scored by the demo scorer and its cutoff, never a bundle's.
    let lab = Lab::new();
    let catalog_dir = lab.path("catalog");
    dev_bundle(&catalog_dir, "strict", 0.99, &[]);
    dev_bundle(&catalog_dir, "loose", 0.01, &[]);
    let input = lab.png("scene.png", &demo_scene());
    let picks = [(catalog(), "fast"), (catalog(), "accurate"), (catalog_dir.clone(), "strict"), (catalog_dir, "loose")];
    let mut seen = Vec::new();
    for (bundles, id) in picks {
        let mut req = lab.request(&input);
        req.bundles = bundles;
        req.bundle = id.into();
        let report = run(&req);
        assert_candidate(&report, MISSING_POSTER);
        assert_eq!(report.bundle.id, id);
        assert!(report.perception_note.as_ref().is_some_and(|l| l.is(copy::FIXTURE_MARKERS)));
        seen.push(decisions(&report));
    }
    assert!(seen.windows(2).all(|pair| pair[0] == pair[1]), "{seen:#?}");
}

#[test]
fn a_class_that_is_off_is_not_compared_and_the_cutoff_is_the_same() {
    let lab = Lab::new();
    let input = lab.png("both.png", &both_classes());
    let scan = |missing: bool, wanted: bool| {
        let mut req = lab.request(&input);
        req.missing = missing;
        req.wanted = wanted;
        run(&req)
    };
    let both = scan(true, true);
    assert_candidate(&both, MISSING_POSTER);
    let wanted_card = assert_candidate(&both, WANTED_POSTER);
    assert_eq!((wanted_card.poster.class.as_str(), wanted_card.poster.class_label.as_str()), ("wanted", "Wanted"));
    assert_eq!(both.candidates.len(), 2);

    for (missing, wanted, kept, class_code) in [(true, false, MISSING_POSTER, "classes_missing"), (false, true, WANTED_POSTER, "classes_wanted")] {
        let report = scan(missing, wanted);
        assert_candidate(&report, kept);
        assert_eq!(report.candidates.len(), 1, "{}", describe(&report));
        assert_eq!(report.classes.as_ref().map(|l| l.code.as_str()), Some(class_code));
        // The other marker is still a face: compared to the posters that are on, and under the cutoff.
        assert_eq!(tracks_with(&report, TrackStatus::BelowCutoff).len(), 1, "{}", describe(&report));
        // The same band as with both classes on: the cutoff did not move.
        let card = &report.candidates[0];
        let same = both.candidates.iter().find(|c| c.poster.id == card.poster.id).unwrap();
        assert_eq!(card.band, same.band);
    }

    // Nothing to compare with is refused, never a scan that compared nothing.
    let neither = scan(false, false);
    assert_refused(&neither, copy::CHOOSE_CLASS);
    // The shells cannot start such a scan.
    let call = json!({"v": 1, "op": "scan", "input": input, "bundles": catalog(), "posters": lab.posters, "out": lab.out(), "missing": false, "wanted": false});
    let answer = openworld_core::invoke::call(&call.to_string(), None, &mut |_| {});
    assert_eq!(answer["refusal"], "choose_class");
}

#[test]
fn the_way_out_names_the_poster_class_and_carries_only_the_file_warnings() {
    let leave = |report: &ScanReport, poster: &str| codes(&assert_candidate(report, poster).leave_notes).iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let expect = |class: &str, warnings: &[&str]| {
        let mut notes = vec!["not_sending".to_string(), "not_affiliated".to_string(), class.to_string()];
        notes.extend(warnings.iter().map(|w| w.to_string()));
        notes.extend(["unauthenticated", "lookalike", "do_not_share", "network_note"].map(String::from));
        notes
    };

    // A recent file.
    let lab = Lab::new();
    let input = lab.png("both.png", &both_classes());
    let recent = lab.scan(&input);
    assert_eq!(leave(&recent, MISSING_POSTER), expect("leave_missing", &[]));
    assert_eq!(leave(&recent, WANTED_POSTER), expect("leave_wanted", &[]));

    // An old file, from posters fetched two days ago. The stale-poster warning is
    // on the result; only the file's own warnings travel to the poster page.
    let stale = Lab::dated(SystemTime::now() - days(2));
    let input = stale.png("both.png", &both_classes());
    let mut req = stale.request(&input);
    let forty_days_ago = SystemTime::now() - days(40);
    req.file_mtime_unix = Some(forty_days_ago.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as i64);
    let old = run(&req);
    assert_eq!(codes(&old.warnings), ["old_file", "posters_stale"]);
    assert_eq!(leave(&old, MISSING_POSTER), expect("leave_missing", &["old_file"]));
    assert_eq!(leave(&old, WANTED_POSTER), expect("leave_wanted", &["old_file"]));

    // The same file pushed by a phone whose container clock disagrees with the file time.
    let mut req = stale.request(&input);
    req.file_mtime_unix = Some(forty_days_ago.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as i64);
    req.media = Some(openworld_core::MediaFacts { width: W, height: H, frames: 1, container_unix: Some(1_000_000_000), ..Default::default() });
    let mut job = openworld_core::scan::open_pushed_job(&req, SystemTime::now()).unwrap();
    job.engine().push(0, &both_classes(), &mut |_| {});
    let declared = job.declared_frames;
    let pushed = job.finish(openworld_core::DecodeEnd { clean: true, declared_frames: declared, reason: None }, &mut |_| {});
    assert_eq!(leave(&pushed, WANTED_POSTER), expect("leave_wanted", &["old_file", "timestamps_disagree"]));

    assert!(copy::LEAVE_WANTED.1.contains("Do not approach") && copy::LEAVE_WANTED.1.contains("1-800-CALL-FBI (1-800-225-5324)"));
    assert!(copy::LEAVE_WANTED.1.contains("call 911"));
    assert!(copy::LEAVE_MISSING.1.contains("Do not approach, follow, or try to hold anyone, especially a child."));
    assert!(copy::LEAVE_MISSING.1.contains("1-800-THE-LOST (1-800-843-5678)"));
    assert!(copy::LEAVE_MISSING.1.contains("not a criminal accusation"));
    assert!(!copy::LEAVE_MISSING.1.contains("CALL-FBI") && !copy::LEAVE_MISSING.1.contains("tips.fbi.gov"));
    // A tap opens that poster's FBI page, and only an FBI page.
    let url = recent.candidates[0].poster.fbi_url.clone().unwrap();
    let prompt = leave_prompt(&url).unwrap();
    assert!(prompt.message.is(copy::LEAVING));
    assert_eq!(prompt.url, url);
}

#[test]
fn old_files_warn_and_the_leave_prompt_is_fbi_only() {
    let now = SystemTime::UNIX_EPOCH + days(40);
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
    assert_eq!(codes(&media_warnings(Some(old), Some(now), now)), ["old_file", "timestamps_disagree"]);
    assert_eq!(codes(&media_warnings(Some(now - days(10)), Some(now - days(13)), now)), ["timestamps_disagree"]);
    assert!(media_warnings(Some(now), None, now).is_empty());
    assert!(media_warnings(Some(now - days(29)), Some(now - days(29)), now).is_empty());
    assert!(media_warnings(None, Some(old), now).is_empty(), "no file time, nothing to compare");
    assert!(leave_prompt("https://www.fbi.gov/wanted/topten").is_ok());
    for bad in ["https://example.com/", "http://www.fbi.gov/wanted", "https://www.fbi.gov.evil.example/", "https://www.fbi.gov@evil.example/"] {
        assert!(leave_prompt(bad).unwrap_err().is(copy::ONLY_FBI), "{bad}");
    }
    assert_eq!(execution_from_provider("CPUExecutionProvider"), Execution::Cpu);
}

// ---------------------------------------------------------------------------
// The video rule: a matching face is a candidate on three gated frames or across one second.

/// A 30 fps clip with the marker on frames `on`, placed where it passes on each of them.
fn clip_with(on: &[u32]) -> Vec<RgbImage> {
    let marked: Vec<u64> = on.iter().map(|&f| u64::from(f)).collect();
    let picture = scene_at(MISSING_ID, spot(MISSING_ID, &marked));
    let count = on.iter().max().unwrap() + 5;
    (0..count).map(|i| if on.contains(&i) { picture.clone() } else { plain(i) }).collect()
}

#[test]
fn a_video_face_needs_three_gated_frames_or_one_second() {
    assert_eq!(MIN_QUALITY_FRAMES, 3);
    for (on, expected) in [
        (vec![12], TrackStatus::TooBrief),
        (vec![12, 13], TrackStatus::TooBrief),
        (vec![12, 13, 14], TrackStatus::Candidate),
        // Two frames a second apart, one track: across one second.
        (vec![10, 40], TrackStatus::Candidate),
        (vec![0, 30], TrackStatus::Candidate),
    ] {
        let report = feed(&clip_with(&on), Feed::default());
        assert_complete(&report);
        assert_eq!(report.tracks.len(), 1, "{on:?} {}", describe(&report));
        let track = &report.tracks[0];
        assert_eq!(track.status, expected, "{on:?} {}", describe(&report));
        assert_eq!(track.quality_frames as usize, on.len());
        if expected == TrackStatus::TooBrief {
            // Brief is not a clearance, and it is not a card.
            assert!(track.label.is(copy::TOO_BRIEF));
            assert!(report.headline.is(copy::NO_CLEARANCE));
            assert!(report.candidates.is_empty());
            assert_eq!(report.counts.faces_compared, 1);
        } else {
            assert!(report.headline.is(copy::POSSIBLE_CANDIDATE));
            assert_eq!(report.candidates.len(), 1);
            assert!(on.contains(&(report.candidates[0].frame_index as u32)));
        }
    }
    // A still decides on its one frame.
    let still = feed(&[scene_at(MISSING_ID, spot(MISSING_ID, &[0]))], Feed { video: false, fps: 0.0, ..Feed::default() });
    assert_eq!(still.tracks[0].status, TrackStatus::Candidate, "{}", describe(&still));
}

#[test]

fn a_face_exactly_one_second_apart_is_across_one_second() {
    // 42/30 - 12/30 is not 1.0 in f64, while 40/30 - 10/30 is. Both pairs are 30 frames apart.
    let report = feed(&clip_with(&[12, 42]), Feed::default());
    assert_eq!(report.tracks.len(), 1, "{}", describe(&report));
    assert_eq!(report.tracks[0].status, TrackStatus::Candidate, "{}", describe(&report));
}

#[test]
fn measured_coverage_can_miss_a_brief_face_that_complete_reads() {
    // Five frames a second at 30 fps reads frames 0, 6, 12, 18, ... A face on frames 13 and 14 is between ticks.
    let frames = clip_with(&[13, 14]);
    let complete = feed(&frames, Feed::default());
    assert_eq!(complete.tracks.len(), 1, "{}", describe(&complete));
    assert_eq!(complete.tracks[0].status, TrackStatus::TooBrief);
    let measured = feed(&frames, Feed { coverage: Coverage::Measured, ..Feed::default() });
    assert_complete(&measured);
    assert!(measured.tracks.is_empty(), "{}", describe(&measured));
    assert!(measured.coverage_banner.as_ref().is_some_and(|l| l.is(copy::BRIEF_FACE)));
    assert!(measured.coverage_line.as_ref().is_some_and(|l| l.is(copy::COVERAGE_MEASURED)));
    assert!(measured.counts.frames_analyzed < measured.counts.frames_decoded);
    assert_eq!(measured.counts.frames_decoded, frames.len() as u64);
    assert_eq!(measured.frames_line.as_ref().map(|l| l.code.as_str()), Some("frames_some"));
    assert_eq!(
        measured.frames_line.as_ref().unwrap().text,
        format!("{} of {} frames analyzed.", measured.counts.frames_analyzed, measured.counts.frames_decoded)
    );

    // A face on screen from frame 10 to 40 is read at the ticks on 12, 18, 24, 30, and 36.
    let picture = scene_at(MISSING_ID, spot(MISSING_ID, &[12, 18, 24, 30, 36]));
    let long = sequence(50, 10..41, &picture);
    let measured = feed(&long, Feed { coverage: Coverage::Measured, ..Feed::default() });
    assert_eq!(measured.tracks.len(), 1, "{}", describe(&measured));
    let track = &measured.tracks[0];
    assert_eq!(track.status, TrackStatus::Candidate, "{}", describe(&measured));
    assert_eq!((track.first_frame, track.last_frame, track.quality_frames), (12, 36, 5));
    assert_eq!(measured.counts.frames_analyzed, 9);
}

#[test]
fn a_held_frame_is_read_at_the_tick_it_is_on_screen() {
    // A phone's measured scan: twenty pictures, the tenth held for two seconds.
    // The even rate steps past it; the pictures on screen at each tick include it.
    let delays: Vec<f64> = (0..20).map(|i| if i == 9 { 2.0 } else { 0.04 }).collect();
    let picture = scene_at(MISSING_ID, spot(MISSING_ID, &[9]));
    let frames = sequence(20, 9..10, &picture);
    let fps = delays.len() as f64 / delays.iter().sum::<f64>();
    let even = feed(&frames, Feed { coverage: Coverage::Measured, fps, ..Feed::default() });
    assert!(even.tracks.is_empty(), "the even rate skips the held picture: {}", describe(&even));
    let on_screen = frames_on_screen(&delays);
    assert!(on_screen.contains(&9));
    let timed = feed(&frames, Feed { coverage: Coverage::Measured, fps, on_screen: Some(on_screen.clone()), ..Feed::default() });
    assert_passed_at(&timed, 9);
    assert_eq!(timed.counts.frames_analyzed, on_screen.len() as u64);
}

// ---------------------------------------------------------------------------
// Stops.

#[test]
fn a_scan_stopped_for_heat_mid_video_is_incomplete() {
    let lab = Lab::new();
    let picture = scene_at(MISSING_ID, spot(MISSING_ID, &frames(24..27)));
    let video = lab.path("long.mkv");
    encode(lab.dir.path(), &sequence(30, 24..27, &picture), 30, FFV1, &video);
    let req = lab.request(&video);
    let flag = StopFlag::default();
    let stopper = flag.clone();
    let report = scan_path(&req, Some(&flag), &mut |event| {
        if matches!(event, Event::Progress { .. }) {
            stopper.stop(StopReason::Heat);
        }
    });
    assert_incomplete(&report, copy::STOPPED_HEAT);
    assert!(report.counts.frames_decoded < 30, "{}", describe(&report));
    assert!(report.candidates.is_empty(), "the marker after the stop was never read");
    assert_eq!(report.counts.frames_declared, Some(30));
    let saved: Value = serde_json::from_slice(&fs::read(req.out.join(outdir::RESULT_FILE)).unwrap()).unwrap();
    assert_eq!(saved["status"], "incomplete");
    assert_eq!(saved["reason"]["code"], "stopped_heat");
    assert!(saved["expires_at"].as_str().is_some_and(|s| !s.is_empty()));
}

#[test]
fn every_stop_reason_is_incomplete_and_says_why() {
    let frames = sequence(10, 0..0, &plain(0));
    for (reason, code) in [
        (StopReason::Cancel, copy::CANCELLED),
        (StopReason::Heat, copy::STOPPED_HEAT),
        (StopReason::Battery, copy::STOPPED_BATTERY),
        (StopReason::Background, copy::STOPPED_BACKGROUND),
    ] {
        let report = feed(&frames, Feed { stop_before: Some((4, reason)), ..Feed::default() });
        assert_incomplete(&report, code);
        assert_eq!(report.counts.frames_decoded, 4);
        assert_eq!(report.frames_line.as_ref().map(|l| l.code.as_str()), Some("frames_all"));
    }
    // A still stopped before its one frame.
    let lab = Lab::new();
    let input = lab.png("scene.png", &demo_scene());
    let flag = StopFlag::default();
    flag.stop(StopReason::Cancel);
    let stopped = scan_path(&lab.request(&input), Some(&flag), &mut |_| {});
    assert_incomplete(&stopped, copy::CANCELLED);
    assert!(stopped.candidates.is_empty());
}

#[test]
fn a_finished_scan_whose_result_cannot_be_written_is_incomplete() {
    let lab = Lab::new();
    let input = lab.png("scene.png", &demo_scene());
    let req = lab.request(&input);
    let out = req.out.clone();
    let report = scan_path(&req, None, &mut |_| {
        let _ = fs::create_dir(out.join(outdir::RESULT_FILE));
    });
    assert_incomplete(&report, copy::RESULT_UNWRITTEN);
    assert!(!out.join(outdir::RESULT_FILE).is_file());
}

// ---------------------------------------------------------------------------
// The result folder.

#[test]
fn the_result_folder_is_marked_expires_and_is_replaced_only_when_it_is_ours() {
    let lab = Lab::new();
    let input = lab.png("scene.png", &demo_scene());
    let req = lab.request(&input);
    let report = run(&req);
    assert_complete(&report);
    assert!(req.out.join(outdir::MARKER).is_file());
    let saved: Value = serde_json::from_slice(&fs::read(req.out.join(outdir::RESULT_FILE)).unwrap()).unwrap();
    let created = parse_rfc3339(saved["created_at"].as_str().unwrap()).unwrap();
    let expires = parse_rfc3339(saved["expires_at"].as_str().unwrap()).unwrap();
    let pack_expires = parse_rfc3339(&report.posters.expires_at).unwrap();
    assert!(expires > created);
    assert!(expires <= created + days(outdir::RESULT_DAYS));
    assert!(expires <= pack_expires, "a result never outlives its posters");
    assert_eq!(saved["expires_at"], report.expires_at.as_str());

    // A second scan into our own result folder replaces it.
    fs::write(req.out.join("leftover.jpg"), b"x").unwrap();
    assert_complete(&run(&req));
    assert!(!req.out.join("leftover.jpg").exists());

    // Sweep removes results whose time is up, and nothing else.
    let root = lab.path("results");
    fs::create_dir_all(&root).unwrap();
    let mut kept = lab.request(&input);
    kept.out = root.join("scan");
    assert_complete(&run(&kept));
    let foreign = root.join("photos");
    fs::create_dir_all(&foreign).unwrap();
    fs::write(foreign.join(outdir::RESULT_FILE), r#"{"expires_at":"2000-01-01T00:00:00Z"}"#).unwrap();
    assert_eq!(outdir::sweep(&root, SystemTime::now()), 0);
    assert!(kept.out.is_dir());
    assert_eq!(outdir::sweep(&root, SystemTime::now() + days(8)), 1);
    assert!(!kept.out.exists());
    assert!(foreign.join(outdir::RESULT_FILE).is_file());
    let answer = openworld_core::invoke::call(&json!({"v": 1, "op": "sweep", "root": root}).to_string(), None, &mut |_| {});
    assert_eq!(answer["removed"], 0);

    // Delete removes a result and refuses anything else.
    assert!(delete_output(&req.out).unwrap().is(copy::DELETED));
    assert!(!req.out.exists());
    assert!(delete_output(&foreign).unwrap_err().is(copy::R_DELETE_FOREIGN));
    assert!(delete_output(lab.dir.path()).unwrap_err().is(copy::R_DELETE_FOREIGN));
    assert!(foreign.is_dir() && input.is_file());
}

#[test]
fn a_finished_scan_and_a_refusal_have_different_statuses() {
    let lab = Lab::new();
    let done = lab.scan(lab.png("blank.png", &blank(40, 40)));
    assert_no_candidate(&done);
    assert!(done.disclosure.iter().any(|l| l.is(copy::NO_CLEARANCE)));
    let refused = lab.scan(lab.path("missing.png"));
    assert_refused(&refused, copy::R_UNREADABLE);
    assert_ne!(done.status, Status::Refused);
}

// ---------------------------------------------------------------------------
// A reader that breaks on purpose: the demo markers, with one part failing.

use openworld_core::fixture::Markers;
use openworld_core::perception::{Aligned, Detection, Perception, Timing};
use openworld_core::quality::{Measured, Reject};

#[derive(Clone, Copy)]
enum Fault {
    /// The embedder returns a vector the cutoff cannot be applied to.
    ZeroVectors,
    EmbedFails,
    DetectFails,
    /// Every face fails this gate.
    Gate(Reject),
}

struct Faulty(Fault, Markers);

impl Perception for Faulty {
    fn kind(&self) -> &'static str {
        self.1.kind()
    }
    fn provider(&self) -> &str {
        "fixture"
    }
    fn min_face_px(&self) -> u32 {
        self.1.min_face_px()
    }
    fn high_score(&self) -> f32 {
        self.1.high_score()
    }
    fn compare_score(&self) -> f32 {
        self.1.compare_score()
    }
    fn dim(&self) -> usize {
        self.1.dim()
    }
    fn detect(&mut self, frame: &RgbImage, det_long: u32) -> Result<Vec<Detection>, String> {
        match self.0 {
            Fault::DetectFails => Err("the detector stopped".into()),
            _ => self.1.detect(frame, det_long),
        }
    }
    fn assess(&self, frame: &RgbImage, det: &Detection) -> Result<Measured, Reject> {
        match self.0 {
            Fault::Gate(reject) => Err(reject),
            _ => self.1.assess(frame, det),
        }
    }
    fn align(&mut self, frame: &RgbImage, det: &Detection, index: u64) -> Result<Option<Aligned>, String> {
        self.1.align(frame, det, index)
    }
    fn embed(&mut self, aligned: &Aligned) -> Result<Vec<f32>, String> {
        match self.0 {
            Fault::ZeroVectors => Ok(vec![0.0; self.dim()]),
            Fault::EmbedFails => Err("the embedder stopped".into()),
            _ => self.1.embed(aligned),
        }
    }
    fn cutoff(&self, bundle_threshold: f32) -> f32 {
        self.1.cutoff(bundle_threshold)
    }
    fn time(&mut self, _det_long: u32) -> Option<Timing> {
        None
    }
}

fn read_with(fault: Fault, frames: &[RgbImage]) -> ScanReport {
    let video = frames.len() > 1;
    feed_with(frames, Feed { video, fps: if video { 30.0 } else { 0.0 }, ..Feed::default() }, Box::new(Faulty(fault, Markers)))
}

#[test]
fn a_face_large_enough_to_compare_that_cannot_be_scored_is_not_cleared_or_under_the_cutoff() {
    let still = [scene_at(MISSING_ID, spot(MISSING_ID, &[0]))];
    let report = read_with(Fault::ZeroVectors, &still);
    assert_no_candidate(&report);
    let track = &report.tracks[0];
    assert_eq!(track.status, TrackStatus::Unscored);
    assert!(track.label.is(copy::FACE_UNSCORED));
    assert!(track.reason.is_none());
    assert!(track.face_px >= 112);
    assert_eq!(report.counts.faces_compared, 0, "an unscored face was not compared");
    assert_eq!(report.counts.faces_not_compared, 1);
    assert_eq!(report.faces_line.as_ref().map(|l| l.code.as_str()), Some("faces_some"));
}

#[test]
fn a_network_that_fails_mid_scan_makes_the_scan_incomplete() {
    let clip = sequence(10, 3..6, &scene_at(MISSING_ID, spot(MISSING_ID, &frames(3..6))));
    for fault in [Fault::EmbedFails, Fault::DetectFails] {
        let report = read_with(fault, &clip);
        assert_eq!(report.status, Status::Incomplete, "{}", describe(&report));
        assert_eq!(report.reason.as_ref().map(|l| l.code.as_str()), Some("runtime_failed"));
        assert!(report.headline.is(copy::INCOMPLETE));
        assert_not_a_clearance(&report);
        assert!(report.candidates.is_empty());
    }
}

#[test]
fn each_quality_gate_names_why_a_face_was_not_compared() {
    let still = [scene_at(MISSING_ID, spot(MISSING_ID, &[0]))];
    for (reject, why) in [
        (Reject::Small, copy::WHY_SMALL),
        (Reject::Unaligned, copy::WHY_UNALIGNED),
        (Reject::Confidence, copy::WHY_CONFIDENCE),
        (Reject::Pose, copy::WHY_POSE),
        (Reject::Blur, copy::WHY_BLUR),
        (Reject::Exposure, copy::WHY_EXPOSURE),
    ] {
        let report = read_with(Fault::Gate(reject), &still);
        assert_no_candidate(&report);
        let track = &report.tracks[0];
        assert_eq!(track.status, TrackStatus::NotCompared, "{reject:?}");
        assert!(track.label.is(copy::NOT_COMPARED));
        assert!(track.reason.as_ref().is_some_and(|l| l.is(why)), "{reject:?}: {:?}", track.reason);
        assert_eq!(track.quality_frames, 0);
    }
}
