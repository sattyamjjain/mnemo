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

#[test]
fn changelog_has_exactly_one_unreleased_section() {
    // A SECOND `## [Unreleased]` silently disables the ordering guard above: both it
    // and `changelog_has_unreleased_section` use `find`/`contains`, which only ever
    // inspect the FIRST heading, so a stale duplicate lower in the file passes
    // unnoticed (this repo had one wedged between `[0.4.0-rc3]` and `[0.4.0-rc1]`).
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("CHANGELOG.md");
    let body = std::fs::read_to_string(&path).expect("CHANGELOG.md readable");
    // Count HEADING lines, not raw substrings: changelog entries legitimately mention
    // `## [Unreleased]` in backticked prose (documenting these very guards), so a naive
    // `matches("## [Unreleased]")` over-counts. A heading line is exactly the token.
    let count = body
        .lines()
        .filter(|line| line.trim_end() == "## [Unreleased]")
        .count();
    assert_eq!(
        count, 1,
        "CHANGELOG.md must have exactly one `## [Unreleased]` heading, found {count}. \
         Retitle any stale duplicate to the release it actually belongs to — a second \
         `## [Unreleased]` makes the ordering guard vacuous."
    );
}
