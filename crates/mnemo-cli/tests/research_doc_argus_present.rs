//! v0.4.4 (2026-05-09 U2) — `docs/research/argus-2605.03378.md`
//! survival test. Cheap drift guard: the research-anchor doc must
//! exist and carry both the arXiv URL and the *composition anchor*
//! disclaimer so a future operator can find the layering rationale
//! fast without re-reading the paper.

use std::path::Path;

const DOC_PATH: &str = "docs/research/argus-2605.03378.md";
const ARXIV_URL: &str = "https://arxiv.org/abs/2605.03378";
const COMPOSITION_ANCHOR_MARKER: &str = "Composition anchor, not a compliance claim";

#[test]
fn argus_research_doc_exists_and_carries_arxiv_url() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(DOC_PATH);
    let body = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("{DOC_PATH} must exist as the ARGUS research-anchor: {e}");
    });
    assert!(
        body.contains(ARXIV_URL),
        "{DOC_PATH} must keep the arXiv 2605.03378 URL ({ARXIV_URL}) — \
         removing it would leave the doc unanchored to the source paper."
    );
}

#[test]
fn argus_research_doc_carries_composition_anchor_disclaimer() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(DOC_PATH);
    let body = std::fs::read_to_string(&path).expect("argus research doc readable");
    assert!(
        body.contains(COMPOSITION_ANCHOR_MARKER),
        "{DOC_PATH} must carry the literal phrase \
         `{COMPOSITION_ANCHOR_MARKER}` — this is the standing rule \
         against compositional-security overclaim. Removing it would \
         slide the doc into compliance-claim territory."
    );
}
