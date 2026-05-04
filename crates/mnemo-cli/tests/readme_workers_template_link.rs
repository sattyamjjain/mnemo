//! v0.4.3 (A1) — README's Cloudflare Workers section must keep its
//! primary-source link to the 2026-04-30 Durable Object Facets open-beta
//! announcement. A future README rewrite that drops the anchor will
//! fail this test before it can land.

use std::path::Path;

const PRIMARY_SOURCE: &str = "https://blog.cloudflare.com/durable-object-facets-dynamic-workers/";
const SECTION_HEADING: &str = "Cloudflare Workers deploy template";
const DESIGN_NOTE_PATH: &str = "docs/src/integrations/cloudflare-workers-deploy.md";

fn read_readme() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("README.md");
    std::fs::read_to_string(&path).expect("README.md must be readable from repo root")
}

#[test]
fn workers_section_keeps_primary_source_link() {
    let body = read_readme();
    assert!(
        body.contains(PRIMARY_SOURCE),
        "README.md must keep the Cloudflare DO Facets primary-source URL ({PRIMARY_SOURCE}). \
         The Workers deploy-template section without a primary-source anchor would be a \
         marketing claim, not a technical pointer."
    );
}

#[test]
fn workers_section_has_heading_and_design_note_link() {
    let body = read_readme();
    assert!(
        body.contains(SECTION_HEADING),
        "README.md must keep the `### {SECTION_HEADING}` subsection (v0.4.3 A1)."
    );
    assert!(
        body.contains(DESIGN_NOTE_PATH),
        "README.md must keep its link to {DESIGN_NOTE_PATH} so readers can reach the design note from the Deployment section."
    );
}
