//! README crates.io-version fence.
//!
//! The README carried a hard-coded "newest on crates.io is 0.5.16" heads-up that went
//! stale in the direction that made the project look WORSE than it is (crates.io was
//! already at 0.5.21). Nobody detected it because nothing pinned that number to a
//! source. This fence pins the README's stated current release to the workspace
//! `[workspace.package].version` — the single source — the same way
//! `docs_rmcp_version_matches_workspace.rs` pins the rmcp version and the same way
//! `scripts/check_version_drift.sh` reads the workspace version.
//!
//! Companion guard: `scripts/check_version_drift.sh` keeps the workspace in step with
//! crates.io (and now also fences mnemo-mcp-server against mnemo-core). Offline here on
//! purpose: a unit test must not depend on the network, so this fences README against
//! the workspace, and the drift script fences the workspace against crates.io. Together
//! they mean README == workspace == crates.io.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// `[workspace.package].version` from the root `Cargo.toml` (the single source).
fn workspace_version() -> String {
    let cargo = std::fs::read_to_string(repo_root().join("Cargo.toml")).expect("read Cargo.toml");
    let mut in_wp = false;
    for line in cargo.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_wp = trimmed == "[workspace.package]";
            continue;
        }
        if in_wp
            && trimmed.starts_with("version")
            && let Some(v) = trimmed.split('"').nth(1)
        {
            return v.to_string();
        }
    }
    panic!("could not find [workspace.package].version in root Cargo.toml");
}

/// The version the README states as the current crates.io release, parsed from the
/// `Current release: `X.Y.Z`` line.
fn readme_stated_version() -> String {
    let readme = std::fs::read_to_string(repo_root().join("README.md")).expect("read README.md");
    let marker = "Current release: `";
    for line in readme.lines() {
        if let Some(pos) = line.find(marker) {
            let rest = &line[pos + marker.len()..];
            if let Some(end) = rest.find('`') {
                return rest[..end].to_string();
            }
        }
    }
    panic!("README.md has no `Current release: `<version>`` line for the fence to pin");
}

#[test]
fn readme_current_release_matches_workspace_version() {
    let workspace = workspace_version();
    let readme = readme_stated_version();
    assert_eq!(
        readme, workspace,
        "README states current release `{readme}` but the workspace \
         [workspace.package].version is `{workspace}`. Update the README `Current \
         release:` line (or the workspace version) so they match. This is the fence \
         that stops the README understating the published version, the way the \
         0.5.16 heads-up did while crates.io was already at 0.5.21."
    );
}
