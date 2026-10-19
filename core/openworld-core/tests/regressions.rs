// SPDX-License-Identifier: Apache-2.0

//! Bugs the rebuild fixed, each reproduced on the old scan core. A file that
//! lies about itself, or that the decoder chokes on, must end Incomplete or
//! Refused. It never reads as a finished scan with no candidate.

mod common;

use common::*;
use openworld_core::copy;
use openworld_core::decode::{self, container_created};
use openworld_core::ffi;
use openworld_core::report::{ScanReport, Status, TrackStatus};
use openworld_core::scan::{estimate_for, open_pushed_job};
use openworld_core::{DecodeEnd, MediaFacts};
use serde_json::json;
use std::alloc::{GlobalAlloc, Layout, System};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

/// The largest single allocation this test binary has made since the last reset.
struct Largest;

static LARGEST: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Largest {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        LARGEST.fetch_max(new_size, Ordering::Relaxed);
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static ALLOCATOR: Largest = Largest;

const MB: usize = 1 << 20;

/// Not a finished scan with nothing found: either it found the marker, or it says it did not finish.
fn assert_not_a_silent_clearance(report: &ScanReport) {
    let found = report.tracks.iter().any(|t| matches!(t.status, TrackStatus::Candidate | TrackStatus::TooBrief));
    assert!(report.status != Status::Complete || found, "a complete scan with no candidate: {}", describe(report));
    if report.status != Status::Complete {
        assert_not_a_clearance(report);
    }
}

/// Run a scan on its own thread and give up after `limit`.
fn scan_within(req: openworld_core::ScanRequest, limit: Duration) -> (ScanReport, Duration) {
    let (send, receive) = mpsc::channel();
    let started = Instant::now();
    std::thread::spawn(move || {
        let _ = send.send(run(&req));
    });
    let report = receive.recv_timeout(limit).unwrap_or_else(|_| panic!("the scan did not finish within {limit:?}"));
    (report, started.elapsed())
}

// 1. `scan --out` at a folder that is not ours.

#[test]
fn a_result_folder_that_is_not_ours_or_holds_the_input_is_refused_and_left_alone() {
    let lab = Lab::new();
    let videos = lab.path("videos");
    fs::create_dir_all(&videos).unwrap();
    let input = videos.join("clip.png");
    openworld_core::fixture::demo_scene().image.save(&input).unwrap();
    let input_bytes = fs::read(&input).unwrap();
    fs::write(videos.join("other.mov"), b"another clip").unwrap();
    let precious = lab.path("precious");
    fs::create_dir_all(precious.join("drafts")).unwrap();
    fs::write(precious.join("thesis.docx"), b"years of work").unwrap();
    fs::write(precious.join("drafts/notes.txt"), b"more work").unwrap();
    let untouched = |dir: &Path| {
        assert_eq!(fs::read(precious.join("thesis.docx")).unwrap(), b"years of work");
        assert_eq!(fs::read(precious.join("drafts/notes.txt")).unwrap(), b"more work");
        assert_eq!(fs::read(&input).unwrap(), input_bytes);
        assert_eq!(fs::read(videos.join("other.mov")).unwrap(), b"another clip");
        assert!(!dir.join(".openworld-result").exists(), "{} was marked as a result", dir.display());
        assert!(!dir.join("result.json").exists());
    };

    for (out, code) in [(precious.clone(), copy::R_OUT_FOREIGN), (videos.clone(), copy::R_OUT_OVERLAP)] {
        // The desktop scan.
        let mut req = lab.request(&input);
        req.out = out.clone();
        assert_refused(&run(&req), code);
        untouched(&out);
        // The typed call the desktop window sends.
        let call = json!({"v": 1, "op": "scan", "input": input, "bundles": catalog(), "posters": lab.posters, "out": out});
        let answer = openworld_core::invoke::call(&call.to_string(), None, &mut |_| {});
        assert_eq!(answer["refusal"], code.0, "{answer}");
        untouched(&out);
        // A phone job.
        req.media = Some(MediaFacts { width: W, height: H, frames: 1, ..Default::default() });
        assert_refused(&open_pushed_job(&req, SystemTime::now()).err().expect("refused"), code);
        untouched(&out);
    }
    // Beside the input is fine.
    let mut req = lab.request(&input);
    req.out = lab.path("videos-result");
    assert_candidate(&run(&req), MISSING_POSTER);
}

// 2. An AVI whose headers count fewer frames than it stores.

/// Overwrite the AVI main header's dwTotalFrames, the video stream header's
/// dwLength, and the OpenDML total, so the headers claim `frames`.
fn claim_frames(avi: &mut [u8], frames: u32) {
    let find = |bytes: &[u8], tag: &[u8]| bytes.windows(4).position(|w| w == tag);
    let avih = find(avi, b"avih").expect("avih") + 8;
    avi[avih + 16..avih + 20].copy_from_slice(&frames.to_le_bytes());
    let strh = find(avi, b"strh").expect("strh") + 8;
    assert_eq!(&avi[strh..strh + 4], b"vids");
    avi[strh + 32..strh + 36].copy_from_slice(&frames.to_le_bytes());
    if let Some(dmlh) = find(avi, b"dmlh") {
        avi[dmlh + 8..dmlh + 12].copy_from_slice(&frames.to_le_bytes());
    }
}

#[test]
fn an_avi_whose_headers_claim_five_frames_of_a_hundred_is_read_to_the_end() {
    let lab = Lab::new();
    let (w, h) = (320, 240);
    let at = spot_in(MISSING_ID, SIDE, &frames(50..55), 16..w - SIDE - 16, 16..h - SIDE - 16);
    let mut marked = blank(w, h);
    place(&mut marked, MISSING_ID, BIG, at);
    let pictures: Vec<_> = (0..100u32)
        .map(|i| {
            let mut frame = if (50..55).contains(&i) { marked.clone() } else { blank(w, h) };
            frame.put_pixel(i % w, 0, image::Rgb([0, 0, 0]));
            frame
        })
        .collect();
    let honest = lab.path("honest.avi");
    encode(lab.dir.path(), &pictures, 25, &["-c:v", "mjpeg", "-q:v", "1"], &honest);
    let mut bytes = fs::read(&honest).unwrap();
    claim_frames(&mut bytes, 5);
    let lying = lab.write("lying.avi", &bytes);
    // FFprobe believes the header: five frames, a fifth of a second.
    let listed = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=nb_frames", "-of", "csv=p=0"])
        .arg(&lying)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&listed.stdout).trim(), "5");

    let probed = decode::probe(&lying).unwrap();
    let report = lab.scan(&lying);
    eprintln!("lying AVI: {}", describe(&report));
    assert_not_a_silent_clearance(&report);
    // Every stored picture is scanned, so the marker on frames 51 to 55 is found.
    // The header that disagrees keeps the scan from reading as complete.
    assert_eq!(report.counts.frames_decoded, 100, "{probed:?} {}", describe(&report));
    assert!(report.candidates.iter().any(|c| c.poster.id == MISSING_POSTER), "{}", describe(&report));
    assert_eq!(report.status, Status::Incomplete, "{}", describe(&report));
    // Measured coverage reads the marker's frames too.
    let measured = lab.scan_measured(&lying);
    assert_not_a_silent_clearance(&measured);
}

// 3. A garbage-filled MPEG-TS that makes FFmpeg print error after error.

#[test]
fn a_corrupt_transport_stream_finishes_quickly_and_is_not_complete() {
    let lab = Lab::new();
    let clean = lab.path("clean.ts");
    let pictures: Vec<_> = (0..100).map(plain).collect();
    encode(lab.dir.path(), &pictures, 25, &["-c:v", "mpeg2video", "-q:v", "4", "-f", "mpegts"], &clean);
    let mut bytes = fs::read(&clean).unwrap();
    assert_eq!(bytes[0], 0x47, "a transport stream");
    // Keep the tables and the first pictures. After that, every other 188-byte packet
    // keeps its header and gets a garbage payload.
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    for (index, packet) in bytes.chunks_mut(188).enumerate().skip(40) {
        if index % 2 == 0 && packet.len() == 188 {
            for byte in &mut packet[4..] {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                *byte = seed as u8;
            }
        }
    }
    let corrupt = lab.write("corrupt.ts", &bytes);
    let (report, took) = scan_within(lab.request(&corrupt), Duration::from_secs(90));
    eprintln!("corrupt TS in {took:?}: {}", describe(&report));
    assert!(took < Duration::from_secs(60), "took {took:?}");
    assert!(matches!(report.status, Status::Incomplete | Status::Refused), "{}", describe(&report));
    assert_not_a_clearance(&report);
}

// 4. A Matroska file that is nothing but nested EBML headers.

#[test]
fn a_matroska_file_of_nested_headers_does_not_crash_the_scan() {
    let lab = Lab::new();
    let total = 4 * MB;
    let mut bytes = Vec::with_capacity(total);
    while bytes.len() + 12 <= total {
        // EBML header ID, then an eight-byte size that runs to the end of the file.
        let rest = (total - bytes.len() - 12) as u64;
        bytes.extend_from_slice(&[0x1A, 0x45, 0xDF, 0xA3, 0x01]);
        bytes.extend_from_slice(&rest.to_be_bytes()[1..]);
    }
    let nested = lab.write("nested.mkv", &bytes);
    assert_eq!(container_created(&nested), None);
    let (report, _) = scan_within(lab.request(&nested), Duration::from_secs(60));
    eprintln!("nested EBML: {}", describe(&report));
    assert!(matches!(report.status, Status::Refused | Status::Incomplete), "{}", describe(&report));
    assert_not_a_clearance(&report);
    // A phone job reads the same clock from the file.
    let mut req = lab.request(&nested);
    req.media = Some(MediaFacts { width: 64, height: 64, frames: 1, ..Default::default() });
    let mut job = open_pushed_job(&req, SystemTime::now()).expect("the clock is not a refusal");
    job.engine().push(0, &blank(64, 64), &mut |_| {});
    let declared = job.declared_frames;
    assert_complete(&job.finish(DecodeEnd { clean: true, declared_frames: declared, reason: None }, &mut |_| {}));
}

// 5. A picture whose header claims more pixels than a scan can hold.

fn huge_png(width: u32, height: u32) -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    png.extend(png_chunk(b"IHDR", &ihdr));
    // A few bytes of zlib: nowhere near the pixels the header promises.
    png.extend(png_chunk(b"IDAT", &[0x78, 0x9C, 0x63, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01]));
    png.extend(png_chunk(b"IEND", &[]));
    png
}

#[test]
fn a_png_that_claims_sixteen_thousand_pixels_square_is_refused_without_allocating_it() {
    let lab = Lab::new();
    let png = lab.write("huge.png", &huge_png(16_000, 16_000));
    assert!(fs::metadata(&png).unwrap().len() < 100);
    LARGEST.store(0, Ordering::Relaxed);
    let started = Instant::now();
    let req = lab.request(&png);
    let report = run(&req);
    let estimate = estimate_for(&req);
    let took = started.elapsed();
    let largest = LARGEST.load(Ordering::Relaxed);
    assert_refused(&report, copy::R_TOO_LARGE);
    assert!(!req.out.exists());
    assert!(estimate.unwrap_err().headline.is(copy::R_TOO_LARGE));
    assert!(matches!(decode::probe(&png), Err(decode::MediaError::TooLarge)));
    assert!(took < Duration::from_secs(5), "took {took:?}");
    assert!(largest < 256 * MB, "largest allocation {} MB", largest / MB);
}

/// `pages` TIFF pages that each claim `side` x `side` RGB and store no pixels.
fn hollow_tiff(pages: usize, side: u32) -> Vec<u8> {
    let list: Vec<TiffPage> = (0..pages)
        .map(|_| TiffPage { width: side, height: side, photometric: 2, bits: vec![8; 3], data: Vec::new(), orientation: 1, extra: vec![] })
        .collect();
    tiff(&list)
}

#[test]
fn a_tiff_with_thousands_of_pages_is_counted_from_its_headers() {
    let lab = Lab::new();
    // Two thousand pages of 4000 x 4000 that store no pixels: counting them
    // decodes nothing, and reading any one of them fails.
    let pages = lab.write("pages.tif", &hollow_tiff(2000, 4000));
    LARGEST.store(0, Ordering::Relaxed);
    let started = Instant::now();
    let probed = decode::probe(&pages).expect("probed from the headers");
    let took = started.elapsed();
    assert_eq!(probed.frames, 2000);
    assert!(probed.video && probed.frames_exact);
    assert_eq!((probed.width, probed.height), (4000, 4000));
    assert!(took < Duration::from_secs(10), "took {took:?}");
    assert!(LARGEST.load(Ordering::Relaxed) < 16 * MB, "probing allocated {} MB", LARGEST.load(Ordering::Relaxed) / MB);
    let mut req = lab.request(&pages);
    req.coverage = openworld_core::Coverage::Measured;
    assert_eq!(estimate_for(&req).unwrap().frames_analyzed, 2000, "one page a second, every page");
    let report = lab.scan(&pages);
    assert!(matches!(report.status, Status::Incomplete | Status::Refused), "{}", describe(&report));
    assert_not_a_clearance(&report);

    // More pages than a scan reads is refused before any is decoded.
    let many = lab.write("many.tif", &many_pages());
    assert!(decode::probe(&many).is_err());
    let report = lab.scan(&many);
    assert!(report.is_refused(), "{}", describe(&report));
    assert_not_a_clearance(&report);
}

/// One page more than a scan reads.
fn many_pages() -> Vec<u8> {
    tiff(&(0..10_001).map(|_| TiffPage::rgb(&blank(1, 1), 1)).collect::<Vec<_>>())
}

#[test]

fn a_tiff_with_more_pages_than_a_scan_reads_is_refused_as_too_large() {
    let lab = Lab::new();
    let many = lab.write("many.tif", &many_pages());
    assert!(matches!(decode::probe(&many), Err(decode::MediaError::TooLarge)), "{:?}", decode::probe(&many).err());
    assert_refused(&lab.scan(&many), copy::R_TOO_LARGE);
}

// 6. A poster pack whose vectors were scaled and re-pinned.

#[test]
fn a_pack_whose_vectors_were_scaled_and_re_pinned_is_refused() {
    let lab = Lab::new();
    let input = lab.png("scene.png", &openworld_core::fixture::demo_scene().image);
    lab.edit_pack(|pack| {
        for poster in pack["posters"].as_array_mut().unwrap() {
            let scaled: Vec<f64> = poster["embedding"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap() * 50.0).collect();
            poster["embedding"] = json!(scaled);
        }
    });
    // The hash matches: the pack loads, and its vectors are what fail.
    assert!(openworld_core::load_pack(&lab.posters, SystemTime::now()).is_ok());
    let report = lab.scan(&input);
    assert_refused(&report, copy::R_PACK_VECTORS);
    assert!(report.candidates.is_empty());
}

// 7. A phone job that promises more frames than it pushes.

#[test]
fn a_phone_job_that_declares_a_hundred_frames_and_pushes_two_is_incomplete() {
    let lab = Lab::new();
    let scene = openworld_core::fixture::demo_scene().image;
    let mut req = lab.request(lab.path("clip.mov"));
    req.media = Some(MediaFacts { width: W, height: H, fps: 30.0, frames: 100, duration_sec: 0.0, video: true, container_unix: None });
    let mut job = open_pushed_job(&req, SystemTime::now()).unwrap();
    assert_eq!(job.declared_frames, Some(100));
    for i in 0..2 {
        assert!(job.engine().push(i, &scene, &mut |_| {}));
    }
    let declared = job.declared_frames;
    let report = job.finish(DecodeEnd { clean: true, declared_frames: declared, reason: None }, &mut |_| {});
    assert_incomplete(&report, copy::NOT_FULLY_DECODED);
    assert_eq!(report.counts.frames_declared, Some(100));
    assert_eq!(report.counts.frames_decoded, 2);
    assert!(report.candidates.is_empty(), "{}", describe(&report));

    // The same through the C interface the shells link, which finishes with the promised count.
    let request = json!({"v": 1, "bundles": catalog(), "posters": lab.posters, "out": lab.out(),
        "media": {"width": W, "height": H, "frames": 100, "fps": 30.0, "video": true}});
    let text = std::ffi::CString::new(request.to_string()).unwrap();
    let job = ffi::ow_job_start(text.as_ptr(), None, std::ptr::null_mut(), std::ptr::null_mut());
    assert!(!job.is_null());
    for i in 0..2 {
        assert_eq!(ffi::ow_job_push_pixels(job, i, scene.as_raw().as_ptr(), W, H, W * 3, ffi::OW_RGB, 0), 1);
    }
    let end = std::ffi::CString::new(r#"{"clean":true}"#).unwrap();
    let answer = ffi::ow_job_finish(job, end.as_ptr());
    let report: serde_json::Value = serde_json::from_str(unsafe { std::ffi::CStr::from_ptr(answer) }.to_str().unwrap()).unwrap();
    ffi::ow_string_free(answer);
    assert_eq!(report["status"], "incomplete");
    assert_eq!(report["reason"]["code"], "not_fully_decoded");
    assert_eq!(report["counts"]["frames_declared"], 100);
}

#[test]
fn a_video_stretched_past_the_pixel_limit_by_its_sample_aspect_is_refused() {
    let lab = Lab::new();
    // 1920x1080 shown 100 times as wide: 192,000 x 1080, 207 megapixels a frame.
    let clip = lab.path("wide.mp4");
    encode(lab.dir.path(), &[blank(1920, 1080), blank(1920, 1080)], 25, &["-c:v", "libx264", "-vf", "setsar=100"], &clip);
    assert!(matches!(decode::probe(&clip), Err(decode::MediaError::TooLarge)));
    assert_refused(&lab.scan(&clip), copy::R_TOO_LARGE);
}
