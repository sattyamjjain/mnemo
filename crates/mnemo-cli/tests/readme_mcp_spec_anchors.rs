//! README's MCP spec anchors must stay present, and current.
//!
//! # History
//!
//! This started life as `readme_mcp_roadmap_link.rs` (v0.4.4 U1, 2026-05-06),
//! pinning the README's link to the 2026-03-09 MCP 2026 Roadmap so that "a
//! future README rewrite that drops the anchor will fail this test before it can
//! land". The point was sound: an alignment claim without a primary-source link
//! is unanchored marketing text.
//!
//! It pinned the wrong thing, though. It asserted a specific *heading string*,
//! which made the anchor unfalsifiable rather than merely present: when the
//! 2026-07-28 spec release superseded the roadmap as the statement of current
//! direction, the guard's effect was to hold the README at the older anchor. A
//! test that prevents a doc from being updated to the truth has inverted its own
//! purpose.
//!
//! # What is pinned now
//!
//! The same invariant, strengthened rather than dropped:
//!
//! 1. The **current** spec revision is cited by primary source.
//! 2. The row-by-row conformance table is linked, so the claim is checkable
//!    rather than asserted.
//! 3. The roadmap link survives, because deleting history is the other way to
//!    make a claim unauditable.
//! 4. The roadmap is **subordinate** to the current spec, asserted by ordering:
//!    a reader must meet the current revision before the superseded one. This is
//!    what "kept as history rather than as current state" means mechanically.

use std::path::Path;

/// The 2026-03-09 roadmap. Kept as history, no longer the current anchor.
const ROADMAP_URL: &str = "https://blog.modelcontextprotocol.io/posts/2026-mcp-roadmap/";
/// The current spec revision.
const SPEC_2026_07_28_URL: &str =
    "https://modelcontextprotocol.io/specification/2026-07-28/changelog";
/// Row-by-row conformance, so the README's claim is checkable.
const CONFORMANCE_DOC_PATH: &str = "docs/src/integrations/mcp-2026-07-28.md";
/// The four-priority roadmap mapping, still reachable.
const ALIGNMENT_DOC_PATH: &str = "docs/src/integrations/mcp-server.md";

fn read_readme() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("README.md");
    std::fs::read_to_string(&path).expect("README.md must be readable from repo root")
}

#[test]
fn readme_cites_the_current_spec_revision() {
    let body = read_readme();
    assert!(
        body.contains(SPEC_2026_07_28_URL),
        "README.md must cite the current MCP spec revision by primary source \
         ({SPEC_2026_07_28_URL}). Without it the README's statement about what mnemo \
         implements is unanchored, which is the defect the original roadmap guard was \
         written to prevent."
    );
}

#[test]
fn readme_links_the_conformance_table() {
    let body = read_readme();
    assert!(
        body.contains(CONFORMANCE_DOC_PATH),
        "README.md must link {CONFORMANCE_DOC_PATH}. The README says which revision \
         mnemo negotiates; that table is where the claim is broken down row by row, \
         including the rows that are still open. A summary without the detail behind \
         it is the thing this repo keeps having to repair."
    );
}

#[test]
fn readme_keeps_the_roadmap_as_history() {
    let body = read_readme();
    assert!(
        body.contains(ROADMAP_URL),
        "README.md must keep the MCP 2026 Roadmap primary-source URL ({ROADMAP_URL}). \
         It is superseded, not irrelevant: mnemo's Enterprise-Readiness framing was \
         written against it, and deleting the source makes that framing unauditable."
    );
    assert!(
        body.contains(ALIGNMENT_DOC_PATH),
        "README.md must keep its link to {ALIGNMENT_DOC_PATH} so readers can still \
         reach the four-priority mapping table."
    );
}

/// The superseded anchor must not read as the current one.
#[test]
fn the_current_spec_leads_and_the_roadmap_follows() {
    let body = read_readme();
    let spec_at = body
        .find(SPEC_2026_07_28_URL)
        .expect("covered by readme_cites_the_current_spec_revision");
    let roadmap_at = body
        .find(ROADMAP_URL)
        .expect("covered by readme_keeps_the_roadmap_as_history");
    assert!(
        spec_at < roadmap_at,
        "the superseded 2026-03-09 roadmap is cited before the current 2026-07-28 spec \
         revision, so a reader meets the stale anchor first and reasonably takes it for \
         current direction. That is exactly the stale-claim defect this repo has now \
         repaired three times. Put the current revision first and keep the roadmap as \
         history below it."
    );
}
