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

/// Parse "A.B.C" (ignoring any `-pre`/`+build` suffix) into `(A, B, C)`.
/// Returns `None` unless it is exactly three numeric dot-separated components,
/// so two-part numbers (`1.31`, arXiv ids like `2605.18226`) and four-part
/// strings never masquerade as a version.
fn parse_semver(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.split(['+', '-']).next().unwrap_or(v);
    let mut it = core.split('.');
    let maj = it.next()?.parse().ok()?;
    let min = it.next()?.parse().ok()?;
    let patch = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((maj, min, patch))
}

/// Every maximal run of digits-and-dots in `text`, trimmed of surrounding dots,
/// EXCLUDING runs immediately preceded by `v`/`V`. These are *candidate*
/// literals; `parse_semver` decides which are real three-part versions. A run
/// stops at the first non-`[0-9.]` byte, so `claude-3.7-sonnet` yields `3.7`
/// (rejected: two parts) and `0.4.0-rc3` yields `0.4.0`.
///
/// The `v`-prefix exclusion is the whole reason this fence is signal, not noise.
/// The README states the *current release* as a bare number (``Current release:
/// `0.5.22` ``, `resolve 0.5.22`) but cites *feature history* with a `v` prefix
/// (`New in v0.4.0`, `wired in v0.5.20`, `As of v0.5.18`). Only the bare form is
/// a current-release claim that must equal the workspace version; the
/// `v`-prefixed citations are true statements about the past and MUST NOT be
/// rewritten just because the workspace has climbed into their patch band.
fn version_like_literals(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            let v_prefixed = start > 0 && (bytes[start - 1] == b'v' || bytes[start - 1] == b'V');
            let tok = text[start..i].trim_matches('.');
            if !tok.is_empty() && !v_prefixed {
                out.push(tok.to_string());
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Fences the README's *current patch band* against the workspace version.
///
/// The `Current release:` fence above pins ONE line, so a stale `0.5.21` could —
/// and did — sit in the surrounding prose while the heading said `0.5.22`: a
/// self-contradiction no guard caught. This pins every *bare* version literal
/// that falls in the workspace's current band (same major.minor, same tens-digit
/// of the patch — `0.5.2x` while the workspace is `0.5.22`) to the workspace
/// version exactly. Out-of-band historical references (`0.4.x`, `0.5.0`–`0.5.1x`)
/// live in a different band, and IN-band historical citations use the `v` prefix
/// (`wired in v0.5.20`) which `version_like_literals` excludes — so the README
/// can still narrate feature history by version without this fence forcing a
/// true `v0.5.20` fact to be rewritten to `v0.5.22`.
#[test]
fn readme_current_band_version_literals_match_workspace() {
    let ws = workspace_version();
    let (wmaj, wmin, wpatch) =
        parse_semver(&ws).expect("workspace [workspace.package].version must be three-part semver");
    let band = wpatch / 10;
    let readme = std::fs::read_to_string(repo_root().join("README.md")).expect("read README.md");

    let mut offenders: Vec<String> = version_like_literals(&readme)
        .into_iter()
        .filter(|lit| match parse_semver(lit) {
            Some((maj, min, patch)) => {
                maj == wmaj && min == wmin && patch / 10 == band && *lit != ws
            }
            None => false,
        })
        .collect();
    offenders.sort();
    offenders.dedup();

    assert!(
        offenders.is_empty(),
        "README carries current-band version literal(s) {offenders:?} that do not match \
         the workspace [workspace.package].version `{ws}`. A stale band literal in the \
         prose contradicts the `Current release:` heading (exactly how a `0.5.21` once sat \
         next to a `0.5.22` heading and no fence caught it). Update the literal(s) to `{ws}` \
         (or bump the workspace). Historical refs outside the `{wmaj}.{wmin}.{band}x` band \
         are intentionally not checked."
    );
}

/// Positive control: proves the fence above actually FIRES on a stale in-band
/// literal and correctly exempts `v`-prefixed history and non-versions. Without
/// this, `readme_current_band_version_literals_match_workspace` would pass
/// vacuously the day someone broke the extractor, and nobody would notice until
/// a stale literal shipped. Uses a fixed synthetic `0.5.22` workspace so it is
/// independent of the live version.
#[test]
fn band_guard_logic_is_not_vacuous() {
    // parse_semver accepts exactly three numeric parts.
    assert_eq!(parse_semver("0.5.22"), Some((0, 5, 22)));
    assert_eq!(parse_semver("0.5.0-rc3"), Some((0, 5, 0)));
    assert_eq!(parse_semver("1.31"), None); // two parts (OTel semconv)
    assert_eq!(parse_semver("2605.18226"), None); // arXiv id, two parts
    assert_eq!(parse_semver("1.2.3.4"), None); // four parts

    // Bare band literals are extracted; `v`-prefixed history is not.
    let sample = "resolve 0.5.21. wired in v0.5.20, arXiv 2605.18226, claude-3.7-sonnet.";
    let lits = version_like_literals(sample);
    assert!(
        lits.contains(&"0.5.21".to_string()),
        "bare current-band literal must be extracted: {lits:?}"
    );
    assert!(
        !lits.contains(&"0.5.20".to_string()),
        "`v`-prefixed history must be exempt from extraction: {lits:?}"
    );

    // The band comparison flags a stale in-band literal, not the workspace
    // version itself, and not an out-of-band historical one.
    let (wmaj, wmin, wpatch) = (0u64, 5u64, 22u64);
    let band = wpatch / 10;
    let flagged = |lit: &str| {
        matches!(parse_semver(lit), Some((a, b, c))
            if a == wmaj && b == wmin && c / 10 == band && lit != "0.5.22")
    };
    assert!(flagged("0.5.21"), "a stale in-band literal must be flagged");
    assert!(
        !flagged("0.5.22"),
        "the workspace version must not be flagged"
    );
    assert!(
        !flagged("0.5.18"),
        "an out-of-band historical literal must not be flagged"
    );
}
