//! v0.4.4 (2026-05-10 UPDATE-1) — `docs/research/delegate52-2604.15597.md`
//! survival test. Mirrors `research_doc_argus_present.rs` for the
//! companion DELEGATE-52 outcome-diffing research-anchor.

use std::path::Path;

const DOC_PATH: &str = "docs/research/delegate52-2604.15597.md";
const ARXIV_URL: &str = "https://arxiv.org/abs/2604.15597";
const COMPOSITION_ANCHOR_MARKER: &str = "Composition anchor, not a compliance claim";
const TETRAD_MARKER: &str = "plan / input / trace / output tetrad";

#[test]
fn delegate52_research_doc_exists_and_carries_arxiv_url() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(DOC_PATH);
    let body = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("{DOC_PATH} must exist as the DELEGATE-52 outcome-diff anchor: {e}");
    });
    assert!(
        body.contains(ARXIV_URL),
        "{DOC_PATH} must keep the arXiv 2604.15597 URL ({ARXIV_URL}) — \
         removing it would leave the doc unanchored to the source paper."
    );
}

#[test]
fn delegate52_research_doc_carries_composition_anchor_disclaimer() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(DOC_PATH);
    let body = std::fs::read_to_string(&path).expect("delegate52 research doc readable");
    assert!(
        body.contains(COMPOSITION_ANCHOR_MARKER),
        "{DOC_PATH} must carry the literal phrase `{COMPOSITION_ANCHOR_MARKER}` \
         — this is the standing rule against compositional-security overclaim. \
         Removing it would slide the doc into compliance-claim territory."
    );
}

#[test]
fn delegate52_research_doc_carries_tetrad_phrasing() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(DOC_PATH);
    let body = std::fs::read_to_string(&path).expect("delegate52 research doc readable");
    assert!(
        body.contains(TETRAD_MARKER),
        "{DOC_PATH} must keep the `{TETRAD_MARKER}` phrasing — this is the \
         load-bearing concept the operator recipe and the example_recalls.md \
         fixtures both anchor on."
    );
}
