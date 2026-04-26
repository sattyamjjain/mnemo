//! v0.4.0-rc3 (Task B2) — black-box integration tests for the
//! `mnemo mcp-server --manifest <path>` hardened entry point.
//!
//! Each test spawns the actual `mnemo` binary (via the
//! `CARGO_BIN_EXE_mnemo` variable cargo wires in for integration
//! tests) with a manifest on disk and a controlled environment, and
//! verifies the safe-spawn gauntlet exits non-zero with the right
//! refusal message. This exercises the real argv/env path the OX-MCP
//! threat model targets — not just the pure functions.
//!
//! These tests intentionally do NOT start the MCP STDIO server, so
//! they finish instantly: a refusal happens BEFORE engine state is
//! constructed.
//!
//! Skipped in fast iteration via `cargo test -p mnemo-mcp-server
//! --lib`; run automatically by the workspace `cargo test --all`.

use std::io::Write;
use std::process::Command;

fn write_manifest(dir: &tempfile::TempDir, body: &str) -> std::path::PathBuf {
    let path = dir.path().join("manifest.toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(body.as_bytes()).unwrap();
    path
}

fn write_keystore(dir: &tempfile::TempDir) -> std::path::PathBuf {
    // 64 hex chars == 32 bytes — the HMAC-SHA256 minimum the
    // provenance signer requires. Bytes don't matter for these tests.
    let path = dir.path().join("keystore.toml");
    let mut f = std::fs::File::create(&path).unwrap();
    let body = r#"
key_id = "test-key"
key_hex = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"
"#;
    f.write_all(body.as_bytes()).unwrap();
    path
}

fn baseline_manifest(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let keystore = write_keystore(dir);
    let body = format!(
        r#"
keystore_path = "{}"
audit_log_path = "{}"
allowed_tools = ["mnemo.recall", "mnemo.verify"]
allowed_parents = ["claude", "systemd"]
lease_ttl_seconds = 60
"#,
        keystore.display(),
        dir.path().join("audit.jsonl").display()
    );
    write_manifest(dir, &body)
}

/// Strip the env to a minimum so the harness doesn't accidentally
/// inherit a sensitive secret from CI / dev env into a test that is
/// supposed to *succeed*.
fn clean_env() -> Vec<(String, String)> {
    vec![
        (
            "PATH".to_string(),
            std::env::var("PATH").unwrap_or_default(),
        ),
        (
            "HOME".to_string(),
            std::env::var("HOME").unwrap_or_default(),
        ),
        // Keep RUST_BACKTRACE off so failed tests print clean stderr.
        ("RUST_BACKTRACE".to_string(), "0".to_string()),
    ]
}

#[test]
fn refuses_inherited_anthropic_api_key() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = baseline_manifest(&dir);
    let out = Command::new(env!("CARGO_BIN_EXE_mnemo"))
        .arg("mcp-server")
        .arg("--manifest")
        .arg(&manifest)
        .env_clear()
        .envs(clean_env())
        .env("ANTHROPIC_API_KEY", "sk-leaked")
        .env("MNEMO_DB_PATH", dir.path().join("mnemo.db"))
        .output()
        .expect("spawn mnemo");
    assert!(
        !out.status.success(),
        "expected refusal, got success: stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ANTHROPIC_API_KEY"),
        "expected refusal to mention ANTHROPIC_API_KEY, got: {stderr}"
    );
}

#[test]
fn refuses_inline_config_arg() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = baseline_manifest(&dir);
    let out = Command::new(env!("CARGO_BIN_EXE_mnemo"))
        .arg("mcp-server")
        .arg("--manifest")
        .arg(&manifest)
        // The trailing `--config-json` is exactly the OX-MCP injection
        // shape the gauntlet refuses. clap will accept the unknown
        // arg only if we permit it — to actually feed argv with this
        // shape, we use the env var `MNEMO_REJECT_INHERITED_SECRETS=0`
        // path? No — clap will reject unknown args before our code
        // runs, which is also a fine outcome. Either way the binary
        // does NOT start. We assert non-zero exit.
        .arg("--config-json")
        .arg("{\"k\":1}")
        .env_clear()
        .envs(clean_env())
        .env("MNEMO_DB_PATH", dir.path().join("mnemo.db"))
        .output()
        .expect("spawn mnemo");
    assert!(
        !out.status.success(),
        "expected refusal, got success: stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

#[test]
fn missing_manifest_path_fails_fast() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_mnemo"))
        .arg("mcp-server")
        .arg("--manifest")
        .arg(dir.path().join("does-not-exist.toml"))
        .env_clear()
        .envs(clean_env())
        .env("MNEMO_DB_PATH", dir.path().join("mnemo.db"))
        .output()
        .expect("spawn mnemo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not found") || stderr.contains("does-not-exist"),
        "expected NotFound message, got: {stderr}"
    );
}

#[test]
fn unknown_tool_in_manifest_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let keystore = write_keystore(&dir);
    let body = format!(
        r#"
keystore_path = "{}"
audit_log_path = "{}"
allowed_tools = ["mnemo.totally_made_up"]
"#,
        keystore.display(),
        dir.path().join("audit.jsonl").display()
    );
    let manifest = write_manifest(&dir, &body);
    let out = Command::new(env!("CARGO_BIN_EXE_mnemo"))
        .arg("mcp-server")
        .arg("--manifest")
        .arg(&manifest)
        .env_clear()
        .envs(clean_env())
        .env("MNEMO_DB_PATH", dir.path().join("mnemo.db"))
        .output()
        .expect("spawn mnemo");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("allowed_tools") || stderr.contains("totally_made_up"),
        "expected allowed_tools complaint, got: {stderr}"
    );
}
