// SPDX-License-Identifier: Apache-2.0

//! Poster update talks to api.fbi.gov only after a curve allows real photos.
//! The fixture curve does not. No image bytes are downloaded here.

use crate::posters::{hash_response_body, PosterClass};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UpdateError {
    #[error("real FBI photos stay off")]
    RealPostersOff,
    #[error("poster list could not be read")]
    BadList,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftPoster {
    pub id: String,
    pub class: PosterClass,
    pub title: String,
    pub fbi_url: String,
}

#[derive(Clone, Debug)]
pub struct DraftPack {
    pub body_sha256: String,
    pub posters: Vec<DraftPoster>,
}

pub trait Transport {
    fn get(&mut self, url: &str) -> Result<Vec<u8>, String>;
}

/// `real_posters_allowed` comes from the Fast curve. The fixture curve leaves it false.
pub fn update_posters(real_posters_allowed: bool, transport: &mut dyn Transport) -> Result<DraftPack, UpdateError> {
    if !real_posters_allowed {
        return Err(UpdateError::RealPostersOff);
    }
    let body = transport.get("https://api.fbi.gov/wanted/v1/list").map_err(|_| UpdateError::BadList)?;
    // The digest is of the body only. A Content-MD5 or similar header is not read.
    let body_sha256 = hash_response_body(&body, None);
    let posters = classify_list(&body)?;
    Ok(DraftPack { body_sha256, posters })
}

#[derive(Deserialize)]
struct ListPage {
    #[serde(default)]
    items: Vec<Value>,
}

pub fn classify_list(body: &[u8]) -> Result<Vec<DraftPoster>, UpdateError> {
    let page: ListPage = serde_json::from_slice(body).map_err(|_| UpdateError::BadList)?;
    let mut posters = Vec::new();
    for item in page.items {
        if let Some(poster) = classify_item(&item) {
            posters.push(poster);
        }
    }
    Ok(posters)
}

/// Missing and wanted are classes, not names. ECAP, unidentified remains, and
/// unnamed Seeking Information are skipped. A named Seeking Information poster
/// is kept as wanted.
pub fn classify_item(item: &Value) -> Option<DraftPoster> {
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
    Some(DraftPoster { id, class, title: title.to_string(), fbi_url })
}

fn unnamed(title_up: &str) -> bool {
    let t = title_up.trim();
    t.is_empty() || t == "SEEKING INFORMATION" || t == "UNKNOWN" || t.contains("UNIDENTIFIED") || t.contains("REMAINS")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoNet;
    impl Transport for NoNet {
        fn get(&mut self, _url: &str) -> Result<Vec<u8>, String> {
            panic!("transport must not be called while real photos are off");
        }
    }

    struct Scripted(Vec<u8>, bool);
    impl Transport for Scripted {
        fn get(&mut self, url: &str) -> Result<Vec<u8>, String> {
            assert_eq!(url, "https://api.fbi.gov/wanted/v1/list");
            self.1 = true;
            Ok(self.0.clone())
        }
    }

    #[test]
    fn fixture_curve_does_not_fetch() {
        let err = update_posters(false, &mut NoNet).unwrap_err();
        assert_eq!(err, UpdateError::RealPostersOff);
    }

    #[test]
    fn list_parser_skips_the_closed_classes() {
        let body = br#"{
            "items": [
                {"uid":"w","title":"JANE EXAMPLE","poster_classification":"Wanted","url":"https://www.fbi.gov/wanted/jane"},
                {"uid":"m","title":"JOHN EXAMPLE","poster_classification":"Missing","url":"https://www.fbi.gov/wanted/john"},
                {"uid":"e","title":"CHILD EXAMPLE","poster_classification":"ECAP","url":"https://www.fbi.gov/wanted/child"},
                {"uid":"u","title":"UNIDENTIFIED REMAINS","poster_classification":"Seeking Information","url":"https://www.fbi.gov/wanted/remains"},
                {"uid":"s","title":"SEEKING INFORMATION","poster_classification":"Seeking Information","url":"https://www.fbi.gov/wanted/seek"},
                {"uid":"n","title":"CASEY EXAMPLE","poster_classification":"Seeking Information","url":"https://www.fbi.gov/wanted/casey"}
            ]
        }"#;
        let mut transport = Scripted(body.to_vec(), false);
        let draft = update_posters(true, &mut transport).unwrap();
        assert!(transport.1);
        assert_eq!(draft.body_sha256.len(), 64);
        let ids: Vec<_> = draft.posters.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["w", "m", "n"]);
        assert_eq!(draft.posters[0].class, PosterClass::Wanted);
        assert_eq!(draft.posters[1].class, PosterClass::Missing);
        assert_eq!(draft.posters[2].class, PosterClass::Wanted);
    }
}
