//! v0.4.4 (A1) — README's Cloudflare H2 must keep its primary-source
//! anchor to the 2026-05-04 Project Think announcement and the
//! companion comparison doc. A future README rewrite that drops either
//! anchor will fail this test before it can land.

use std::path::Path;

const PRIMARY_SOURCE: &str = "https://blog.cloudflare.com/project-think/";
const SUBSECTION_HEADING: &str = "Project Think — loop vs. ledger";
const COMPARISON_DOC_PATH: &str = "docs/comparisons/cloudflare-project-think.md";

fn read_readme() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("README.md");
    std::fs::read_to_string(&path).expect("README.md must be readable from repo root")
}

#[test]
fn project_think_paragraph_keeps_primary_source_link() {
    let body = read_readme();
    assert!(
        body.contains(PRIMARY_SOURCE),
        "README.md must keep the Project Think primary-source URL ({PRIMARY_SOURCE}). \
         The Cloudflare H2 section without the Project Think anchor would be a \
         marketing claim, not a technical pointer."
    );
}

#[test]
fn project_think_subsection_and_comparison_doc_link_present() {
    let body = read_readme();
    assert!(
        body.contains(SUBSECTION_HEADING),
        "README.md must keep the `### {SUBSECTION_HEADING}` subsection (v0.4.4 A1)."
    );
    assert!(
        body.contains(COMPARISON_DOC_PATH),
        "README.md must keep its link to {COMPARISON_DOC_PATH} so readers can reach \
         the layering rationale from the H2 section."
    );
}
