// SPDX-License-Identifier: Apache-2.0

//! Frozen poster snapshots.
//!
//! The hash is the SHA-256 of the snapshot file bytes. Checksum headers are
//! not an input. A running job keeps the pack it loaded and does not re-read it.

use crate::embed::unit_embedding;
use crate::fiducial::PERCEPTION_FIDUCIAL;
use crate::timeutil::{format_rfc3339, parse_rfc3339};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const SCHEMA: &str = "openworld.poster_pack.v1";

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("missing poster pack")]
    Missing,
    #[error("poster pack hash does not match")]
    BadHash,
    #[error("poster pack is expired")]
    Expired,
    #[error("poster pack could not be read")]
    Unreadable,
}

impl PackError {
    /// The sentence a scan shows when this pack cannot be used.
    pub fn refusal(self) -> &'static str {
        match self {
            PackError::Missing => "The poster pack is missing. Refusing.",
            PackError::BadHash => "The poster pack hash does not match. Refusing.",
            PackError::Expired => "The poster pack is expired. Refusing.",
            PackError::Unreadable => "The poster pack could not be read. Refusing.",
        }
    }
}

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
    pub fbi_url: String,
    pub embedding: Option<Vec<f32>>,
    pub fiducial_id: Option<u16>,
    pub plate: Option<String>,
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
    pub posters: Vec<Poster>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PosterFile {
    schema: String,
    id: String,
    source: String,
    perception: String,
    created_at: String,
    expires_at: String,
    posters: Vec<Poster>,
}

impl PosterPack {
    pub fn enabled<'a>(&'a self, missing: bool, wanted: bool, now: SystemTime) -> Vec<&'a Poster> {
        self.posters
            .iter()
            .filter(|p| match p.class {
                PosterClass::Missing => missing,
                PosterClass::Wanted => wanted,
            })
            .filter(|p| !poster_expired(p, now))
            .collect()
    }

    pub fn any_plate<'a>(&'a self, missing: bool, wanted: bool, now: SystemTime) -> bool {
        self.enabled(missing, wanted, now)
            .iter()
            .any(|p| p.plate.as_ref().is_some_and(|s| !s.is_empty()))
    }
}

pub fn load_pack(dir: &Path, now: SystemTime) -> Result<PosterPack, PackError> {
    let snapshot = dir.join("snapshot.json");
    let digest_path = dir.join("snapshot.sha256");
    if !snapshot.is_file() || !digest_path.is_file() {
        return Err(PackError::Missing);
    }
    let bytes = fs::read(&snapshot).map_err(|_| PackError::Unreadable)?;
    let expected = fs::read_to_string(&digest_path).map_err(|_| PackError::Unreadable)?;
    let actual = sha256_hex(&bytes);
    if expected.trim() != actual {
        return Err(PackError::BadHash);
    }
    let file: PosterFile = serde_json::from_slice(&bytes).map_err(|_| PackError::Unreadable)?;
    if file.schema != SCHEMA {
        return Err(PackError::Unreadable);
    }
    let expires = parse_rfc3339(&file.expires_at).ok_or(PackError::Unreadable)?;
    if expires <= now {
        return Err(PackError::Expired);
    }
    let posters: Vec<Poster> = file
        .posters
        .into_iter()
        .filter(|p| !poster_expired(p, now))
        .collect();
    if posters.is_empty() {
        return Err(PackError::Expired);
    }
    Ok(PosterPack {
        schema: file.schema,
        id: file.id,
        source: file.source,
        perception: file.perception,
        created_at: file.created_at,
        expires_at: file.expires_at,
        body_sha256: actual,
        posters,
    })
}

pub(crate) fn write_pack(dir: &Path, file: &PosterFile) -> Result<PosterPack, PackError> {
    fs::create_dir_all(dir).map_err(|_| PackError::Unreadable)?;
    let bytes = serde_json::to_vec_pretty(file).map_err(|_| PackError::Unreadable)?;
    let hash = sha256_hex(&bytes);
    fs::write(dir.join("snapshot.json"), &bytes).map_err(|_| PackError::Unreadable)?;
    fs::write(dir.join("snapshot.sha256"), format!("{hash}\n"))
        .map_err(|_| PackError::Unreadable)?;
    let now = parse_rfc3339(&file.created_at).unwrap_or(SystemTime::UNIX_EPOCH);
    load_pack(dir, now)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let dig = hasher.finalize();
    dig.iter().map(|b| format!("{b:02x}")).collect()
}

/// Hash the response body. Any checksum header is ignored.
pub fn hash_response_body(body: &[u8], _checksum_header: Option<&str>) -> String {
    sha256_hex(body)
}

fn poster_expired(poster: &Poster, now: SystemTime) -> bool {
    poster
        .expires_at
        .as_deref()
        .and_then(parse_rfc3339)
        .is_some_and(|t| t <= now)
}

#[derive(Clone, Debug)]
pub struct FixturePoster {
    pub id: &'static str,
    pub class: PosterClass,
    pub title: &'static str,
    pub fiducial_id: Option<u16>,
    pub plate: Option<&'static str>,
}

pub fn fixture_poster_specs() -> Vec<FixturePoster> {
    vec![
        FixturePoster {
            id: "fixture-missing-a",
            class: PosterClass::Missing,
            title: "Fixture subject A",
            fiducial_id: Some(7),
            plate: None,
        },
        FixturePoster {
            id: "fixture-wanted-b",
            class: PosterClass::Wanted,
            title: "Fixture subject B",
            fiducial_id: Some(11),
            plate: None,
        },
        FixturePoster {
            id: "fixture-plate-c",
            class: PosterClass::Wanted,
            title: "Fixture vehicle C",
            fiducial_id: None,
            plate: Some("FIX123"),
        },
    ]
}

pub(crate) fn fixture_file(now: SystemTime) -> PosterFile {
    let posters = fixture_poster_specs()
        .into_iter()
        .map(|spec| Poster {
            id: spec.id.into(),
            class: spec.class,
            title: spec.title.into(),
            fbi_url: "https://www.fbi.gov/wanted".into(),
            embedding: spec.fiducial_id.map(unit_embedding),
            fiducial_id: spec.fiducial_id,
            plate: spec.plate.map(str::to_string),
            expires_at: None,
        })
        .collect();
    PosterFile {
        schema: SCHEMA.into(),
        id: "fixture-v0".into(),
        source: "fixture".into(),
        perception: PERCEPTION_FIDUCIAL.into(),
        created_at: format_rfc3339(now),
        expires_at: "2027-12-31T00:00:00Z".into(),
        posters,
    }
}

pub fn write_fixture_pack(dir: &Path, now: SystemTime) -> Result<PosterPack, PackError> {
    write_pack(dir, &fixture_file(now))
}

pub fn normalize_plate(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

pub fn snapshot_path(dir: &Path) -> PathBuf {
    dir.join("snapshot.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn hash_ignores_a_checksum_header_and_catches_an_edit() {
        let body = b"{\"items\":[]}";
        let hash = hash_response_body(body, Some("not-the-hash"));
        assert_eq!(hash, sha256_hex(body));
        let dir = tempfile::tempdir().unwrap();
        let now = parse_rfc3339("2026-10-07T00:00:00Z").unwrap();
        write_fixture_pack(dir.path(), now).unwrap();
        let loaded = load_pack(dir.path(), now).unwrap();
        assert_eq!(loaded.posters.len(), 3);
        let mut bytes = fs::read(dir.path().join("snapshot.json")).unwrap();
        bytes.push(b' ');
        fs::write(dir.path().join("snapshot.json"), bytes).unwrap();
        assert_eq!(
            load_pack(dir.path(), now).unwrap_err().refusal(),
            "The poster pack hash does not match. Refusing."
        );
        assert_eq!(
            PackError::Missing.refusal(),
            "The poster pack is missing. Refusing."
        );
        assert_eq!(
            PackError::Unreadable.refusal(),
            "The poster pack could not be read. Refusing."
        );
        assert_eq!(
            PackError::Expired.refusal(),
            "The poster pack is expired. Refusing."
        );
    }

    #[test]
    fn expired_pack_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let now = parse_rfc3339("2026-10-07T00:00:00Z").unwrap();
        let mut file = fixture_file(now);
        file.expires_at = "2020-01-01T00:00:00Z".into();
        write_pack(dir.path(), &file).unwrap_err();
        // write_pack loads at created_at, which is after expires_at, so it errors.
        // Write the bytes directly and then load.
        let bytes = serde_json::to_vec_pretty(&file).unwrap();
        fs::write(dir.path().join("snapshot.json"), &bytes).unwrap();
        fs::write(
            dir.path().join("snapshot.sha256"),
            format!("{}\n", sha256_hex(&bytes)),
        )
        .unwrap();
        assert!(matches!(
            load_pack(dir.path(), now),
            Err(PackError::Expired)
        ));
        let future = now + Duration::from_secs(1);
        let _ = future;
    }
}
