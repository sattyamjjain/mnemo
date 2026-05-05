//! v0.4.4 (U1) — CHANGELOG.md must always carry a `## [Unreleased]`
//! heading. Cheap drift guard against accidental deletion when a
//! release-day commit forgets to re-open the next cycle's section.
//!
//! When a version cuts (e.g. v0.4.4), this test continues to pass
//! because the cut workflow renames `## [Unreleased]` to `## [0.4.4]`
//! and writes a fresh `## [Unreleased]` above it. If that pattern is
//! broken, this test catches it before the release lands.

use std::path::Path;

#[test]
fn changelog_has_unreleased_section() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("CHANGELOG.md");
    let body =
        std::fs::read_to_string(&path).expect("CHANGELOG.md must be readable from repo root");
    assert!(
        body.contains("## [Unreleased]"),
        "CHANGELOG.md must carry a `## [Unreleased]` heading at all times. \
         When cutting a release, rename the previous `## [Unreleased]` to \
         `## [<version>]` and open a fresh `## [Unreleased]` above it."
    );
}

#[test]
fn changelog_unreleased_appears_above_latest_release_heading() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("CHANGELOG.md");
    let body = std::fs::read_to_string(&path).expect("CHANGELOG.md readable");
    let unreleased_idx = body
        .find("## [Unreleased]")
        .expect("[Unreleased] heading required");
    // Find the first dated release heading (`## [<X.Y.Z>] - <YYYY-MM-DD>`).
    let release_idx = body
        .find("## [0.")
        .expect("at least one dated release heading required");
    assert!(
        unreleased_idx < release_idx,
        "`## [Unreleased]` must appear above the latest dated release heading; \
         CHANGELOG.md ordering is reversed."
    );
}
