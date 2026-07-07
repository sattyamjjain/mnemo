//! Catches workspace-version drift in CI.
//!
//! Asserts that `mnemo-core`'s compiled `CARGO_PKG_VERSION` matches the
//! version stamped into the workspace `Cargo.toml` (since every crate
//! inherits `version.workspace = true`, this transitively asserts that
//! every published crate ships with the same version).
//!
//! Originally landed as the U1 regression test for v0.4.2; bumped each
//! cut alongside the workspace version. See
//! [docs/compat/version-skew-matrix.md](../../docs/compat/version-skew-matrix.md).

#[test]
fn cargo_pkg_version_matches_v0_5_11() {
    assert_eq!(
        env!("CARGO_PKG_VERSION"),
        "0.5.11",
        "mnemo-core CARGO_PKG_VERSION drifted from the v0.5.11 cut. \
         Bump `workspace.package.version` in /Cargo.toml AND update \
         docs/compat/version-skew-matrix.md to match."
    );
}
