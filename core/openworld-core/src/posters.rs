// SPDX-License-Identifier: Apache-2.0

//! Frozen poster snapshots.
//!
//! The hash is the SHA-256 of the snapshot file bytes; a checksum header is
//! never an input. Each poster photo beside the snapshot is pinned by its own
//! SHA-256 inside it. A running job keeps the pack it loaded.
//!
//! A pack is refused when it is expired, older than seven days, dated in the
//! future, or when its vectors are not unit vectors of the embedder's length.

use crate::copy::{self, Line};
use crate::perception::{valid_vector, PERCEPTION_FIDUCIAL, PERCEPTION_ONNX};
use crate::timeutil::{format_rfc3339, parse_rfc3339};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const SCHEMA: &str = "openworld.poster_pack.v1";
/// Posters older than this are refused. Matching stays off until they are updated.
pub const MAX_AGE: Duration = Duration::from_secs(7 * 86400);
/// Posters older than this carry a warning.
pub const STALE_AGE: Duration = Duration::from_secs(86400);
/// A device clock this far behind the pack's own date is not trusted.
const CLOCK_SKEW: Duration = Duration::from_secs(3600);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PosterClass {
    Missing,
    Wanted,
}

impl PosterClass {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "missing" => Some(PosterClass::Missing),
            "wanted" => Some(PosterClass::Wanted),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PosterClass::Missing => "missing",
            PosterClass::Wanted => "wanted",
        }
    }

    /// The word on a candidate card. The stored class stays `missing` or `wanted`.
    pub fn label(self) -> &'static str {
        match self {
            PosterClass::Missing => "Missing",
            PosterClass::Wanted => "Wanted",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Poster {
    pub id: String,
    pub class: PosterClass,
    pub title: String,
    /// The poster's own FBI page. A poster without one keeps its card, without the link.
    #[serde(default)]
    pub fbi_url: Option<String>,
    pub embedding: Option<Vec<f32>>,
    /// The poster photo, relative to the pack folder, and its SHA-256.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PosterPack {
    pub schema: String,
    pub id: String,
    pub source: String,
    pub perception: String,
    pub created_at: String,
    pub expires_at: String,
    pub body_sha256: String,
    /// SHA-256 of the embedder weight that made these vectors. A network pack
    /// is only compared by that embedder.
    pub embedder_sha256: Option<String>,
    pub posters: Vec<Poster>,
    /// The pack folder, so poster photos can be shown from it.
    #[serde(skip)]
    pub dir: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PosterFile {
    schema: String,
    id: String,
    source: String,
    perception: String,
    created_at: String,
    expires_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    embedder_sha256: Option<String>,
    posters: Vec<Poster>,
}

impl PosterPack {
    pub fn enabled(&self, missing: bool, wanted: bool) -> Vec<&Poster> {
        self.posters
            .iter()
            .filter(|p| match p.class {
                PosterClass::Missing => missing,
                PosterClass::Wanted => wanted,
            })
            .collect()
    }

    pub fn count(&self, class: PosterClass) -> usize {
        self.posters.iter().filter(|p| p.class == class).count()
    }

    pub fn demo(&self) -> bool {
        self.perception == PERCEPTION_FIDUCIAL
    }

    /// Fetched more than a day ago. Still usable, with a warning.
    pub fn stale(&self, now: SystemTime) -> bool {
        parse_rfc3339(&self.created_at).and_then(|t| now.duration_since(t).ok()).is_some_and(|age| age > STALE_AGE)
    }

    /// Every vector is a unit vector of `dim` values. Checked before any cutoff is applied.
    pub fn check_vectors(&self, dim: usize) -> Result<(), Line> {
        let ok = self.posters.iter().all(|p| p.embedding.as_deref().is_some_and(|v| valid_vector(v, dim)));
        ok.then_some(()).ok_or_else(|| Line::of(copy::R_PACK_VECTORS))
    }
}

pub fn load_pack(dir: &Path, now: SystemTime) -> Result<PosterPack, Line> {
    let snapshot = dir.join("snapshot.json");
    let digest_path = dir.join("snapshot.sha256");
    if !snapshot.is_file() || !digest_path.is_file() {
        return Err(Line::of(copy::R_PACK_MISSING));
    }
    let bytes = fs::read(&snapshot).map_err(|_| Line::of(copy::R_PACK_UNREADABLE))?;
    let expected = fs::read_to_string(&digest_path).map_err(|_| Line::of(copy::R_PACK_UNREADABLE))?;
    let actual = sha256_hex(&bytes);
    if expected.trim() != actual {
        return Err(Line::of(copy::R_PACK_HASH));
    }
    let file: PosterFile = serde_json::from_slice(&bytes).map_err(|_| Line::of(copy::R_PACK_UNREADABLE))?;
    if file.schema != SCHEMA {
        return Err(Line::of(copy::R_PACK_UNREADABLE));
    }
    if file.perception != PERCEPTION_ONNX && file.perception != PERCEPTION_FIDUCIAL {
        return Err(Line::of(copy::R_PACK_PERCEPTION));
    }
    let created = parse_rfc3339(&file.created_at).ok_or_else(|| Line::of(copy::R_PACK_UNREADABLE))?;
    let expires = parse_rfc3339(&file.expires_at).ok_or_else(|| Line::of(copy::R_PACK_UNREADABLE))?;
    if created > now + CLOCK_SKEW {
        return Err(Line::of(copy::R_PACK_FUTURE));
    }
    if expires <= now {
        return Err(Line::of(copy::R_PACK_EXPIRED));
    }
    if now.duration_since(created).is_ok_and(|age| age > MAX_AGE) {
        return Err(Line::of(copy::R_PACK_OLD));
    }
    let mut posters = Vec::new();
    for mut poster in file.posters {
        if poster.expires_at.as_deref().and_then(parse_rfc3339).is_some_and(|t| t <= now) {
            continue;
        }
        // A link that is not the FBI's own page is dropped; the card stays.
        if poster.fbi_url.as_deref().is_some_and(|url| crate::scan::leave_prompt(url).is_err()) {
            poster.fbi_url = None;
        }
        check_image(dir, &poster)?;
        posters.push(poster);
    }
    if posters.is_empty() {
        return Err(Line::of(copy::R_PACK_EXPIRED));
    }
    Ok(PosterPack {
        schema: file.schema,
        id: file.id,
        source: file.source,
        perception: file.perception,
        created_at: file.created_at,
        expires_at: file.expires_at,
        body_sha256: actual,
        embedder_sha256: file.embedder_sha256,
        posters,
        dir: dir.to_path_buf(),
    })
}

/// A poster photo must sit inside the pack and match its pin.
fn check_image(dir: &Path, poster: &Poster) -> Result<(), Line> {
    let Some(name) = &poster.image else {
        return Ok(());
    };
    let inside = Path::new(name).components().all(|c| matches!(c, std::path::Component::Normal(_)));
    let bytes = if inside { fs::read(dir.join(name)).ok() } else { None };
    match (bytes, &poster.image_sha256) {
        (Some(bytes), Some(pin)) if sha256_hex(&bytes) == pin.trim() => Ok(()),
        _ => Err(Line::of(copy::R_PACK_HASH)),
    }
}

pub(crate) fn write_pack(dir: &Path, file: &PosterFile, images: &[(String, Vec<u8>)]) -> Result<PosterPack, Line> {
    let unreadable = || Line::of(copy::R_PACK_UNREADABLE);
    fs::create_dir_all(dir).map_err(|_| unreadable())?;
    for (name, bytes) in images {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|_| unreadable())?;
        }
        fs::write(path, bytes).map_err(|_| unreadable())?;
    }
    let bytes = serde_json::to_vec_pretty(file).map_err(|_| unreadable())?;
    let hash = sha256_hex(&bytes);
    fs::write(dir.join("snapshot.json"), &bytes).map_err(|_| unreadable())?;
    fs::write(dir.join("snapshot.sha256"), format!("{hash}\n")).map_err(|_| unreadable())?;
    let now = parse_rfc3339(&file.created_at).unwrap_or(SystemTime::UNIX_EPOCH);
    load_pack(dir, now)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Hash the response body. Any checksum header is ignored.
pub fn hash_response_body(body: &[u8], _checksum_header: Option<&str>) -> String {
    sha256_hex(body)
}

/// A pack of demo markers: two posters, one per class. Each poster's photo is its marker.
#[cfg(feature = "fixture")]
pub fn write_fixture_pack(dir: &Path, now: SystemTime) -> Result<PosterPack, Line> {
    let specs = [(7u16, "fixture-missing-a", PosterClass::Missing, "Demo subject A"), (11, "fixture-wanted-b", PosterClass::Wanted, "Demo subject B")];
    let mut images = Vec::new();
    let mut posters = Vec::new();
    for (marker, id, class, title) in specs {
        let mut png = Vec::new();
        crate::fixture::render(marker, crate::fixture::SCENE_MODULE)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|_| Line::of(copy::R_PACK_UNREADABLE))?;
        let name = format!("images/{id}.png");
        posters.push(Poster {
            id: id.into(),
            class,
            title: title.into(),
            fbi_url: Some("https://www.fbi.gov/wanted".into()),
            embedding: Some(crate::fixture::unit_embedding(marker)),
            image: Some(name.clone()),
            image_sha256: Some(sha256_hex(&png)),
            expires_at: None,
        });
        images.push((name, png));
    }
    let file = PosterFile {
        schema: SCHEMA.into(),
        id: "demo-v1".into(),
        source: "fixture".into(),
        perception: PERCEPTION_FIDUCIAL.into(),
        created_at: format_rfc3339(now),
        expires_at: format_rfc3339(now + MAX_AGE),
        embedder_sha256: None,
        posters,
    };
    write_pack(dir, &file, &images)
}

/// A pack of network vectors, bound to the embedder weight that made them.
/// The shipping commands do not write one. `source` names where the photos came from.
pub fn write_network_pack(
    dir: &Path,
    id: &str,
    source: &str,
    embedder_sha256: &str,
    posters: Vec<Poster>,
    created_at: SystemTime,
    expires_at: SystemTime,
) -> Result<PosterPack, Line> {
    let file = PosterFile {
        schema: SCHEMA.into(),
        id: id.into(),
        source: source.into(),
        perception: PERCEPTION_ONNX.into(),
        created_at: format_rfc3339(created_at),
        expires_at: format_rfc3339(expires_at),
        embedder_sha256: Some(embedder_sha256.into()),
        posters,
    };
    write_pack(dir, &file, &[])
}

#[cfg(all(test, feature = "fixture"))]
mod tests {
    use super::*;

    fn now() -> SystemTime {
        parse_rfc3339("2026-10-07T00:00:00Z").unwrap()
    }

    #[test]
    fn hash_ignores_a_checksum_header_and_catches_an_edit() {
        let body = b"{\"items\":[]}";
        assert_eq!(hash_response_body(body, Some("not-the-hash")), sha256_hex(body));
        let dir = tempfile::tempdir().unwrap();
        write_fixture_pack(dir.path(), now()).unwrap();
        let loaded = load_pack(dir.path(), now()).unwrap();
        assert_eq!(loaded.posters.len(), 2);
        assert!(loaded.check_vectors(crate::fixture::EMBED_DIM).is_ok());
        let mut bytes = fs::read(dir.path().join("snapshot.json")).unwrap();
        bytes.push(b' ');
        fs::write(dir.path().join("snapshot.json"), bytes).unwrap();
        assert_eq!(load_pack(dir.path(), now()).unwrap_err(), Line::of(copy::R_PACK_HASH));
        assert_eq!(load_pack(&dir.path().join("none"), now()).unwrap_err(), Line::of(copy::R_PACK_MISSING));
    }

    #[test]
    fn a_swapped_poster_photo_fails_its_pin() {
        let dir = tempfile::tempdir().unwrap();
        write_fixture_pack(dir.path(), now()).unwrap();
        fs::write(dir.path().join("images/fixture-missing-a.png"), b"another face").unwrap();
        assert_eq!(load_pack(dir.path(), now()).unwrap_err(), Line::of(copy::R_PACK_HASH));
    }

    #[test]
    fn age_expiry_and_clock_are_checked() {
        let dir = tempfile::tempdir().unwrap();
        write_fixture_pack(dir.path(), now()).unwrap();
        let day = Duration::from_secs(86400);
        assert!(!load_pack(dir.path(), now() + day / 2).unwrap().stale(now() + day / 2));
        assert!(load_pack(dir.path(), now() + day * 2).unwrap().stale(now() + day * 2));
        assert_eq!(load_pack(dir.path(), now() + day * 8).unwrap_err(), Line::of(copy::R_PACK_EXPIRED));
        assert_eq!(load_pack(dir.path(), now() - day).unwrap_err(), Line::of(copy::R_PACK_FUTURE));
    }

    #[test]
    fn scaled_or_short_vectors_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut pack = write_fixture_pack(dir.path(), now()).unwrap();
        for poster in &mut pack.posters {
            poster.embedding = poster.embedding.take().map(|v| v.into_iter().map(|x| x * 50.0).collect());
        }
        assert_eq!(pack.check_vectors(crate::fixture::EMBED_DIM).unwrap_err(), Line::of(copy::R_PACK_VECTORS));
        pack.posters[0].embedding = Some(vec![1.0]);
        assert!(pack.check_vectors(crate::fixture::EMBED_DIM).is_err());
    }

    #[test]
    fn a_link_that_is_not_the_fbi_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let mut poster = load_fixture(dir.path()).posters[0].clone();
        poster.fbi_url = Some("https://www.fbi.gov.evil.example/wanted".into());
        poster.image = None;
        poster.image_sha256 = None;
        let file = PosterFile {
            schema: SCHEMA.into(),
            id: "x".into(),
            source: "fixture".into(),
            perception: PERCEPTION_FIDUCIAL.into(),
            created_at: format_rfc3339(now()),
            expires_at: format_rfc3339(now() + Duration::from_secs(3600)),
            embedder_sha256: None,
            posters: vec![poster],
        };
        let other = tempfile::tempdir().unwrap();
        let pack = write_pack(other.path(), &file, &[]).unwrap();
        assert_eq!(pack.posters[0].fbi_url, None);
    }

    fn load_fixture(dir: &Path) -> PosterPack {
        write_fixture_pack(dir, now()).unwrap()
    }
}
