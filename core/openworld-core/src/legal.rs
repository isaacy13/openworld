// SPDX-License-Identifier: Apache-2.0

//! The policies and licenses the apps show, built in, and the first-run lines.
//!
//! The texts are `docs/legal/*.md` and the generated `docs/legal/third-party.txt`
//! (`scripts/notices.py`), compiled into the library so reading them needs no network.
//! A shell shows the first-run screen until the person accepts `VERSION`, and keeps the
//! accepted version only in its own preferences.

use crate::copy::{self, Copy, Line};
use serde_json::{json, Value};

/// The version a person accepts. Change it whenever the Terms, the Privacy Policy, or the
/// Biometric Data Policy change in a way that needs acceptance again.
pub const VERSION: &str = "2026-10-19";

/// Where questions, arbitration notices, and opt-outs go. When the address moves, change it
/// here and in `docs/legal/*.md` together (docs/legal/README.md).
pub const CONTACT: &str = "support@onexengineering.com";

struct Document {
    id: &'static str,
    title: Copy,
    /// Markdown is shown as plain paragraphs; the licenses file is already plain.
    markdown: bool,
    text: &'static str,
}

const DOCUMENTS: &[Document] = &[
    Document { id: "terms", title: copy::DOC_TERMS, markdown: true, text: include_str!("../../../docs/legal/terms.md") },
    Document { id: "privacy", title: copy::DOC_PRIVACY, markdown: true, text: include_str!("../../../docs/legal/privacy.md") },
    Document { id: "biometric", title: copy::DOC_BIOMETRIC, markdown: true, text: include_str!("../../../docs/legal/biometric.md") },
    Document {
        id: "health_data",
        title: copy::DOC_HEALTH_DATA,
        markdown: true,
        text: include_str!("../../../docs/legal/washington-health-data.md"),
    },
    Document { id: "licenses", title: copy::DOC_LICENSES, markdown: false, text: include_str!("../../../docs/legal/third-party.txt") },
];

/// What the first-run screen and the About screen need, without the texts.
pub fn summary() -> Value {
    json!({
        "ok": true,
        "version": VERSION,
        "contact": CONTACT,
        "first_run": copy::FIRST_RUN.iter().map(|c| Line::of(*c)).collect::<Vec<_>>(),
        "accept": Line::of(copy::ACCEPT_TERMS),
        "changed": Line::of(copy::TERMS_CHANGED),
        "documents": DOCUMENTS.iter().map(|d| json!({ "id": d.id, "title": Line::of(d.title) })).collect::<Vec<_>>(),
    })
}

/// One document as paragraphs, for a scrolling list.
pub fn document(id: &str) -> Result<Value, Line> {
    let doc = DOCUMENTS.iter().find(|d| d.id == id).ok_or_else(|| Line::of(copy::R_NO_DOCUMENT))?;
    let paragraphs = if doc.markdown { plain(doc.text) } else { blocks(doc.text) };
    Ok(json!({ "ok": true, "id": doc.id, "title": Line::of(doc.title), "paragraphs": paragraphs }))
}

/// Blocks separated by blank lines, each kept as written. A Windows checkout has CRLF endings.
fn blocks(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n").split("\n\n").map(|b| b.trim_matches('\n').to_string()).filter(|b| !b.trim().is_empty()).collect()
}

/// The policies' Markdown as plain paragraphs: headings, emphasis, code marks, and links
/// become their text, and list items start with a bullet.
fn plain(markdown: &str) -> Vec<String> {
    blocks(markdown)
        .into_iter()
        .map(|block| block.lines().map(plain_line).collect::<Vec<_>>().join("\n"))
        .collect()
}

fn plain_line(line: &str) -> String {
    let line = line.trim_start_matches('#').trim_start();
    let line = line.strip_prefix("- ").map(|rest| format!("• {rest}")).unwrap_or_else(|| line.to_string());
    let mut out = String::with_capacity(line.len());
    let mut rest = line.replace("**", "").replace('`', "");
    // [text](target) -> text
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|i| open + i) else { break };
        let Some(end) = rest[close..].find(')').map(|i| close + i) else { break };
        out.push_str(&rest[..open]);
        out.push_str(&rest[open + 1..close]);
        rest = rest[end + 1..].to_string();
    }
    out.push_str(&rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_document_reads_and_names_the_contact() {
        let all = summary();
        for doc in all["documents"].as_array().unwrap() {
            let id = doc["id"].as_str().unwrap();
            let read = document(id).unwrap();
            let paragraphs = read["paragraphs"].as_array().unwrap();
            assert!(paragraphs.len() > 3, "{id} is nearly empty");
            if id != "licenses" {
                let text: String = paragraphs.iter().map(|p| p.as_str().unwrap()).collect();
                assert!(text.contains(CONTACT), "{id} does not name {CONTACT}");
                assert!(!text.contains("[Owner"), "{id} still has a blank to fill");
                assert!(!text.contains("**") && !text.contains("]("), "{id} still has Markdown marks");
            }
        }
        assert_eq!(document("nope").unwrap_err().code, copy::R_NO_DOCUMENT.0);
    }

    #[test]
    fn the_terms_carry_the_arbitration_opt_out_and_texas_law() {
        let terms = DOCUMENTS[0].text;
        assert!(terms.contains("State of Texas"));
        assert!(terms.contains("Opting out"));
        assert!(copy::ACCEPT_TERMS.1.contains("arbitration"));
    }

    #[test]
    fn markdown_becomes_plain_text() {
        assert_eq!(plain_line("## 3. **Where** it is"), "3. Where it is");
        assert_eq!(plain_line("- See the [Privacy Policy](privacy.md), section 2."), "• See the Privacy Policy, section 2.");
        assert_eq!(plain_line("from `api.fbi.gov` [a](b) and [c](d)"), "from api.fbi.gov a and c");
        assert_eq!(plain("# T\n\nOne\ntwo\n\n\n- a\n- b\n"), vec!["T", "One\ntwo", "• a\n• b"]);
        assert_eq!(plain("# T\r\n\r\nOne\r\ntwo\r\n"), vec!["T", "One\ntwo"]);
    }

    #[test]
    fn the_licenses_name_the_models_and_onnx_runtime() {
        let licenses = DOCUMENTS.iter().find(|d| d.id == "licenses").unwrap().text;
        assert!(licenses.contains("YuNet") && licenses.contains("SFace") && licenses.contains("ONNX Runtime"));
    }
}
