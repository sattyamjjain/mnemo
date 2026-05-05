//! v0.4.4 (U2) — `docs/release/v0.4.3-publish-status.md` survival test.
//!
//! Cheap drift guard for the release-day audit habit: the
//! per-version publish-status doc must exist and carry the canonical
//! header so a future release-day tooling pass can locate the audit
//! ledger by string match instead of fuzzy filename guessing.

use std::path::Path;

const DOC_PATH: &str = "docs/release/v0.4.3-publish-status.md";
const REQUIRED_HEADER: &str = "Cargo workspace v0.4.3 publish status";

#[test]
fn release_status_doc_exists_and_has_canonical_header() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(DOC_PATH);
    let body = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("{DOC_PATH} must exist as the v0.4.3 publish-status audit ledger: {e}");
    });
    assert!(
        body.contains(REQUIRED_HEADER),
        "{DOC_PATH} must carry the canonical `# {REQUIRED_HEADER}` header so \
         release-day tooling can locate the audit ledger by string match."
    );
}

#[test]
fn release_status_doc_records_all_seventeen_crates() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(DOC_PATH);
    let body = std::fs::read_to_string(&path).expect("publish-status doc readable");
    for crate_name in [
        "mnemo-core",
        "mnemo-graph",
        "mnemo-mcp",
        "mnemo-postgres",
        "mnemo-rest",
        "mnemo-admin",
        "mnemo-pgwire",
        "mnemo-grpc",
        "mnemo-compliance",
        "mnemo-letta",
        "mnemo-mesh",
        "mnemo-codemode",
        "mnemo-deal",
        "mnemo-md-sync",
        "mnemo-cma",
        "mnemo-baseline",
        "mnemo-mcp-server",
    ] {
        assert!(
            body.contains(crate_name),
            "publish-status doc must list crate `{crate_name}` — drift would leave \
             the audit ledger incomplete."
        );
    }
}
