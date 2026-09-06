//! README enforcement-table fence: a security control mnemo *claims* must be a
//! security control mnemo *runs*.
//!
//! `README.md` carries an "Enforced by default?" table listing every security
//! control and whether it actually executes. Some rows are `✅` with a stated
//! condition (the MCP role-filter runs only when the manifest declares a
//! `[role_filter]` block; capability leases run only when the operator passes
//! `--lease-ttl-seconds`). Two rows are an honest `❌`: the consent-token guard
//! and the mesh / deal / baseline / CMA adapters are library surfaces the running
//! server never invokes.
//!
//! That table was hand-maintained and unpinned, which is the whole problem with
//! it: nothing failed if a row stopped being true. For a memory layer aimed at
//! regulated workloads a stale `✅` is worse than an absent feature, because a
//! reader stops looking for the control they think they already have. So each
//! claim below is checked against the tree:
//!
//! * the two conditional `✅` rows must have their wiring — `mnemo-cli` must
//!   actually attach the role filter and the lease store to the served MCP
//!   server, not merely parse the config into a value it drops (which is exactly
//!   what the role filter did before v0.5.20; see
//!   `hardened_mode_attaches_role_filter.rs`);
//! * the two `❌` rows must still be unwired — if someone wires one, this test
//!   fails and the table has to be updated in the same change rather than
//!   drifting into an understatement.
//!
//! Every assertion is also guarded for **non-vacuity**: the README row being
//! pinned must be present. Deleting a row is then a build failure rather than a
//! silent way to make its check pass.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root resolves")
}

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Fails when `needle` is absent from `README.md`. Used to prove the row a check
/// is about still exists, so no check can pass by the row having been deleted.
fn readme_claims(readme: &str, needle: &str, what: &str) {
    assert!(
        readme.contains(needle),
        "README no longer contains the {what} claim this test pins (looked for {needle:?}).\n\
         If the row was intentionally reworded or removed, update this test in the same \
         change — otherwise the check below silently stops guarding anything."
    );
}

/// The MCP role-filter row claims a denied tool is rejected at `tools/call`.
/// That requires the CLI to hand the filter to the server it serves.
#[test]
fn role_filter_claim_matches_real_wiring() {
    let readme = read("README.md");
    readme_claims(&readme, "**MCP role-filter**", "MCP role-filter");
    readme_claims(&readme, "-32601", "role-filter rejection-code");

    let main = read("crates/mnemo-cli/src/main.rs");
    assert!(
        main.contains("server.with_role_filter("),
        "README claims the manifest [role_filter] is attached to the hardened MCP server, but \
         crates/mnemo-cli/src/main.rs never calls with_role_filter(). Parsing the block and \
         dropping the value is the exact defect #124 shipped with: every library-side test \
         passed while the served binary exposed every tool."
    );
}

/// The lease row claims `mnemo.forget_subject` refuses without a live lease when
/// `--lease-ttl-seconds` is set. That requires a store on the served server.
#[test]
fn lease_claim_matches_real_wiring() {
    let readme = read("README.md");
    readme_claims(
        &readme,
        "Lease tokens (capability-leased reads)",
        "lease-token",
    );
    readme_claims(&readme, "--lease-ttl-seconds", "lease TTL flag");

    let main = read("crates/mnemo-cli/src/main.rs");
    assert!(
        main.contains("server.with_lease_store("),
        "README claims capability-leased reads are shipped and opt-in, but \
         crates/mnemo-cli/src/main.rs never calls with_lease_store(), so no deployment could \
         turn them on."
    );
    assert!(
        main.contains("lease_ttl_seconds"),
        "README documents --lease-ttl-seconds but the CLI does not define it"
    );

    let server = read("crates/mnemo-mcp/src/server.rs");
    assert!(
        server.contains("LeaseScope::ForgetSubject"),
        "README claims forget_subject checks the lease, but server.rs never checks a \
         ForgetSubject-scoped lease"
    );
}

/// The consent-token row is an honest `❌`: the guard is a `mnemo-compliance`
/// type the engine never calls. `mnemo-compliance` *is* a CLI dependency (for
/// retention profiles), so the claim is specifically about the guard, not the
/// crate — and that is what this checks.
#[test]
fn consent_token_guard_is_still_library_only() {
    let readme = read("README.md");
    readme_claims(&readme, "**Consent-token-per-write**", "consent-token");
    readme_claims(
        &readme,
        "core engine never calls it",
        "consent-token ❌ wording",
    );

    let root = repo_root();
    let mut callers: Vec<String> = Vec::new();
    for entry in walk_rust_files(&root.join("crates")) {
        let rel = entry
            .strip_prefix(&root)
            .ok()
            .and_then(|p| p.to_str())
            .unwrap_or_default()
            .to_string();
        // Only shipped code counts. The claim is that the *running server* never
        // invokes the guard, so a reference from a `tests/` file — including
        // this one, which names the type in its own assertions — is not a
        // counter-example.
        if !rel.contains("/src/") {
            continue;
        }
        // The definition lives in mnemo-compliance; anywhere else is a call site.
        if rel.starts_with("crates/mnemo-compliance/") {
            continue;
        }
        if std::fs::read_to_string(&entry)
            .unwrap_or_default()
            .contains("ConsentTokenGuard")
        {
            callers.push(entry.display().to_string());
        }
    }
    assert!(
        callers.is_empty(),
        "README says the consent-token guard is library-only and the core engine never calls \
         it, but ConsentTokenGuard is referenced outside mnemo-compliance in:\n  {}\n\
         If it is now wired, that row must stop saying ❌.",
        callers.join("\n  ")
    );
}

/// The adapter row is an honest `❌`: mesh / deal / baseline / CMA are standalone
/// crates the running server does not invoke. The strongest cheap check is that
/// none of them is reachable from the binary at all.
#[test]
fn adapter_crates_are_still_not_invoked_by_the_server() {
    let readme = read("README.md");
    readme_claims(
        &readme,
        "not invoked by the running server",
        "adapter-crate ❌ wording",
    );

    let manifest = read("crates/mnemo-cli/Cargo.toml");
    let wired: Vec<&str> = ["mnemo-mesh", "mnemo-deal", "mnemo-baseline", "mnemo-cma"]
        .into_iter()
        .filter(|c| manifest.contains(&format!("\n{c} = ")))
        .collect();
    assert!(
        wired.is_empty(),
        "README says these are standalone adapters not invoked by the running server, but they \
         are now dependencies of the mnemo-cli binary: {wired:?}. Either the row is wrong or the \
         dependency is."
    );
}

fn walk_rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            // `target/` holds vendored + generated sources; it is not our claim.
            if p.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            out.extend(walk_rust_files(&p));
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
    out
}
