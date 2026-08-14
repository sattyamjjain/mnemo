//! Workspace-wide version fence — every crate, the newest git tag, and the
//! CHANGELOG must agree on ONE version.
//!
//! ---------------------------------------------------------------------------
//! What went wrong
//! ---------------------------------------------------------------------------
//! Three different version numbers coexisted across this workspace at once:
//!
//!   * `[workspace.package].version`      = 0.5.23
//!   * `crates/mnemo-golem-wit`           = 0.5.21   (a LITERAL, not inherited)
//!   * newest git tag                     = v0.5.22
//!
//! `mnemo-golem-wit` drifted because it is in the root `[workspace] exclude`
//! (its cdylib WASM component cannot link for a native host target), and an
//! excluded crate cannot write `version.workspace = true` — so it carries a
//! literal that nothing was pinning. Every existing guard looked at *published*
//! crates or at the README, so a hand-maintained literal on an excluded crate
//! was invisible to all of them.
//!
//! ---------------------------------------------------------------------------
//! Why this is a test and not only a script
//! ---------------------------------------------------------------------------
//! `cargo test --workspace` runs in CI on every pull request, so encoding the
//! fence as a test is what makes a mismatch *unmergeable* rather than merely
//! reported. It is deliberately OFFLINE — a unit test must not depend on the
//! network. The online half (repo vs crates.io/npm/PyPI) lives in
//! `scripts/registry_parity.sh`. Together: Cargo.toml == tag == CHANGELOG here,
//! and repo == registry there.
//!
//! ---------------------------------------------------------------------------
//! A note on exact three-way equality
//! ---------------------------------------------------------------------------
//! Literal `Cargo.toml == newest tag == CHANGELOG section` cannot hold at all
//! times, and a fence that is red on every pre-release commit is a fence
//! everyone learns to ignore (the same trap `scripts/check_version_drift.sh`
//! documents in its header). Between a version bump and its release the repo is
//! legitimately AHEAD of the newest tag — that is exactly the state right now
//! (workspace 0.5.23, newest tag v0.5.22, `[Unreleased]` open).
//!
//! So the fence enforces the equality *and* the only other legal state:
//!
//!   RELEASE state  tag == workspace  =>  CHANGELOG must carry `## [workspace]`
//!                                        (full three-way equality asserted)
//!   WINDOW state   tag <  workspace  =>  `## [Unreleased]` must be open, and
//!                                        the newest RELEASED section must be
//!                                        <= workspace
//!   ILLEGAL        tag >  workspace  =>  the workspace regressed behind a tag
//!
//! Anything else fails by name. There is no third number anywhere.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Parse `A.B.C` (ignoring any `-pre` / `+build` suffix) into `(A, B, C)`.
fn parse_semver(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.split(['+', '-']).next().unwrap_or(v);
    let mut it = core.split('.');
    let maj = it.next()?.trim().parse().ok()?;
    let min = it.next()?.trim().parse().ok()?;
    let patch = it.next()?.trim().parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((maj, min, patch))
}

/// `[workspace.package].version` from the root `Cargo.toml` — the single source.
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

/// Every crate manifest in the repo: `crates/*/Cargo.toml`, `bench/*/Cargo.toml`,
/// and `python/Cargo.toml`. Directory reads are sorted so failures are stable.
fn crate_manifests() -> Vec<PathBuf> {
    let root = repo_root();
    let mut out = Vec::new();
    for dir in ["crates", "bench"] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path().join("Cargo.toml"))
            .filter(|p| p.is_file())
            .collect();
        paths.sort();
        out.extend(paths);
    }
    let py = root.join("python").join("Cargo.toml");
    if py.is_file() {
        out.push(py);
    }
    assert!(
        out.len() > 20,
        "expected to enumerate the whole workspace (20+ manifests), found {} — the \
         enumeration is broken, which would make this fence pass vacuously",
        out.len()
    );
    out
}

/// The `[package]` `name` and `version` lines from a manifest.
///
/// Returns `(name, Some(literal))` when the crate pins a literal version, or
/// `(name, None)` when it writes `version.workspace = true` (inherited, correct
/// by construction). Only the `[package]` table is inspected, so a `version = `
/// under `[dependencies]` can never be mistaken for the crate's own version.
fn package_name_and_literal_version(manifest: &Path) -> (String, Option<String>) {
    let text = std::fs::read_to_string(manifest)
        .unwrap_or_else(|e| panic!("read {}: {e}", manifest.display()));
    let mut in_pkg = false;
    let mut name = None;
    let mut literal = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') {
            // Stop at the first table AFTER [package]; nested tables such as
            // [package.metadata.component] are not the package table itself but
            // also never carry `name`/`version` at this level.
            in_pkg = trimmed == "[package]";
            continue;
        }
        if !in_pkg {
            continue;
        }
        if name.is_none()
            && trimmed.starts_with("name")
            && let Some(v) = trimmed.split('"').nth(1)
        {
            name = Some(v.to_string());
        }
        if literal.is_none() && trimmed.starts_with("version") {
            // `version.workspace = true` inherits; anything quoted is a literal.
            if trimmed.contains("workspace") {
                continue;
            }
            if let Some(v) = trimmed.split('"').nth(1) {
                literal = Some(v.to_string());
            }
        }
    }
    (
        name.unwrap_or_else(|| manifest.display().to_string()),
        literal,
    )
}

/// Newest `v*` tag by semver, or `None` when no tags are reachable (a shallow
/// CI checkout without `fetch-depth: 0`, or a fresh clone). Absent tags SKIP the
/// tag legs rather than failing them: a fence that fails because the checkout
/// has no tags is testing the checkout, not the repo. The dedicated
/// `version-fence` job in `ci.yml` uses `fetch-depth: 0` so the tag legs
/// actually run there.
fn newest_tag() -> Option<(String, (u64, u64, u64))> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_root())
        .args(["tag", "--list", "v*"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|t| {
            let t = t.trim();
            parse_semver(t.strip_prefix('v')?).map(|s| (t.to_string(), s))
        })
        .max_by_key(|(_, s)| *s)
}

/// Every `## [X.Y.Z]` heading in CHANGELOG.md, newest first as written.
/// `## [Unreleased]` is reported separately by `changelog_unreleased_is_open`.
fn changelog_released_sections() -> Vec<(String, (u64, u64, u64))> {
    let body =
        std::fs::read_to_string(repo_root().join("CHANGELOG.md")).expect("read CHANGELOG.md");
    body.lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("## [")?;
            let ver = rest.split(']').next()?;
            parse_semver(ver).map(|s| (ver.to_string(), s))
        })
        .collect()
}

fn changelog_unreleased_is_open() -> bool {
    std::fs::read_to_string(repo_root().join("CHANGELOG.md"))
        .expect("read CHANGELOG.md")
        .lines()
        .any(|l| l.trim().starts_with("## [Unreleased]"))
}

// ---------------------------------------------------------------------------
// The fence
// ---------------------------------------------------------------------------

/// Every crate that pins a LITERAL version must pin the workspace version.
///
/// This is the leg that catches `mnemo-golem-wit`. Crates using
/// `version.workspace = true` are correct by construction and are reported as
/// inheriting; the assertion is only about literals, because a literal is the
/// only way a second number can enter the workspace.
#[test]
fn every_crate_version_matches_the_workspace() {
    let ws = workspace_version();
    let mut offenders = Vec::new();
    let mut inherited = 0usize;
    let mut literals = 0usize;

    for manifest in crate_manifests() {
        let (name, literal) = package_name_and_literal_version(&manifest);
        match literal {
            None => inherited += 1,
            Some(v) => {
                literals += 1;
                if v != ws {
                    let rel = manifest
                        .strip_prefix(repo_root())
                        .unwrap_or(&manifest)
                        .display()
                        .to_string();
                    offenders.push(format!("{name} ({rel}) pins {v}, workspace is {ws}"));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "workspace version fence FAILED — {} crate(s) pin a version that is not the \
         workspace [workspace.package].version `{ws}`:\n  {}\n\n\
         A crate outside the workspace (root `[workspace] exclude`) cannot write \
         `version.workspace = true`, so it carries a hand-maintained literal that must be \
         bumped with the workspace. This is exactly how `mnemo-golem-wit` sat at 0.5.21 \
         while the workspace was at 0.5.23 and every other guard — which look at published \
         crates or the README — missed it. Bump the literal(s) to `{ws}`.\n\
         (checked {literals} literal + {inherited} inherited manifests)",
        offenders.len(),
        offenders.join("\n  ")
    );
}

/// Every internal `{ path = ..., version = "X" }` pin in the root
/// `[workspace.dependencies]` must name the workspace version.
///
/// These pins are what `cargo publish` uploads as the dependency requirement, so
/// a stale one ships a crate that asks the registry for a version that does not
/// exist. It is the same "second number" class as a stale literal, one table
/// over — and it was already half-wrong: the root pinned `mnemo-golem-wit` at
/// 0.5.23 while the crate itself said 0.5.21.
#[test]
fn internal_dependency_pins_match_the_workspace() {
    let ws = workspace_version();
    let cargo = std::fs::read_to_string(repo_root().join("Cargo.toml")).expect("read Cargo.toml");

    let mut offenders = Vec::new();
    let mut checked = 0usize;
    for line in cargo.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || !trimmed.starts_with("mnemo-") {
            continue;
        }
        // Only internal path deps carry both `path =` and `version =`.
        if !trimmed.contains("path") || !trimmed.contains("version") {
            continue;
        }
        let Some(name) = trimmed.split_whitespace().next() else {
            continue;
        };
        // The pin is the quoted string following `version`.
        let Some(after) = trimmed.split("version").nth(1) else {
            continue;
        };
        let Some(pin) = after.split('"').nth(1) else {
            continue;
        };
        checked += 1;
        if pin != ws {
            offenders.push(format!("{name} pinned at {pin}, workspace is {ws}"));
        }
    }

    assert!(
        checked > 5,
        "internal-pin extraction found only {checked} pins — the parser is broken, which \
         would make this fence pass vacuously"
    );
    assert!(
        offenders.is_empty(),
        "workspace version fence FAILED — {} internal dependency pin(s) in the root \
         [workspace.dependencies] do not name the workspace version `{ws}`:\n  {}\n\n\
         `cargo publish` uploads these as the dependency requirement, so a stale pin ships a \
         crate that asks crates.io for a version that was never published.",
        offenders.len(),
        offenders.join("\n  ")
    );
}

/// The newest git tag must never be AHEAD of the workspace version.
///
/// A tag ahead of `Cargo.toml` means the workspace version regressed below
/// something already released — crates.io is append-only, so that version can
/// never be re-published and the release line is broken.
#[test]
fn newest_tag_is_not_ahead_of_the_workspace() {
    let ws = workspace_version();
    let ws_sem = parse_semver(&ws).expect("workspace version must be three-part semver");
    let Some((tag, tag_sem)) = newest_tag() else {
        eprintln!("no v* tags reachable in this checkout — tag legs skipped");
        return;
    };
    assert!(
        tag_sem <= ws_sem,
        "workspace version fence FAILED — newest git tag `{tag}` is AHEAD of the workspace \
         version `{ws}`. crates.io is append-only: a version below an existing tag can never \
         be published, so the release line is broken. Bump [workspace.package].version to at \
         least {tag_sem:?}."
    );
}

/// Tag, workspace and CHANGELOG must be in exactly one of the two legal states.
///
/// RELEASE (tag == workspace): the CHANGELOG must carry a `## [<workspace>]`
/// section, so the released version is documented — full three-way equality.
///
/// WINDOW (tag < workspace): `## [Unreleased]` must be open and the newest
/// RELEASED section must not exceed the workspace, so an in-flight bump is
/// always accompanied by an open window rather than a silent third number.
#[test]
fn tag_workspace_and_changelog_agree() {
    let ws = workspace_version();
    let ws_sem = parse_semver(&ws).expect("workspace version must be three-part semver");
    let released = changelog_released_sections();
    let newest_section = released.first().cloned();

    // The newest released CHANGELOG section may never exceed the workspace.
    if let Some((sec, sec_sem)) = &newest_section {
        assert!(
            *sec_sem <= ws_sem,
            "workspace version fence FAILED — CHANGELOG's newest released section `## [{sec}]` \
             is AHEAD of the workspace version `{ws}`. The changelog documents a version the \
             workspace has not reached; bump [workspace.package].version or correct the section."
        );
    }

    let Some((tag, tag_sem)) = newest_tag() else {
        eprintln!("no v* tags reachable in this checkout — tag legs skipped");
        return;
    };

    if tag_sem == ws_sem {
        // RELEASE state — assert the third leg exactly.
        assert!(
            released.iter().any(|(_, s)| *s == ws_sem),
            "workspace version fence FAILED — tag `{tag}` and workspace `{ws}` agree (a release \
             commit), but CHANGELOG.md has no `## [{ws}]` section. Every released version must be \
             documented: promote the `[Unreleased]` entries into `## [{ws}] - <date>`. \
             (This is the same invariant the cargo-publish release gate enforces, asserted here \
             so it fails at PR time instead of at publish time.)"
        );
    } else {
        // WINDOW state — the workspace is ahead of the newest tag.
        assert!(
            changelog_unreleased_is_open(),
            "workspace version fence FAILED — workspace `{ws}` is ahead of the newest tag `{tag}` \
             (an unreleased bump), but CHANGELOG.md has no open `## [Unreleased]` section. An \
             in-flight version bump must have an open window documenting it, or the bump is a \
             third number nobody can account for."
        );
    }
}

/// Positive control: the fence's parsers must actually work, so none of the
/// assertions above can pass vacuously if an extractor breaks.
#[test]
fn fence_logic_is_not_vacuous() {
    assert_eq!(parse_semver("0.5.23"), Some((0, 5, 23)));
    assert_eq!(parse_semver("0.4.0-rc3"), Some((0, 4, 0)));
    assert_eq!(parse_semver("1.31"), None);
    assert_eq!(parse_semver("1.2.3.4"), None);

    // The literal-vs-inherited discriminator is the whole fence: prove it reads
    // a literal as a literal and an inherited version as inherited.
    let root = repo_root();
    let (wit_name, wit_ver) =
        package_name_and_literal_version(&root.join("crates/mnemo-golem-wit/Cargo.toml"));
    assert_eq!(wit_name, "mnemo-golem-wit");
    assert!(
        wit_ver.is_some(),
        "mnemo-golem-wit is outside the workspace and MUST pin a literal version; if it ever \
         gains `version.workspace = true` this fence would stop watching it (and cargo would \
         fail to find a workspace root anyway)"
    );

    let (core_name, core_ver) =
        package_name_and_literal_version(&root.join("crates/mnemo-core/Cargo.toml"));
    assert_eq!(core_name, "mnemo-core");
    assert_eq!(
        core_ver, None,
        "mnemo-core must inherit via `version.workspace = true`, not pin a literal"
    );

    // The CHANGELOG extractor must find real sections.
    assert!(
        changelog_released_sections().len() > 5,
        "CHANGELOG section extraction returned too few sections — the parser is broken"
    );
}
