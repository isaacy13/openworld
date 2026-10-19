// SPDX-License-Identifier: Apache-2.0

//! Update posters: read the public FBI list, keep the open, named missing and
//! wanted posters, check each one's photo on this device, and write a new pack.
//!
//! It runs only when the selected official bundle's curve allows real photos.
//! No curve does yet. Only `https://` addresses on api.fbi.gov, www.fbi.gov,
//! and fbi.gov are requested, and nothing about the user's files is sent. Each
//! response body is hashed; a header is never an input.
//!
//! A poster gets a vector only when its lead photo shows exactly one face and
//! that face passes the quality gates at the compare size. Every box the
//! detector reports counts, however small or unsure: a second face may be the
//! person the poster is about, and a vector of the wrong face matches the wrong
//! person. Any other poster is left out and counted with its reason.
//!
//! The new pack is written beside the old one and moved into place only after
//! it loads and checks. A failed update leaves the old pack as it was.

use crate::bundle::{load_bundles, Bundle};
use crate::copy::{self, Copy, Line};
use crate::decode::{still_from_bytes, MediaError};
use crate::engine::Event;
use crate::perception::{auto_long_side, valid_vector, Perception};
use crate::posters::{hash_response_body, load_pack, sha256_hex, write_network_pack, Poster, PosterClass, PosterPack, MAX_AGE};
use crate::quality::Reject;
use crate::timeutil::format_rfc3339;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

pub const LIST_URL: &str = "https://api.fbi.gov/wanted/v1/list";
pub const PAGE_SIZE: u64 = 50;
/// A list that says it has more pages than this is not read.
const MAX_PAGES: u64 = 200;
pub const MAX_LIST_BYTES: usize = 8 << 20;
pub const MAX_IMAGE_BYTES: usize = 20 << 20;
/// Waits between requests, so the update is a light load on the FBI.
pub const PAGE_PAUSE: Duration = Duration::from_secs(1);
pub const IMAGE_PAUSE: Duration = Duration::from_millis(250);
/// A progress event every this many poster photos.
const CHECKED_EVERY: u64 = 10;
const HOSTS: [&str; 3] = ["api.fbi.gov", "www.fbi.gov", "fbi.gov"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchError {
    /// Not an FBI address, or a redirect off one.
    Refused,
    /// The body was over the limit.
    TooLarge,
    /// The server answered with this status.
    Status(u16),
    /// No answer: offline, timed out, or TLS failed.
    Network(String),
}

pub trait Transport {
    /// The body of a GET, at most `limit` bytes. Only called with an address `fbi_url` allows.
    fn get(&mut self, url: &str, limit: usize) -> Result<Vec<u8>, FetchError>;
    /// A polite wait before the next request.
    fn pause(&mut self, _wait: Duration) {}
}

/// An `https://` address on an FBI host, with no port, user name, or odd characters.
pub fn fbi_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    url.bytes().all(|b| b.is_ascii_graphic()) && HOSTS.contains(&host.to_ascii_lowercase().as_str())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftPoster {
    pub id: String,
    pub class: PosterClass,
    pub title: String,
    pub fbi_url: String,
    /// The lead photo at its largest size on an FBI host.
    pub photo: Option<String>,
}

/// What an update wrote, and what it left out and why.
#[derive(Debug)]
pub struct Updated {
    pub pack: PosterPack,
    pub kept: u64,
    pub skipped: Vec<(Copy, u64)>,
    /// SHA-256 of the list pages' body hashes, one per line, in page order.
    pub list_sha256: String,
}

/// The bundle whose models build the vectors. Real photos stay off unless an
/// official curve allows them, and the selected bundle must be one of those.
pub fn update_bundle(bundles: &Path, id: &str) -> Result<Bundle, Line> {
    let all = load_bundles(bundles).unwrap_or_default();
    if !all.iter().any(Bundle::real_posters_allowed) {
        return Err(Line::of(copy::R_REAL_POSTERS_OFF));
    }
    let bundle = all.into_iter().find(|b| b.id == id).ok_or_else(|| Line::of(copy::R_NO_BUNDLE))?;
    if !bundle.real_posters_allowed() {
        return Err(Line::of(copy::R_REAL_POSTERS_OFF));
    }
    if !bundle.weights_ready {
        return Err(Line::of(copy::R_NO_WEIGHTS));
    }
    Ok(bundle)
}

/// The whole update: the gate, the selected bundle's models, HTTPS, and the pack in `dest`.
pub fn update_from_catalog(bundles: &Path, id: &str, dest: &Path, events: &mut dyn FnMut(Event)) -> Result<Updated, Line> {
    let bundle = update_bundle(bundles, id)?;
    let mut transport = https()?;
    if !replaceable(dest) {
        return Err(Line::of(copy::R_POSTERS_FOREIGN));
    }
    let embedder = bundle.weight("embedder").map(|w| w.sha256.clone()).unwrap_or_default();
    // The embedder runs on the CPU, as it does in a scan; the cutoff was measured there.
    let mut models = crate::onnx_exec::load(&bundle, false).map_err(|message| Line::with(copy::R_RUNTIME.0, message, &[]))?;
    update_posters(dest, &embedder, transport.as_mut(), &mut models, SystemTime::now(), events)
}

#[cfg(feature = "network")]
fn https() -> Result<Box<dyn Transport>, Line> {
    Ok(Box::new(crate::net::Https::new()))
}

#[cfg(not(feature = "network"))]
fn https() -> Result<Box<dyn Transport>, Line> {
    Err(Line::of(copy::R_NETWORK_OFF))
}

/// Fetch every page of the list, check each kept poster's photo with
/// `perception`, and replace the pack in `dest`. `now` is the fetch time.
pub fn update_posters(
    dest: &Path,
    embedder_sha256: &str,
    transport: &mut dyn Transport,
    perception: &mut dyn Perception,
    now: SystemTime,
    events: &mut dyn FnMut(Event),
) -> Result<Updated, Line> {
    let unwritten = |_| Line::of(copy::R_POSTERS_UNWRITTEN);
    if !replaceable(dest) {
        return Err(Line::of(copy::R_POSTERS_FOREIGN));
    }
    let listing = fetch_list(transport, events)?;
    let mut skipped = Vec::new();
    count(&mut skipped, copy::SKIP_NOT_OPEN, listing.not_open);
    // The photos are written as they pass, so they are never all in memory.
    let parent = match dest.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    fs::create_dir_all(parent).map_err(unwritten)?;
    let work = tempfile::Builder::new().prefix(".posters-update-").tempdir_in(parent).map_err(unwritten)?;
    let staged = work.path().join("new");
    fs::create_dir_all(staged.join("images")).map_err(unwritten)?;
    let total = listing.posters.len() as u64;
    let mut posters = Vec::new();
    for (i, draft) in listing.posters.into_iter().enumerate() {
        if draft.photo.is_some() {
            transport.pause(IMAGE_PAUSE);
        }
        match check_photo(draft.photo.as_deref(), transport, perception) {
            Ok(photo) => {
                fs::write(staged.join(&photo.name), &photo.bytes).map_err(unwritten)?;
                posters.push(Poster {
                    id: draft.id,
                    class: draft.class,
                    title: draft.title,
                    fbi_url: Some(draft.fbi_url),
                    embedding: Some(photo.embedding),
                    image: Some(photo.name),
                    image_sha256: Some(photo.sha256),
                    expires_at: None,
                });
            }
            Err(Miss::Skip(why)) => count(&mut skipped, why, 1),
            Err(Miss::Runtime) => return Err(Line::of(copy::R_POSTERS_RUNTIME)),
        }
        let checked = i as u64 + 1;
        if checked.is_multiple_of(CHECKED_EVERY) || checked == total {
            events(Event::PostersChecked { checked, posters: total });
        }
    }
    if posters.is_empty() {
        return Err(Line::of(copy::R_POSTERS_NONE));
    }
    let kept = posters.len() as u64;
    let id = format!("fbi-{}", &listing.list_sha256[..16]);
    let source = format!("api.fbi.gov/wanted/v1/list, fetched {}", format_rfc3339(now));
    write_network_pack(&staged, &id, &source, embedder_sha256, posters, now, now + MAX_AGE).map_err(|_| Line::of(copy::R_POSTERS_UNWRITTEN))?;
    commit(work.path(), dest)?;
    let pack = load_pack(dest, now).map_err(|_| Line::of(copy::R_POSTERS_UNWRITTEN))?;
    Ok(Updated { pack, kept, skipped, list_sha256: listing.list_sha256 })
}

fn count(skipped: &mut Vec<(Copy, u64)>, why: Copy, n: u64) {
    if n == 0 {
        return;
    }
    match skipped.iter_mut().find(|(c, _)| c.0 == why.0) {
        Some((_, total)) => *total += n,
        None => skipped.push((why, n)),
    }
}

/// Nothing there, an empty folder, or a pack an update may replace: only the
/// snapshot, its hash, and the photos folder.
fn replaceable(dest: &Path) -> bool {
    match fs::read_dir(dest) {
        Ok(entries) => entries.flatten().all(|e| matches!(e.file_name().to_str(), Some("snapshot.json" | "snapshot.sha256" | "images"))),
        Err(_) => fs::symlink_metadata(dest).is_err(),
    }
}

/// Move `work/new` to `dest`. The old pack is set aside first and put back if the move fails.
fn commit(work: &Path, dest: &Path) -> Result<(), Line> {
    let old = work.join("old");
    let had_old = fs::symlink_metadata(dest).is_ok();
    if had_old && fs::rename(dest, &old).is_err() {
        return Err(Line::of(copy::R_POSTERS_UNWRITTEN));
    }
    if fs::rename(work.join("new"), dest).is_err() {
        if had_old {
            let _ = fs::rename(&old, dest);
        }
        return Err(Line::of(copy::R_POSTERS_UNWRITTEN));
    }
    Ok(())
}

#[derive(Default)]
struct Listing {
    posters: Vec<DraftPoster>,
    not_open: u64,
    list_sha256: String,
}

/// Every page, until the list's `total` is reached, or a short page when it gives none.
fn fetch_list(transport: &mut dyn Transport, events: &mut dyn FnMut(Event)) -> Result<Listing, Line> {
    let bad = || Line::of(copy::R_POSTERS_BAD_LIST);
    let mut listing = Listing::default();
    let mut hashes = Vec::new();
    let mut seen = HashSet::new();
    for page in 1..=MAX_PAGES {
        if page > 1 {
            transport.pause(PAGE_PAUSE);
        }
        let url = format!("{LIST_URL}?page={page}&pageSize={PAGE_SIZE}");
        let body = transport.get(&url, MAX_LIST_BYTES).map_err(|_| Line::of(copy::R_POSTERS_FETCH))?;
        hashes.push(hash_response_body(&body, None));
        let list: Value = serde_json::from_slice(&body).map_err(|_| bad())?;
        let items = list.get("items").and_then(Value::as_array).ok_or_else(bad)?;
        let pages = list.get("total").and_then(Value::as_u64).map(|total| total.div_ceil(PAGE_SIZE));
        if pages.is_some_and(|p| p > MAX_PAGES) {
            return Err(bad());
        }
        events(Event::PostersPage { page: page as u32, pages: pages.map(|p| p as u32) });
        for item in items {
            // Pages can shift while they are read, so a poster can come twice.
            let key = item.get("uid").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| item.to_string());
            if !seen.insert(key) {
                continue;
            }
            match classify_item(item) {
                Some(poster) => listing.posters.push(poster),
                None => listing.not_open += 1,
            }
        }
        let last = match pages {
            Some(pages) => page >= pages,
            None => (items.len() as u64) < PAGE_SIZE,
        };
        if last || items.is_empty() {
            listing.list_sha256 = sha256_hex(hashes.join("\n").as_bytes());
            return Ok(listing);
        }
    }
    Err(bad())
}

/// A poster photo that passed, ready to store beside the snapshot.
struct Photo {
    name: String,
    sha256: String,
    bytes: Vec<u8>,
    embedding: Vec<f32>,
}

enum Miss {
    /// Left out, for this reason.
    Skip(Copy),
    /// The models failed. The update stops.
    Runtime,
}

impl From<Copy> for Miss {
    fn from(why: Copy) -> Self {
        Miss::Skip(why)
    }
}

impl From<String> for Miss {
    fn from(_message: String) -> Self {
        Miss::Runtime
    }
}

/// Download one photo and read it the way a scan reads a still: one face,
/// through the gates, aligned, and embedded.
fn check_photo(url: Option<&str>, transport: &mut dyn Transport, perception: &mut dyn Perception) -> Result<Photo, Miss> {
    let url = url.ok_or(copy::SKIP_NO_PHOTO)?;
    let bytes = transport.get(url, MAX_IMAGE_BYTES).map_err(|err| match err {
        FetchError::TooLarge => copy::SKIP_PHOTO_TOO_LARGE,
        _ => copy::SKIP_PHOTO_UNAVAILABLE,
    })?;
    let frame = still_from_bytes(&bytes).map_err(|err| match err {
        MediaError::TooLarge => copy::SKIP_PHOTO_TOO_LARGE,
        MediaError::BadCodec => copy::SKIP_NOT_PICTURE,
    })?;
    let det_long = auto_long_side(frame.width().max(frame.height()), perception.min_face_px());
    let found = perception.detect(&frame, det_long)?;
    let det = match found.as_slice() {
        [one] => one,
        [] => return Err(copy::SKIP_NO_FACE.into()),
        _ => return Err(copy::SKIP_SEVERAL_FACES.into()),
    };
    perception.assess(&frame, det).map_err(Reject::copy)?;
    let aligned = perception.align(&frame, det, 0)?.ok_or(Reject::Unaligned.copy())?;
    let embedding = perception.embed(&aligned)?;
    if !valid_vector(&embedding, perception.dim()) {
        return Err(copy::FACE_UNSCORED.into());
    }
    let ext = match image::guess_format(&bytes) {
        Ok(image::ImageFormat::Png) => "png",
        Ok(image::ImageFormat::WebP) => "webp",
        _ => "jpg",
    };
    let sha256 = sha256_hex(&bytes);
    Ok(Photo { name: format!("images/{sha256}.{ext}"), sha256, bytes, embedding })
}

/// Missing and wanted are classes, not names. ECAP, unidentified remains, and
/// unnamed Seeking Information are skipped, and so is a poster the source marks
/// resolved. A named Seeking Information poster is kept as wanted.
pub fn classify_item(item: &Value) -> Option<DraftPoster> {
    // A poster the source marks resolved is dropped: captured, located, recovered, deceased, surrendered.
    let status = item.get("status").and_then(|v| v.as_str()).unwrap_or("").to_ascii_lowercase();
    if ["captured", "located", "recovered", "deceased", "surrendered", "found", "resolved"].iter().any(|s| status.contains(s)) {
        return None;
    }
    let classification = item.get("poster_classification").and_then(|v| v.as_str()).unwrap_or("");
    let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("").trim();
    let class_up = classification.to_ascii_uppercase();
    let title_up = title.to_ascii_uppercase();
    if class_up.contains("ECAP") {
        return None;
    }
    if class_up.contains("UNIDENTIFIED") || title_up.contains("UNIDENTIFIED") || title_up.contains("REMAINS") {
        return None;
    }
    let seeking = class_up.contains("SEEKING");
    if seeking && unnamed(&title_up) {
        return None;
    }
    let class = if class_up.contains("MISSING") {
        PosterClass::Missing
    } else if class_up.contains("WANTED") || seeking {
        PosterClass::Wanted
    } else {
        return None;
    };
    if unnamed(&title_up) {
        return None;
    }
    let id = item.get("uid").and_then(|v| v.as_str()).unwrap_or(title).to_string();
    let fbi_url = item.get("url").and_then(|v| v.as_str()).unwrap_or("https://www.fbi.gov/wanted").to_string();
    if !(fbi_url.starts_with("https://www.fbi.gov/") || fbi_url.starts_with("https://fbi.gov/")) {
        return None;
    }
    Some(DraftPoster { id, class, title: title.to_string(), fbi_url, photo: lead_photo(item) })
}

/// The first photo only: later ones can be other people, composites, or
/// age-progressed pictures. `original` is preferred, then `large`.
fn lead_photo(item: &Value) -> Option<String> {
    let first = item.get("images")?.as_array()?.first()?;
    ["original", "large"].iter().filter_map(|size| first.get(size)?.as_str()).find(|url| fbi_url(url)).map(str::to_string)
}

fn unnamed(title_up: &str) -> bool {
    let t = title_up.trim();
    t.is_empty() || t == "SEEKING INFORMATION" || t == "UNKNOWN" || t.contains("UNIDENTIFIED") || t.contains("REMAINS")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_fbi_addresses_are_requested() {
        for good in [LIST_URL, "https://www.fbi.gov/wanted/x/@@images/image/large", "https://fbi.gov/x", "https://WWW.FBI.GOV/x", "https://www.fbi.gov"] {
            assert!(fbi_url(good), "{good}");
        }
        for bad in [
            "http://www.fbi.gov/x",
            "https://www.fbi.gov.evil.example/x",
            "https://evil.example/www.fbi.gov/",
            "https://user@www.fbi.gov/x",
            "https://www.fbi.gov:8443/x",
            "https://cdn.fbi.gov/x",
            "https://www.fbi.gov\\@evil.example/",
            "https://www.fbi.gov/a b",
            "ftp://www.fbi.gov/x",
            "",
        ] {
            assert!(!fbi_url(bad), "{bad}");
        }
    }

    #[test]
    fn list_parser_skips_the_closed_classes() {
        let items = [
            json!({"uid":"w","title":"JANE EXAMPLE","poster_classification":"Wanted","url":"https://www.fbi.gov/wanted/jane"}),
            json!({"uid":"m","title":"JOHN EXAMPLE","poster_classification":"Missing","url":"https://www.fbi.gov/wanted/john"}),
            json!({"uid":"e","title":"CHILD EXAMPLE","poster_classification":"ECAP","url":"https://www.fbi.gov/wanted/child"}),
            json!({"uid":"u","title":"UNIDENTIFIED REMAINS","poster_classification":"Seeking Information","url":"https://www.fbi.gov/wanted/remains"}),
            json!({"uid":"s","title":"SEEKING INFORMATION","poster_classification":"Seeking Information","url":"https://www.fbi.gov/wanted/seek"}),
            json!({"uid":"n","title":"CASEY EXAMPLE","poster_classification":"Seeking Information","url":"https://www.fbi.gov/wanted/casey"}),
            json!({"uid":"c","title":"CAUGHT EXAMPLE","poster_classification":"Wanted","status":"captured","url":"https://www.fbi.gov/wanted/caught"}),
            json!({"uid":"l","title":"LOCATED EXAMPLE","poster_classification":"Missing","status":"located","url":"https://www.fbi.gov/wanted/located"}),
            json!({"uid":"x","title":"OFFSITE EXAMPLE","poster_classification":"Wanted","url":"https://evil.example/wanted"}),
        ];
        let kept: Vec<_> = items.iter().filter_map(classify_item).collect();
        let ids: Vec<_> = kept.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["w", "m", "n"]);
        assert_eq!(kept[0].class, PosterClass::Wanted);
        assert_eq!(kept[1].class, PosterClass::Missing);
        assert_eq!(kept[2].class, PosterClass::Wanted);
        // A poster with nothing but a name and class is tolerated, and has no photo.
        let bare = classify_item(&json!({"title":"BARE EXAMPLE","poster_classification":"Missing"})).unwrap();
        assert_eq!((bare.id.as_str(), bare.photo, bare.fbi_url.as_str()), ("BARE EXAMPLE", None, "https://www.fbi.gov/wanted"));
    }

    #[test]
    fn the_lead_photo_is_the_largest_on_an_fbi_host() {
        let photo = |images: Value| lead_photo(&json!({ "images": images }));
        let fbi = "https://www.fbi.gov/wanted/a/@@images/image/large";
        assert_eq!(photo(json!([{"original": "https://www.fbi.gov/o.jpg", "large": fbi}])).as_deref(), Some("https://www.fbi.gov/o.jpg"));
        assert_eq!(photo(json!([{"original": "https://evil.example/o.jpg", "large": fbi, "thumb": "https://www.fbi.gov/t.jpg"}])).as_deref(), Some(fbi));
        // A thumbnail is too small to check, and a later photo may be someone else.
        assert_eq!(photo(json!([{"thumb": "https://www.fbi.gov/t.jpg"}, {"original": fbi}])), None);
        assert_eq!(photo(json!([])), None);
        assert_eq!(photo(json!("not a list")), None);
    }

    #[test]
    fn real_posters_stay_off_without_an_official_curve() {
        let shipped = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bundles");
        assert_eq!(update_bundle(&shipped, "fast").unwrap_err(), Line::of(copy::R_REAL_POSTERS_OFF));
        // A developer bundle's curve may say yes; it is not official, so it stays off.
        let root = tempfile::tempdir().unwrap();
        let dir = crate::bundle::tests::dev_bundle(root.path(), "dev", 0.5, &[]);
        let curve = r#"{"schema": "openworld.curve.v1", "picker_line": "Measured.", "real_posters_allowed": true, "chance": {"fmr_at_cutoff": 1e-6, "source": "LFW"}}"#;
        fs::write(dir.join("curve.json"), curve).unwrap();
        let manifest = fs::read_to_string(dir.join("manifest.toml")).unwrap().replace("estimate_factor = 1.0", "estimate_factor = 1.0\ncurve = \"curve.json\"");
        fs::write(dir.join("manifest.toml"), manifest).unwrap();
        assert!(load_bundles(root.path()).unwrap()[0].curve.as_ref().unwrap().real_posters_allowed);
        assert_eq!(update_bundle(root.path(), "dev").unwrap_err(), Line::of(copy::R_REAL_POSTERS_OFF));
        assert_eq!(update_bundle(&root.path().join("none"), "dev").unwrap_err(), Line::of(copy::R_REAL_POSTERS_OFF));
    }

    #[test]
    fn a_folder_that_is_not_a_pack_is_never_replaced() {
        let root = tempfile::tempdir().unwrap();
        assert!(replaceable(&root.path().join("new")));
        assert!(replaceable(root.path()));
        fs::write(root.path().join("snapshot.json"), b"{}").unwrap();
        fs::create_dir(root.path().join("images")).unwrap();
        assert!(replaceable(root.path()));
        fs::write(root.path().join("notes.txt"), b"mine").unwrap();
        assert!(!replaceable(root.path()));
        assert!(!replaceable(&root.path().join("notes.txt")));
    }
}

/// The pack-building path, on a scripted list and synthetic markers. No photo of a person.
#[cfg(all(test, feature = "fixture"))]
mod pack_tests {
    use super::*;
    use crate::fixture::{self, Markers, SCENE_MODULE};
    use crate::perception::{Aligned, Detection, Timing};
    use crate::quality::Measured;
    use crate::timeutil::parse_rfc3339;
    use image::{Rgb, RgbImage};
    use serde_json::json;
    use std::collections::HashMap;

    fn now() -> SystemTime {
        parse_rfc3339("2026-10-19T10:00:00Z").unwrap()
    }

    /// The FBI, scripted: list pages by number and photos by address.
    #[derive(Default)]
    struct Fbi {
        pages: Vec<Value>,
        photos: HashMap<String, Result<Vec<u8>, FetchError>>,
        asked: Vec<(String, usize)>,
        pauses: Vec<Duration>,
        list_fails: bool,
    }

    impl Transport for Fbi {
        fn get(&mut self, url: &str, limit: usize) -> Result<Vec<u8>, FetchError> {
            assert!(fbi_url(url), "asked for {url}");
            self.asked.push((url.to_string(), limit));
            if let Some(query) = url.strip_prefix(LIST_URL) {
                if self.list_fails {
                    return Err(FetchError::Network("offline".into()));
                }
                let page: usize = query.split(['?', '&']).find_map(|kv| kv.strip_prefix("page=")).unwrap().parse().unwrap();
                return Ok(self.pages.get(page - 1).cloned().unwrap_or(json!({"items": []})).to_string().into_bytes());
            }
            let body = self.photos.get(url).cloned().unwrap_or(Err(FetchError::Status(404)))?;
            if body.len() > limit {
                return Err(FetchError::TooLarge);
            }
            Ok(body)
        }

        fn pause(&mut self, wait: Duration) {
            self.pauses.push(wait);
        }
    }

    fn png(image: &RgbImage) -> Vec<u8> {
        let mut out = Vec::new();
        image.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    /// A white photo with a demo marker per `(id, module)`, side by side.
    fn photo(markers: &[(u16, u32)]) -> Vec<u8> {
        let mut canvas = RgbImage::from_pixel(400, 200, Rgb([255, 255, 255]));
        for (i, (id, module)) in markers.iter().enumerate() {
            fixture::place(&mut canvas, &fixture::render(*id, *module), 20 + 190 * i as u32, 20);
        }
        png(&canvas)
    }

    /// A PNG header that claims 20,000 by 20,000 pixels, with no pixels.
    fn huge_png() -> Vec<u8> {
        fn crc(bytes: &[u8]) -> u32 {
            let mut c = !0u32;
            for b in bytes {
                c ^= u32::from(*b);
                for _ in 0..8 {
                    c = if c & 1 == 1 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
                }
            }
            !c
        }
        let mut chunk = b"IHDR".to_vec();
        chunk.extend(20_000u32.to_be_bytes());
        chunk.extend(20_000u32.to_be_bytes());
        chunk.extend([8, 2, 0, 0, 0]);
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        for chunk in [chunk, b"IDAT".to_vec(), b"IEND".to_vec()] {
            out.extend((chunk.len() as u32 - 4).to_be_bytes());
            out.extend(&chunk);
            out.extend(crc(&chunk).to_be_bytes());
        }
        out
    }

    fn item(uid: &str, class: &str, photo: Option<&str>) -> Value {
        let mut item = json!({"uid": uid, "title": format!("{} EXAMPLE", uid.to_uppercase()), "poster_classification": class, "url": format!("https://www.fbi.gov/wanted/{uid}")});
        if let Some(url) = photo {
            item["images"] = json!([{"original": url, "large": url, "thumb": url, "caption": null}]);
        }
        item
    }

    fn at(name: &str) -> String {
        format!("https://www.fbi.gov/wanted/{name}/@@images/image/original")
    }

    /// Two pages: posters that pass, and one for each reason a poster is left out.
    fn fbi() -> Fbi {
        let mut fbi = Fbi::default();
        let mut items = vec![
            item("m1", "Kidnappings and Missing Persons", Some(&at("m1"))),
            item("w1", "Ten Most Wanted", Some(&at("w1"))),
            item("ecap", "ECAP", Some(&at("ecap"))),
            item("nophoto", "Wanted", None),
            item("offsite", "Wanted", Some("https://images.example/offsite.jpg")),
            item("gone", "Wanted", Some(&at("gone"))),
        ];
        items.extend((0..45).map(|i| item(&format!("filler{i}"), "ECAP", None)));
        items.extend([
            item("html", "Wanted", Some(&at("html"))),
            item("huge", "Wanted", Some(&at("huge"))),
            item("big", "Wanted", Some(&at("big"))),
            item("blank", "Missing", Some(&at("blank"))),
            item("two", "Wanted", Some(&at("two"))),
            item("small", "Missing", Some(&at("small"))),
            item("w1", "Ten Most Wanted", Some(&at("w1"))),
        ]);
        let total = items.len();
        fbi.pages = items.chunks(PAGE_SIZE as usize).enumerate().map(|(i, page)| json!({"total": total, "items": page, "page": i + 1})).collect();
        for (name, body) in [
            ("m1", Ok(photo(&[(7, SCENE_MODULE)]))),
            ("w1", Ok(photo(&[(11, SCENE_MODULE)]))),
            ("ecap", Ok(photo(&[(13, SCENE_MODULE)]))),
            ("html", Ok(b"<html>not a picture</html>".to_vec())),
            ("huge", Ok(huge_png())),
            ("big", Err(FetchError::TooLarge)),
            ("blank", Ok(photo(&[]))),
            ("two", Ok(photo(&[(17, SCENE_MODULE), (19, SCENE_MODULE)]))),
            ("small", Ok(photo(&[(23, 8)]))),
        ] {
            fbi.photos.insert(at(name), body);
        }
        fbi
    }

    fn update(fbi: &mut Fbi, perception: &mut dyn Perception, dest: &Path) -> (Result<Updated, Line>, Vec<Event>) {
        let mut events = Vec::new();
        let result = update_posters(dest, "embedder-sha", fbi, perception, now(), &mut |e| events.push(e));
        (result, events)
    }

    fn skipped(done: &Updated) -> Vec<(&str, u64)> {
        done.skipped.iter().map(|(why, n)| (why.0, *n)).collect()
    }

    #[test]
    fn every_page_is_read_and_each_poster_passes_or_is_counted_with_a_reason() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("posters");
        let mut fbi = fbi();
        let (done, events) = update(&mut fbi, &mut Markers, &dest);
        let done = done.unwrap();
        assert_eq!(done.kept, 2);
        assert_eq!(
            skipped(&done),
            vec![
                ("skip_not_open", 46),
                ("skip_no_photo", 2),
                ("skip_photo_unavailable", 1),
                ("skip_not_picture", 1),
                ("skip_photo_too_large", 2),
                ("skip_no_face", 1),
                ("skip_several_faces", 1),
                ("why_small", 1),
            ]
        );
        // Every unique item is kept or counted once; the repeat on page two is not.
        assert_eq!(done.kept + done.skipped.iter().map(|(_, n)| n).sum::<u64>(), 57);
        let pack = &done.pack;
        assert_eq!((pack.count(PosterClass::Missing), pack.count(PosterClass::Wanted)), (1, 1));
        assert_eq!(pack.perception, "onnx");
        assert_eq!(pack.embedder_sha256.as_deref(), Some("embedder-sha"));
        assert_eq!(pack.created_at, "2026-10-19T10:00:00Z");
        assert_eq!(pack.expires_at, "2026-10-26T10:00:00Z");
        assert_eq!(pack.source, "api.fbi.gov/wanted/v1/list, fetched 2026-10-19T10:00:00Z");
        assert_eq!(pack.id, format!("fbi-{}", &done.list_sha256[..16]));
        // The pack loads from disk, and each photo beside it is pinned by its own hash.
        let loaded = load_pack(&dest, now()).unwrap();
        assert!(loaded.check_vectors(fixture::EMBED_DIM).is_ok());
        let m1 = loaded.posters.iter().find(|p| p.id == "m1").unwrap();
        let bytes = fs::read(dest.join(m1.image.as_ref().unwrap())).unwrap();
        assert_eq!(m1.image_sha256.as_deref(), Some(sha256_hex(&bytes).as_str()));
        assert_eq!(bytes, photo(&[(7, SCENE_MODULE)]));
        assert_eq!(m1.fbi_url.as_deref(), Some("https://www.fbi.gov/wanted/m1"));
        // Only the photos of kept posters are stored.
        assert_eq!(fs::read_dir(dest.join("images")).unwrap().count(), 2);
        // The list is read page by page with a pause, and photos are capped smaller than pages.
        let lists: Vec<_> = fbi.asked.iter().filter(|(url, _)| url.starts_with(LIST_URL)).collect();
        assert_eq!(lists.len(), 2);
        assert_eq!(lists[1].0, format!("{LIST_URL}?page=2&pageSize={PAGE_SIZE}"));
        assert!(lists.iter().all(|(_, limit)| *limit == MAX_LIST_BYTES));
        assert!(fbi.asked.iter().filter(|(url, _)| !url.starts_with(LIST_URL)).all(|(_, limit)| *limit == MAX_IMAGE_BYTES));
        assert!(!fbi.asked.iter().any(|(url, _)| url.contains("ecap") || url.contains("example")));
        assert_eq!(fbi.pauses.iter().filter(|w| **w == PAGE_PAUSE).count(), 1);
        assert_eq!(fbi.pauses.iter().filter(|w| **w == IMAGE_PAUSE).count(), 9);
        // Progress names the pages and the photos checked.
        let pages: Vec<_> = events.iter().filter_map(|e| if let Event::PostersPage { page, pages } = e { Some((*page, *pages)) } else { None }).collect();
        assert_eq!(pages, vec![(1, Some(2)), (2, Some(2))]);
        assert!(matches!(events.last(), Some(Event::PostersChecked { checked: 11, posters: 11 })));
        // The list hash is of the bodies only, in page order.
        let bodies: Vec<String> = fbi.pages.iter().map(|p| sha256_hex(p.to_string().as_bytes())).collect();
        assert_eq!(done.list_sha256, sha256_hex(bodies.join("\n").as_bytes()));
    }

    #[test]
    fn without_a_total_the_list_ends_at_a_short_page() {
        let root = tempfile::tempdir().unwrap();
        let mut fbi = fbi();
        let full: Vec<Value> = (0..PAGE_SIZE).map(|i| item(&format!("x{i}"), "ECAP", None)).collect();
        fbi.pages = vec![json!({"items": full}), json!({"items": [item("m1", "Missing", Some(&at("m1")))]}), json!({"items": [item("w1", "Wanted", Some(&at("w1")))]})];
        let (done, events) = update(&mut fbi, &mut Markers, &root.path().join("posters"));
        let done = done.unwrap();
        assert_eq!(done.kept, 1);
        assert_eq!(skipped(&done), vec![("skip_not_open", PAGE_SIZE)]);
        assert!(matches!(events[0], Event::PostersPage { page: 1, pages: None }));
    }

    #[test]
    fn a_failed_update_leaves_the_old_pack_as_it_was() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("posters");
        crate::posters::write_fixture_pack(&dest, now()).unwrap();
        let before = fs::read(dest.join("snapshot.json")).unwrap();
        let unchanged = |code: Copy| {
            assert_eq!(fs::read(dest.join("snapshot.json")).unwrap(), before, "{}", code.0);
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1, "{}: nothing left beside it", code.0);
        };
        let mut offline = Fbi { list_fails: true, ..fbi() };
        assert_eq!(update(&mut offline, &mut Markers, &dest).0.unwrap_err(), Line::of(copy::R_POSTERS_FETCH));
        unchanged(copy::R_POSTERS_FETCH);
        for bad in [json!({"error": "busy"}), json!({"total": 1_000_000, "items": []})] {
            let mut odd = Fbi { pages: vec![bad], ..fbi() };
            assert_eq!(update(&mut odd, &mut Markers, &dest).0.unwrap_err(), Line::of(copy::R_POSTERS_BAD_LIST));
            unchanged(copy::R_POSTERS_BAD_LIST);
        }
        let mut faceless = Fbi { pages: vec![json!({"total": 1, "items": [item("blank", "Missing", Some(&at("blank")))]})], ..fbi() };
        assert_eq!(update(&mut faceless, &mut Markers, &dest).0.unwrap_err(), Line::of(copy::R_POSTERS_NONE));
        unchanged(copy::R_POSTERS_NONE);
        assert_eq!(update(&mut fbi(), &mut Broken, &dest).0.unwrap_err(), Line::of(copy::R_POSTERS_RUNTIME));
        unchanged(copy::R_POSTERS_RUNTIME);
        // A good update replaces it whole.
        let done = update(&mut fbi(), &mut Markers, &dest).0.unwrap();
        assert_eq!(load_pack(&dest, now()).unwrap().id, done.pack.id);
        assert!(!dest.join("images/fixture-missing-a.png").exists());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_folder_of_other_files_is_refused_before_anything_is_fetched() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("notes.txt"), b"mine").unwrap();
        let mut fbi = fbi();
        assert_eq!(update(&mut fbi, &mut Markers, root.path()).0.unwrap_err(), Line::of(copy::R_POSTERS_FOREIGN));
        assert!(fbi.asked.is_empty());
        assert_eq!(fs::read(root.path().join("notes.txt")).unwrap(), b"mine");
    }

    /// Markers whose embedder stops.
    struct Broken;

    impl Perception for Broken {
        fn kind(&self) -> &'static str {
            Markers.kind()
        }
        fn provider(&self) -> &str {
            "fixture"
        }
        fn min_face_px(&self) -> u32 {
            Markers.min_face_px()
        }
        fn high_score(&self) -> f32 {
            Markers.high_score()
        }
        fn compare_score(&self) -> f32 {
            Markers.compare_score()
        }
        fn dim(&self) -> usize {
            Markers.dim()
        }
        fn detect(&mut self, frame: &RgbImage, det_long: u32) -> Result<Vec<Detection>, String> {
            Markers.detect(frame, det_long)
        }
        fn assess(&self, frame: &RgbImage, det: &Detection) -> Result<Measured, Reject> {
            Markers.assess(frame, det)
        }
        fn align(&mut self, frame: &RgbImage, det: &Detection, index: u64) -> Result<Option<Aligned>, String> {
            Markers.align(frame, det, index)
        }
        fn embed(&mut self, _aligned: &Aligned) -> Result<Vec<f32>, String> {
            Err("the embedder stopped".into())
        }
        fn time(&mut self, _det_long: u32) -> Option<Timing> {
            None
        }
    }
}
