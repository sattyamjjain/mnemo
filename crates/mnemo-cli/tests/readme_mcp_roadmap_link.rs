//! v0.4.4 (U1) — README's Access Protocols section must keep its
//! spec-context anchor link to the 2026-03-09 MCP 2026 Roadmap post.
//! A future README rewrite that drops the anchor will fail this test
//! before it can land.

use std::path::Path;

const ROADMAP_URL: &str = "https://blog.modelcontextprotocol.io/posts/2026-mcp-roadmap/";
const SUBSECTION_HEADING: &str = "mnemo and the MCP 2026 Roadmap";
const ALIGNMENT_DOC_PATH: &str = "docs/src/integrations/mcp-server.md";

fn read_readme() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("README.md");
    std::fs::read_to_string(&path).expect("README.md must be readable from repo root")
}

#[test]
fn readme_keeps_mcp_roadmap_primary_source_link() {
    let body = read_readme();
    assert!(
        body.contains(ROADMAP_URL),
        "README.md must keep the MCP 2026 Roadmap primary-source URL ({ROADMAP_URL}). \
         The Access Protocols section cites this as the spec-context anchor for \
         mnemo's Enterprise-Readiness alignment claim; without the link the claim \
         is unanchored marketing text."
    );
}

#[test]
fn readme_keeps_mcp_roadmap_subsection_and_alignment_link() {
    let body = read_readme();
    assert!(
        body.contains(SUBSECTION_HEADING),
        "README.md must keep the `### {SUBSECTION_HEADING}` subsection (v0.4.4 U1)."
    );
    assert!(
        body.contains(ALIGNMENT_DOC_PATH),
        "README.md must keep its link to {ALIGNMENT_DOC_PATH} so readers can reach \
         the four-priority mapping table from the Access Protocols section."
    );
}
