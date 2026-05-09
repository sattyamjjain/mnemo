//! v0.4.4 (2026-05-09 U1) — README's Memory curation interop subsection
//! must keep its primary-source anchor to the 2026-05-06 Anthropic
//! Dreams Research Preview docs page and its companion comparison
//! doc. A future README rewrite that drops either anchor will fail
//! this test before it can land.

use std::path::Path;

const DREAMS_URL: &str = "https://platform.claude.com/docs/en/managed-agents/dreams";
const SUBSECTION_HEADING: &str = "Memory curation interop";
const COMPARISON_DOC_PATH: &str = "docs/comparisons/anthropic-dreams.md";
const RESEARCH_PREVIEW_DISCLAIMER: &str = "Research Preview";

fn read_readme() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("README.md");
    std::fs::read_to_string(&path).expect("README.md must be readable from repo root")
}

#[test]
fn dreams_subsection_keeps_primary_source_link() {
    let body = read_readme();
    assert!(
        body.contains(DREAMS_URL),
        "README.md must keep the Anthropic Dreams Research Preview \
         primary-source URL ({DREAMS_URL}). The Memory curation interop \
         subsection without the anchor would be marketing text, not a \
         technical pointer."
    );
}

#[test]
fn dreams_subsection_keeps_heading_word_dreams() {
    let body = read_readme();
    assert!(
        body.contains(SUBSECTION_HEADING),
        "README.md must keep the `### {SUBSECTION_HEADING}` subsection \
         (v0.4.4 2026-05-09 U1)."
    );
    // The heading line must contain the literal word "Dreams" so
    // future grep-based discovery (and TOC tooling) keeps working.
    let heading_line = body
        .lines()
        .find(|line| line.contains(SUBSECTION_HEADING))
        .expect("subsection heading line must be findable");
    assert!(
        heading_line.contains("Dreams"),
        "the `### {SUBSECTION_HEADING}` heading line must mention `Dreams` \
         literally — found instead: {heading_line:?}"
    );
}

#[test]
fn dreams_subsection_links_to_comparison_doc() {
    let body = read_readme();
    assert!(
        body.contains(COMPARISON_DOC_PATH),
        "README.md must keep its link to {COMPARISON_DOC_PATH} so \
         readers can reach the curator-action ↔ substrate-primitive \
         layering table from the Memory curation interop subsection."
    );
}

#[test]
fn dreams_paragraph_carries_research_preview_honesty_disclaimer() {
    // The Dreams API is Research Preview behind a Request-access
    // form. The README paragraph must say so explicitly — this is
    // the operator's standing rule against integration-overclaim.
    let body = read_readme();
    assert!(
        body.contains(RESEARCH_PREVIEW_DISCLAIMER),
        "README.md Memory curation interop subsection must carry the \
         literal phrase `{RESEARCH_PREVIEW_DISCLAIMER}` so readers know \
         the Dreams API is not GA. Removing this disclaimer would slide \
         the section into integration-overclaim territory."
    );
}
