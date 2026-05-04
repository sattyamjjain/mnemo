//! v0.4.3 (U1) — `docs/compat/version-skew-matrix.md` survival test.
//!
//! Catches accidental doc deletion or column-rename ahead of an
//! SDK-bump release. The matrix is the canonical source of truth for
//! which client-SDK versions are tested against which mnemo cut; if it
//! disappears or loses the four `mcp-*` column headers, future SDK
//! triage breaks silently. Failing the build is the right answer.

use std::path::Path;

const REQUIRED_HEADERS: &[&str] = &["mcp-python", "mcp-go", "mcp-ruby", "mcp-csharp"];

#[test]
fn skew_matrix_doc_exists_and_carries_required_columns() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("docs")
        .join("compat")
        .join("version-skew-matrix.md");
    let body = std::fs::read_to_string(&path).expect(
        "docs/compat/version-skew-matrix.md must exist — it is the canonical \
         server-vs-SDK skew reference. Restore from `main` if missing.",
    );
    let mut missing: Vec<&'static str> = Vec::new();
    for h in REQUIRED_HEADERS {
        if !body.contains(h) {
            missing.push(h);
        }
    }
    assert!(
        missing.is_empty(),
        "docs/compat/version-skew-matrix.md is missing required column header(s): \
         {missing:?}. The matrix's MCP-SDK columns are load-bearing for the \
         v0.4.3 + later client-SDK skew triage; do not remove them."
    );
}

#[test]
fn skew_matrix_doc_carries_cloudflare_substrate_annotation() {
    // v0.4.3 also splits the Cloudflare substrate row into Workers
    // KV+Vectorize vs DO Facets SQLite — the bench crate baselines
    // both. The annotation must survive future doc rewrites so the
    // mnemo-bench-cf author has a single source of truth.
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("docs")
        .join("compat")
        .join("version-skew-matrix.md");
    let body = std::fs::read_to_string(&path).expect("matrix doc readable");
    assert!(
        body.contains("DO Facets") || body.contains("Durable Object Facets"),
        "docs/compat/version-skew-matrix.md must mention the DO Facets \
         substrate annotation introduced in v0.4.3 (U1)."
    );
}
