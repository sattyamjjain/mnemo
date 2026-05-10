//! v0.4.4 (2026-05-10 UPDATE-1) — `docs/tests/example_recalls.md`
//! survival test. Pins the two outcome-diff substrate fixtures
//! (primary-agent plan capture + full-tetrad reconstruction RECALL)
//! against accidental deletion or rewrite that would silently drop
//! the operator recipe described in the DELEGATE-52 research-anchor.

use std::path::Path;

const DOC_PATH: &str = "docs/tests/example_recalls.md";
const ROW1_MARKER: &str = "Fixture row 1 — primary-agent plan capture";
const ROW2_MARKER: &str = "Fixture row 2 — full-tetrad reconstruction RECALL";
const RESEARCH_LINK: &str = "../research/delegate52-2604.15597.md";

#[test]
fn example_recalls_doc_exists_and_has_both_fixture_rows() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(DOC_PATH);
    let body = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("{DOC_PATH} must exist as the outcome-diff substrate fixtures: {e}");
    });
    assert!(
        body.contains(ROW1_MARKER),
        "{DOC_PATH} must keep the `## {ROW1_MARKER}` heading — the primary-agent \
         plan-capture fixture is the entry point for the operator recipe."
    );
    assert!(
        body.contains(ROW2_MARKER),
        "{DOC_PATH} must keep the `## {ROW2_MARKER}` heading — the full-tetrad \
         reconstruction RECALL is the audit-side fixture."
    );
}

#[test]
fn example_recalls_doc_links_to_delegate52_research_anchor() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(DOC_PATH);
    let body = std::fs::read_to_string(&path).expect("example_recalls doc readable");
    assert!(
        body.contains(RESEARCH_LINK),
        "{DOC_PATH} must keep its link to {RESEARCH_LINK} so readers can reach \
         the full operator recipe + non-overlap callout from the fixtures."
    );
}
