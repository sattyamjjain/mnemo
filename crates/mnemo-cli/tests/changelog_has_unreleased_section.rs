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

/// A version that has already been RELEASED must not still have a `### …`
/// sub-section sitting under `## [Unreleased]`. That is exactly how 0.5.5–0.5.18
/// piled up inside [Unreleased] for weeks while their git tags and (eventually)
/// their own sections existed. "Released" = the CHANGELOG's own `## [X.Y.Z]`
/// dated headings (self-contained, so this never goes vacuous on a tagless
/// shallow CI checkout), unioned with `git tag` when the checkout has tags.
#[test]
fn changelog_unreleased_has_no_released_version() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("CHANGELOG.md");
    let body = std::fs::read_to_string(&path).expect("CHANGELOG.md readable");

    // The [Unreleased] block: from its heading up to the next `## [` heading.
    let start = body.find("## [Unreleased]").expect("[Unreleased] required");
    let after = &body[start + "## [Unreleased]".len()..];
    let end = after.find("\n## [").unwrap_or(after.len());
    let unreleased_block = &after[..end];

    // Released versions: every `## [X.Y.Z]` dated heading (not [Unreleased]) ...
    let mut released: std::collections::BTreeSet<String> = body
        .lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("## [")?;
            let ver = rest.split(']').next()?;
            (ver != "Unreleased").then(|| ver.to_string())
        })
        .collect();
    // ... unioned with git tags if this checkout fetched them.
    if let Ok(out) = std::process::Command::new("git")
        .args(["tag", "--list", "v*"])
        .output()
    {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some(v) = line.trim().strip_prefix('v') {
                released.insert(v.to_string());
            }
        }
    }

    // Boundary-safe: `v0.5.1` must not match inside `v0.5.18`.
    fn names_version(header: &str, v: &str) -> bool {
        // `[X.Y.Z]` is self-delimited by the closing bracket.
        if header.contains(&format!("[{v}]")) {
            return true;
        }
        let pat = format!("v{v}");
        let mut from = 0;
        while let Some(pos) = header[from..].find(&pat) {
            let after_idx = from + pos + pat.len();
            let ok = header[after_idx..]
                .chars()
                .next()
                .is_none_or(|c| !(c.is_ascii_digit() || c == '.'));
            if ok {
                return true;
            }
            from = after_idx;
        }
        false
    }

    let offenders: Vec<String> = unreleased_block
        .lines()
        .filter(|l| l.starts_with("### "))
        .flat_map(|hdr| {
            released
                .iter()
                .filter(move |v| names_version(hdr, v))
                .map(move |v| format!("  {v} named in: {hdr}"))
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "released version(s) still have a `### ` sub-section under `## [Unreleased]`. \
         Move each into its own `## [<version>] — <date>` section (tags/release dates \
         are the source of truth):\n{}",
        offenders.join("\n")
    );
}
