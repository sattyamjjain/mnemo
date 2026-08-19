//! MCP-runtime version fence.
//!
//! The docs claimed the `rmcp` MCP runtime version three different wrong ways at once
//! ("rmcp 1.3" in the README, `rmcp = "1.3"` in the integration docs, "rmcp 0.14" in
//! architecture/introduction) while the workspace actually pins `rmcp = "2.2"` in the
//! root `Cargo.toml` `[workspace.dependencies]`. No test covered it.
//!
//! This fence parses the real `rmcp` version out of the root `Cargo.toml` at test time
//! (so there is no second place to drift — the same approach `scripts/check_version_drift.sh`
//! uses for `[workspace.package].version`), then scans `README.md` and `docs/src/**/*.md`
//! for every `rmcp <version>` / `rmcp = "<version>"` claim and fails, per file and line,
//! on any whose `major.minor` disagrees with the workspace.
//!
//! `ALLOWLIST` exempts genuinely historical/dated version mentions with a stated reason
//! (same idiom as `KNOWN_NON_CRATE` in `readme_crate_claims_are_real.rs`). It is empty on
//! purpose: every rmcp version claim in live docs must track the workspace dep.
//!
//! # Requirement vs resolved version
//!
//! `rmcp = "3.0"` is a caret requirement, so it *resolves* upward: today it resolves to
//! rmcp 3.1.3. A doc describing what the runtime actually does may need to name the
//! resolved version rather than the requirement, because the behaviour is patch-specific
//! (`docs/src/integrations/mcp-2026-07-28.md` turns on `ProtocolVersion::LATEST` still
//! being `2025-11-25`, which is a property of 3.1.3 and could change in 3.2).
//!
//! So a claim is accepted when it *satisfies* the requirement: same major, and minor at
//! or above the pinned minor. This is deliberately not an allowlist entry, which would
//! exempt a whole file forever, including after the workspace moved to a different major.
//! Under this rule a doc saying `rmcp 3.1.3` still fails the moment the workspace pins
//! `rmcp = "4.0"`, and a doc left at `rmcp 3.1` fails once the workspace pins `3.2` -
//! which is exactly the drift this fence exists to catch.

use std::path::{Path, PathBuf};

/// (repo-relative file, reason) — dated/historical `rmcp <ver>` mentions that are
/// intentionally NOT the current workspace version. Empty: no such text exists today.
const ALLOWLIST: &[(&str, &str)] = &[];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn major_minor(version: &str) -> String {
    let mut parts = version.split('.');
    let major = parts.next().unwrap_or_default();
    let minor = parts.next().unwrap_or("0");
    format!("{major}.{minor}")
}

/// `major.minor` of the `rmcp` dep in the root `Cargo.toml` `[workspace.dependencies]`.
fn workspace_rmcp_major_minor() -> String {
    let cargo = std::fs::read_to_string(repo_root().join("Cargo.toml")).expect("read Cargo.toml");
    for line in cargo.lines() {
        let trimmed = line.trim_start();
        // rmcp = { version = "2.2", features = [...] }
        if trimmed.starts_with("rmcp")
            && let Some(rest) = trimmed.split("version = \"").nth(1)
            && let Some(end) = rest.find('"')
        {
            return major_minor(&rest[..end]);
        }
    }
    panic!("could not find `rmcp` version in [workspace.dependencies] of root Cargo.toml");
}

/// Extract a `MAJOR.MINOR[.PATCH]` version immediately after an `rmcp` token, tolerating
/// the connector chars that appear between the token and the number (` = "`, backticks).
/// Returns `None` when `rmcp` is not followed by a version (e.g. `rmcp's`, a bare URL).
fn version_after_rmcp(rest: &str) -> Option<String> {
    let trimmed = rest.trim_start_matches([' ', '=', '"', '`', ':']);
    if !trimmed.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let end = trimmed
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(trimmed.len());
    let version = &trimmed[..end];
    // A real claim is at least MAJOR.MINOR — a lone number is not a version claim.
    if version.split('.').count() >= 2 {
        Some(version.to_string())
    } else {
        None
    }
}

/// Every rmcp version claim on a line, as `major.minor`.
fn rmcp_claims_in_line(line: &str) -> Vec<String> {
    let mut claims = Vec::new();
    let mut from = 0;
    while let Some(pos) = line[from..].find("rmcp") {
        let after = from + pos + "rmcp".len();
        if let Some(version) = version_after_rmcp(&line[after..]) {
            claims.push(major_minor(&version));
        }
        from = after;
    }
    claims
}

fn markdown_files() -> Vec<PathBuf> {
    let root = repo_root();
    let mut files = vec![root.join("README.md")];
    collect_markdown(&root.join("docs").join("src"), &mut files);
    files
}

fn collect_markdown(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_markdown(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
}

fn is_allowlisted(rel_path: &str) -> bool {
    ALLOWLIST.iter().any(|(file, _reason)| *file == rel_path)
}

/// Split a `MAJOR.MINOR` string into its numeric parts.
fn parts(major_minor: &str) -> Option<(u64, u64)> {
    let mut it = major_minor.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().unwrap_or("0").parse().ok()?;
    Some((major, minor))
}

/// Does a documented version satisfy the workspace's caret requirement?
///
/// Same major, and minor at or above the pinned minor. See the module docs for why
/// this is not plain equality.
fn satisfies_requirement(claim: &str, pinned: &str) -> bool {
    match (parts(claim), parts(pinned)) {
        (Some((cmaj, cmin)), Some((pmaj, pmin))) => cmaj == pmaj && cmin >= pmin,
        // Unparseable on either side: fall back to exact match rather than passing.
        _ => claim == pinned,
    }
}

#[test]
fn docs_rmcp_version_matches_workspace() {
    let expected = workspace_rmcp_major_minor();
    let root = repo_root();
    let mut violations = Vec::new();

    for file in markdown_files() {
        let content = std::fs::read_to_string(&file).unwrap_or_default();
        let rel = file
            .strip_prefix(&root)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        for (idx, line) in content.lines().enumerate() {
            for claim in rmcp_claims_in_line(line) {
                if !satisfies_requirement(&claim, &expected) && !is_allowlisted(&rel) {
                    violations.push(format!(
                        "{rel}:{}: claims `rmcp {claim}`, which does not satisfy the \
                         workspace requirement `rmcp {expected}` (same major, minor >= pinned)",
                        idx + 1
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "rmcp version drift — {} live surface(s) disagree with the workspace \
         [workspace.dependencies] rmcp = \"{expected}\":\n{}",
        violations.len(),
        violations.join("\n"),
    );
}

#[test]
fn workspace_rmcp_version_is_parseable() {
    // Guard the guard: if the root Cargo.toml layout changes, fail loudly here rather
    // than silently letting the scan compare against an empty/garbage version.
    let mm = workspace_rmcp_major_minor();
    assert!(
        mm.contains('.') && mm.chars().next().is_some_and(|c| c.is_ascii_digit()),
        "parsed rmcp workspace version looks wrong: {mm:?}"
    );
}

/// The satisfaction rule must stay tight in both directions. Loosening equality to
/// "satisfies the requirement" is only defensible if it still fails on real drift,
/// so the cases that must fail are asserted, not just the ones that must pass.
#[test]
fn requirement_satisfaction_still_rejects_real_drift() {
    // Accepted: the resolved version of a caret requirement.
    assert!(
        satisfies_requirement("3.1", "3.0"),
        "3.1.x resolves from `rmcp = \"3.0\"`"
    );
    assert!(
        satisfies_requirement("3.0", "3.0"),
        "the pinned version itself"
    );
    assert!(
        satisfies_requirement("3.7", "3.0"),
        "any later minor in the same major"
    );

    // Rejected: a different major line. This is the original bug the fence was
    // written for (docs saying rmcp 1.3 / 0.14 while the workspace pinned 2.2).
    assert!(
        !satisfies_requirement("2.2", "3.0"),
        "an older major must still fail"
    );
    assert!(
        !satisfies_requirement("1.3", "3.0"),
        "a much older major must still fail"
    );
    assert!(
        !satisfies_requirement("0.14", "3.0"),
        "a 0.x claim must still fail"
    );
    assert!(
        !satisfies_requirement("3.1", "4.0"),
        "a doc left behind after the workspace moves major must fail; this is what an \
         ALLOWLIST entry would have silently permitted forever"
    );

    // Rejected: a stale minor after the workspace pin advances.
    assert!(
        !satisfies_requirement("3.1", "3.2"),
        "a doc naming a minor BELOW the pin is stale and must fail"
    );

    // Unparseable input falls back to exact match rather than passing.
    assert!(!satisfies_requirement("three", "3.0"));
    assert!(satisfies_requirement("three", "three"));
}
