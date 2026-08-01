//! Build-health fence — `cargo build --workspace` must stay linkable.
//!
//! From 69a8ca6 (2026-05-21) until it was excluded, `cargo build --workspace`
//! was red on every push because `crates/mnemo-golem-wit` — a `cdylib` WASM
//! *component* whose host imports (`mnemo:golem-vector/host@…`) have no native
//! definition — was a `[workspace] members` entry. A workspace build must link
//! a native `.so`/`.dylib` for every member cdylib, and that cdylib cannot link
//! natively ("Undefined symbols … `_cabi_post_mnemo:golem-vector/…`"). Clippy
//! stayed green (it emits metadata and never links), which is exactly why the
//! regression hid in plain sight for 72 days.
//!
//! This fences the CLASS of regression, not that one crate: any `[workspace]
//! members` entry whose own `Cargo.toml` declares a `cdylib` crate-type will be
//! dragged into the default `cargo build --workspace` link and can break it.
//! Such a crate belongs in `[workspace] exclude` (built standalone, e.g. with
//! `cargo component build … --target wasm32-wasip2`), never in `members`. The
//! one legitimate exception is the PyO3 extension module, which is built with
//! `maturin` and excluded from every workspace-wide cargo command — it is
//! allow-listed below.

use std::path::{Path, PathBuf};

/// The only member allowed to declare a `cdylib`: the PyO3 extension module,
/// which is built with `maturin` (never a workspace `cargo build`) and is
/// `--exclude`d from every workspace-wide cargo command in `ci.yml`.
const CDYLIB_ALLOWLIST: &[&str] = &["mnemo-python"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root resolves")
}

/// Parse the `members = [ ... ]` array from the root `Cargo.toml`.
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

/// Read `[package] name = "..."` from a member's `Cargo.toml`.
fn package_name(member_dir: &Path) -> Option<String> {
    let body = std::fs::read_to_string(member_dir.join("Cargo.toml")).ok()?;
    let mut in_package = false;
    for line in body.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if in_package
            && let Some(rest) = t.strip_prefix("name")
            && let Some(rest) = rest.trim_start().strip_prefix('=')
        {
            return Some(rest.trim().trim_matches('"').to_string());
        }
    }
    None
}

/// True if the member's `Cargo.toml` declares a `crate-type` list containing
/// `cdylib` (in any `[lib]`/`[[example]]`/etc. table — a plain substring scan,
/// which is deliberately conservative: it errs toward flagging).
fn declares_cdylib(member_dir: &Path) -> bool {
    let Ok(body) = std::fs::read_to_string(member_dir.join("Cargo.toml")) else {
        return false;
    };
    body.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .any(|l| l.contains("crate-type") && l.contains("cdylib"))
}

#[test]
fn no_workspace_member_declares_a_cdylib() {
    let root = repo_root();

    let mut offenders: Vec<String> = Vec::new();
    for member in workspace_member_paths(&root) {
        let dir = root.join(&member);
        if !declares_cdylib(&dir) {
            continue;
        }
        let name = package_name(&dir).unwrap_or_else(|| member.clone());
        if CDYLIB_ALLOWLIST.contains(&name.as_str()) {
            continue;
        }
        offenders.push(format!("{member} (package `{name}`)"));
    }

    assert!(
        offenders.is_empty(),
        "these `[workspace] members` declare a `cdylib` crate-type and will be \
         dragged into `cargo build --workspace`'s native link — which is what \
         kept `main` red for 72 days when `mnemo-golem-wit` sat in `members`: \
         {offenders:?}. Move each into `[workspace] exclude` and build it \
         standalone (a WASM component builds with `cargo component build … \
         --target wasm32-wasip2`). Only a maturin-built PyO3 module (allow-listed \
         as {CDYLIB_ALLOWLIST:?}) may declare a cdylib while remaining a member, \
         because every workspace-wide cargo command `--exclude`s it."
    );
}

#[test]
fn cdylib_allowlist_has_no_stale_entries() {
    // Keep the allowlist an accurate ledger: every allow-listed name must still
    // be a real member that still declares a cdylib. If maturin/PyO3 ever stops
    // needing it, drop it rather than let a dead exception linger.
    let root = repo_root();
    let members = workspace_member_paths(&root);

    let mut stale: Vec<String> = Vec::new();
    for allowed in CDYLIB_ALLOWLIST {
        let matches = members.iter().any(|m| {
            let dir = root.join(m);
            declares_cdylib(&dir) && package_name(&dir).as_deref() == Some(allowed)
        });
        if !matches {
            stale.push((*allowed).to_string());
        }
    }

    assert!(
        stale.is_empty(),
        "CDYLIB_ALLOWLIST names that are no longer a cdylib-declaring member: \
         {stale:?}. Remove them so the allowlist stays honest."
    );
}
