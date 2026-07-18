//! v0.5.5 (#74) — README crate-claim fence.
//!
//! Workspace-member drift is when a `mnemo-*` crate name is written into
//! `README.md` as if it were a real, shipped crate while it has no source
//! tree and no `[workspace] members` entry (issue #74 catalogued seven such
//! names). This test is the recurrence gate: it extracts every `mnemo-*`
//! token from `README.md` and fails the build unless each one is EITHER
//!
//!   1. a real workspace member — matched against the union of every
//!      member directory's basename AND its declared `[package] name`
//!      (parsed live from the root `Cargo.toml` + each member `Cargo.toml`,
//!      so the fence tracks the workspace automatically), OR
//!   2. on the explicit `KNOWN_NON_CRATE` allowlist below — each entry is a
//!      real non-crate reference (a PyPI/npm distribution name, a JSON
//!      filename, a clearly-labelled sketch, a hypothetical in prose, or the
//!      one planned crate that the README only ever mentions in an
//!      explicitly-negated "has not been built" context).
//!
//! A brand-new phantom crate name lands in neither set and fails the build,
//! forcing the author to either wire the crate or add it to
//! `docs/roadmap/planned-crates.md` + this allowlist with a stated reason.
//! Mirrors the spirit of the AAK rule-count fence and the marketing-phrase
//! lint in `readme_no_marketing_phrases.rs`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// `mnemo-*` tokens that appear in `README.md` but are deliberately NOT
/// workspace crates. Every entry carries the reason it is legitimate.
const KNOWN_NON_CRATE: &[(&str, &str)] = &[
    // NOTE: `mnemo-db` is intentionally NOT listed here. It is now a real
    // (name-reservation pointer) workspace crate published to crates.io, so the
    // `allowlist_has_no_stale_entries` guard requires it be absent from this
    // list. The README's `pip install mnemo-db` lines still refer to the
    // separate PyPI distribution of the same name — a different registry.
    (
        "mnemo-sdk",
        "npm package short name (published as @mndfreek/mnemo-sdk)",
    ),
    (
        "mnemo-grafana",
        "filename of the bundled Grafana dashboard JSON (mnemo-grafana.json)",
    ),
    (
        "mnemo-deal-agent",
        "filename of an example agent-config JSON",
    ),
    (
        "mnemo-mcp-worker",
        "name field inside a fenced, explicitly-labelled sketch wrangler.toml",
    ),
    (
        "mnemo-dreams",
        "hypothetical primitive named only in prose, never claimed as shipped",
    ),
    (
        "mnemo-primitive",
        "hypothetical primitive named only in prose, never claimed as shipped",
    ),
    (
        "mnemo-v0",
        "token-extraction artifact from the doc filename 2026-04-25-mnemo-v0.3.4.md",
    ),
    (
        "mnemo-bench-cf",
        "planned (not built) crate; README mentions it only in explicitly-negated \
         'has not been built — it is not a workspace member' context. Tracked in \
         docs/roadmap/planned-crates.md (#74).",
    ),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root resolves")
}

/// Parse the `members = [ ... ]` array from the root `Cargo.toml` into a list
/// of member paths (e.g. `crates/mnemo-core`, `bench/locomo`, `python`).
fn workspace_member_paths(root: &Path) -> Vec<String> {
    let cargo = std::fs::read_to_string(root.join("Cargo.toml")).expect("root Cargo.toml readable");
    let start = cargo.find("members").expect("workspace has a members key");
    let open = cargo[start..].find('[').expect("members array opens") + start;
    let close = cargo[open..].find(']').expect("members array closes") + open;
    cargo[open + 1..close]
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Read the `[package] name = "..."` from a member's `Cargo.toml`.
fn package_name(member_dir: &Path) -> Option<String> {
    let body = std::fs::read_to_string(member_dir.join("Cargo.toml")).ok()?;
    for line in body.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("name") {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix('=') {
                return Some(rest.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

/// The set of identifiers that legitimately name a real workspace crate:
/// each member directory's basename plus its declared package name.
fn real_crate_identifiers(root: &Path) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for member in workspace_member_paths(root) {
        if let Some(base) = Path::new(&member).file_name().and_then(|s| s.to_str()) {
            ids.insert(base.to_string());
        }
        if let Some(name) = package_name(&root.join(&member)) {
            ids.insert(name);
        }
    }
    ids
}

/// Extract every distinct `mnemo-<[a-z0-9-]+>` token from `text`.
fn mnemo_tokens(text: &str) -> BTreeSet<String> {
    let bytes = text.as_bytes();
    let mut out = BTreeSet::new();
    let needle = b"mnemo-";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let mut j = i + needle.len();
            while j < bytes.len() {
                let c = bytes[j];
                if c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' {
                    j += 1;
                } else {
                    break;
                }
            }
            let token = text[i..j].trim_end_matches('-').to_string();
            // Require at least one char after the `mnemo-` prefix.
            if token.len() > needle.len() {
                out.insert(token);
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

#[test]
fn readme_mnemo_crate_names_are_real_members_or_allowlisted() {
    let root = repo_root();
    let readme = std::fs::read_to_string(root.join("README.md")).expect("README.md readable");

    let real = real_crate_identifiers(&root);
    let allow: BTreeSet<&str> = KNOWN_NON_CRATE.iter().map(|(name, _)| *name).collect();

    let mut violations: Vec<String> = Vec::new();
    for token in mnemo_tokens(&readme) {
        if real.contains(&token) || allow.contains(token.as_str()) {
            continue;
        }
        violations.push(token);
    }

    assert!(
        violations.is_empty(),
        "README.md references mnemo-* crate name(s) that are neither a real \
         workspace member nor an allowlisted non-crate reference: {violations:?}. \
         Either add the crate under crates/ + [workspace] members, or — if it is \
         aspirational — add it to docs/roadmap/planned-crates.md and to \
         KNOWN_NON_CRATE in this test with the reason it is not a crate. This \
         fence exists so issue #74 (workspace-member drift) cannot silently return."
    );
}

#[test]
fn allowlist_has_no_stale_entries() {
    // If an allowlisted name later becomes a real workspace member, or stops
    // appearing in the README, drop it from KNOWN_NON_CRATE so the list stays
    // an accurate ledger rather than accumulating dead exceptions.
    let root = repo_root();
    let readme = std::fs::read_to_string(root.join("README.md")).expect("README.md readable");
    let tokens = mnemo_tokens(&readme);
    let real = real_crate_identifiers(&root);

    let mut stale: Vec<String> = Vec::new();
    for (name, _reason) in KNOWN_NON_CRATE {
        if !tokens.contains(*name) {
            stale.push(format!("{name} (no longer in README)"));
        } else if real.contains(*name) {
            stale.push(format!(
                "{name} (now a real workspace member — remove the exception)"
            ));
        }
    }

    assert!(
        stale.is_empty(),
        "KNOWN_NON_CRATE has stale entries that should be removed: {stale:?}."
    );
}
