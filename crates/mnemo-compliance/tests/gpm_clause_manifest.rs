//! The GPM clause manifest must keep telling the truth about this workspace.
//!
//! `docs/research/governed-persistent-memory-clauses.toml` records, for each of
//! the five Governed Persistent Memory clauses (arXiv:2608.12476), whether mnemo
//! ships it, ships a weaker form, or does not implement it. The markdown gap
//! table is generated from that manifest by
//! `scripts/gen_gpm_clause_table.py`.
//!
//! A manifest nobody checks is just prose in a different file format. These
//! tests are what make it load-bearing:
//!
//! - a `ships` / `partial` clause must point at a symbol that **exists** — so a
//!   refactor that deletes or renames the implementation cannot leave the doc
//!   claiming mnemo still ships that clause;
//! - an `absent` clause must point at **nothing** — so an implementation cannot
//!   quietly appear while the doc still says "not implemented" (the failure mode
//!   that matters least for marketing and most for honesty);
//! - the `conflicts` clause must name a shipped feature it conflicts with, and
//!   **that** symbol must exist — if `as_of` recall ever stops surfacing deleted
//!   records, the "direct conflict" claim is stale and this test says so rather
//!   than letting the page assert a conflict that no longer exists.
//!
//! This crate owns the test because it already owns the compliance story
//! (EU AI Act audit-log and DPDPA consent primitives); GPM is the same axis.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(rename = "clause")]
    clauses: Vec<Clause>,
}

#[derive(Debug, Deserialize)]
struct Clause {
    id: String,
    name: String,
    status: String,
    statement: String,
    #[serde(default)]
    implementation: Vec<Impl>,
    #[serde(default)]
    conflicts_with: Option<ConflictsWith>,
}

#[derive(Debug, Deserialize)]
struct Impl {
    path: String,
    symbol: String,
}

#[derive(Debug, Deserialize)]
struct ConflictsWith {
    feature: String,
    path: String,
    symbol: String,
}

const SHIPPED: &[&str] = &["ships", "partial"];
const NOT_SHIPPED: &[&str] = &["absent", "conflicts"];

fn repo_root() -> PathBuf {
    // crates/mnemo-compliance -> repo root
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate lives at <repo>/crates/mnemo-compliance")
        .to_path_buf()
}

fn load() -> Manifest {
    let path = repo_root().join("docs/research/governed-persistent-memory-clauses.toml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read the clause manifest at {}: {e}", path.display()));
    toml::from_str(&text).unwrap_or_else(|e| {
        panic!(
            "clause manifest at {} is not valid TOML: {e}",
            path.display()
        )
    })
}

/// Assert `symbol` appears verbatim in the file at `path`, which must exist.
fn assert_symbol_present(clause: &str, path: &str, symbol: &str) {
    let full = repo_root().join(path);
    assert!(
        full.exists(),
        "clause `{clause}` points at {path}, which does not exist. Either the \
         implementation moved (update the manifest) or the clause is no longer \
         shipped (change its status)."
    );
    let src = std::fs::read_to_string(&full)
        .unwrap_or_else(|e| panic!("clause `{clause}`: cannot read {path}: {e}"));
    assert!(
        src.contains(symbol),
        "clause `{clause}` claims `{symbol}` in {path}, but that symbol is not \
         there. A rename or removal has made the GPM gap table wrong — fix the \
         manifest, or change the clause status if the capability actually went away."
    );
}

/// A `ships` / `partial` clause must point at real code.
#[test]
fn shipped_clauses_point_at_symbols_that_exist() {
    let manifest = load();
    let mut checked = 0usize;
    for c in &manifest.clauses {
        if !SHIPPED.contains(&c.status.as_str()) {
            continue;
        }
        assert!(
            !c.implementation.is_empty(),
            "clause `{}` is status `{}` but names no implementation. A clause we \
             claim to ship must say where.",
            c.id,
            c.status
        );
        for imp in &c.implementation {
            assert_symbol_present(&c.id, &imp.path, &imp.symbol);
            checked += 1;
        }
    }
    assert!(
        checked > 0,
        "no shipped clause was checked — the manifest lost every ships/partial \
         entry, which is itself the drift this test exists to catch."
    );
}

/// An `absent` / `conflicts` clause must point at nothing. This is the direction
/// that protects honesty rather than marketing: it fails when an implementation
/// appears and the doc still says "not implemented".
#[test]
fn absent_clauses_point_at_nothing() {
    let manifest = load();
    for c in &manifest.clauses {
        if !NOT_SHIPPED.contains(&c.status.as_str()) {
            continue;
        }
        assert!(
            c.implementation.is_empty(),
            "clause `{}` is status `{}` but names {} implementation(s). If mnemo \
             now implements this clause, promote the status to `ships`/`partial` \
             and regenerate the table — do not leave the doc claiming a gap that \
             has closed.",
            c.id,
            c.status,
            c.implementation.len()
        );
    }
}

/// A `conflicts` clause must name the shipped feature it collides with, and that
/// feature must still exist. Otherwise the page asserts a live conflict that has
/// silently been resolved.
#[test]
fn conflicting_clauses_name_a_feature_that_still_ships() {
    let manifest = load();
    let mut seen = 0usize;
    for c in &manifest.clauses {
        if c.status != "conflicts" {
            assert!(
                c.conflicts_with.is_none(),
                "clause `{}` is status `{}` but declares `conflicts_with`. Only a \
                 `conflicts` clause may name a conflicting feature.",
                c.id,
                c.status
            );
            continue;
        }
        let cw = c.conflicts_with.as_ref().unwrap_or_else(|| {
            panic!(
                "clause `{}` is status `conflicts` but does not say what it \
                 conflicts with. The whole point of that status is naming the \
                 shipped feature standing in the way.",
                c.id
            )
        });
        assert!(
            !cw.feature.trim().is_empty(),
            "clause `{}`: conflicts_with.feature must be non-empty",
            c.id
        );
        assert_symbol_present(&c.id, &cw.path, &cw.symbol);
        seen += 1;
    }
    assert_eq!(
        seen, 1,
        "expected exactly one `conflicts` clause (non-revival vs `as_of` recall); \
         found {seen}. If that changed, update this expectation deliberately."
    );
}

/// The five clauses are the paper's, not ours — none may be dropped, and every
/// status must be one the generator knows how to render.
#[test]
fn manifest_covers_the_five_clauses_with_known_statuses() {
    let manifest = load();

    let ids: BTreeSet<&str> = manifest.clauses.iter().map(|c| c.id.as_str()).collect();
    let expected: BTreeSet<&str> = [
        "ledger-integrity",
        "source-binding",
        "conflict-isolation",
        "non-revival",
        "claim-closure",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        ids, expected,
        "the manifest must carry exactly GPM's five clauses. Dropping one is how \
         a gap table starts flattering the project."
    );

    for c in &manifest.clauses {
        assert!(
            SHIPPED.contains(&c.status.as_str()) || NOT_SHIPPED.contains(&c.status.as_str()),
            "clause `{}` has unknown status `{}`; expected one of ships|partial|absent|conflicts \
             (scripts/gen_gpm_clause_table.py renders exactly these)",
            c.id,
            c.status
        );
        assert!(
            !c.name.trim().is_empty() && !c.statement.trim().is_empty(),
            "clause `{}` must carry a name and a one-line statement",
            c.id
        );
    }
}
