//! Regression guard for the security bug fixed in
//! `fix/mcp-role-filter-not-attached-in-hardened-mode`.
//!
//! The manifest `[role_filter]` block was parsed, validated, and built into a
//! `ManifestRoleFilter` in `crates/mnemo-cli/src/main.rs`, then **never passed to
//! `MnemoServer`** on the hardened path (`MnemoServer::new(engine)` with no
//! `.with_role_filter(...)`). So `mnemo mcp-server --manifest` — the surface
//! advertised for regulated deployments — enforced nothing: a `deny`d tool was
//! still listed and still callable.
//!
//! The library-side tests in `crates/mnemo-mcp/tests/role_filter_*.rs` all passed
//! through this bug, because the library was never broken — only the binary dropped
//! the filter. This test therefore drives the **actual `mnemo` binary** over stdio
//! (the CLI hardened path), not the library, so it fails when the wiring is missing.
//!
//! It is a *behavioural* guard: it boots the real hardened server with a manifest
//! whose `[role_filter]` denies `mnemo.forget`, then over JSON-RPC asserts the denied
//! tool is (a) absent from `tools/list` and (b) rejected by `tools/call` with
//! `-32601`. The no-filter baseline confirms the un-filtered path is unchanged.
//!
//! Verified to FAIL against the pre-fix `main.rs`: pre-fix, `mnemo.forget` is present
//! in `tools/list` and `tools/call` reaches the tool handler (returns a param error,
//! not `-32601`).

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const DENIED_TOOL: &str = "mnemo.forget";
const READ_TIMEOUT: Duration = Duration::from_secs(30);

fn write_keystore(dir: &tempfile::TempDir) -> std::path::PathBuf {
    // 32 bytes hex — the HMAC-SHA256 minimum the provenance signer requires.
    let path = dir.path().join("keystore.toml");
    std::fs::write(
        &path,
        "key_id = \"test-key\"\n\
         key_hex = \"00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff\"\n",
    )
    .unwrap();
    path
}

/// Write a manifest that boots the hardened server. `role_filter_block` is appended
/// verbatim (empty string = no `[role_filter]`, the baseline path).
fn write_manifest(dir: &tempfile::TempDir, role_filter_block: &str) -> std::path::PathBuf {
    let keystore = write_keystore(dir);
    let body = format!(
        "keystore_path = \"{}\"\n\
         audit_log_path = \"{}\"\n\
         allowed_tools = [\"mnemo.recall\", \"mnemo.verify\"]\n\
         allowed_agents = []\n\
         allowed_parents = []\n{role_filter_block}",
        keystore.display(),
        dir.path().join("audit.jsonl").display(),
    );
    let path = dir.path().join("manifest.toml");
    std::fs::write(&path, body).unwrap();
    path
}

/// Boot `mnemo mcp-server --manifest <path>` with a clean env (no leaked secrets,
/// no `MNEMO_PARENT_BASENAME` so the parent-process gauntlet passes) and piped stdio.
fn boot(dir: &tempfile::TempDir, manifest: &std::path::Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_mnemo"))
        .arg("mcp-server")
        .arg("--manifest")
        .arg(manifest)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", std::env::var("HOME").unwrap_or_default())
        .env("RUST_BACKTRACE", "0")
        .env("MNEMO_DB_PATH", dir.path().join("mnemo.db"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn mnemo binary")
}

/// Spawn a reader thread that forwards each stdout line over a channel. stdout only
/// carries JSON-RPC frames (logs go to stderr), so line-delimited parsing is safe.
fn line_reader(child: &mut Child) -> Receiver<String> {
    let stdout = child.stdout.take().expect("child stdout");
    let (tx, rx) = channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            match line {
                Ok(l) => {
                    if tx.send(l).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    rx
}

fn send(child: &mut Child, frame: &Value) {
    let stdin = child.stdin.as_mut().expect("child stdin");
    writeln!(stdin, "{frame}").expect("write json-rpc frame");
    stdin.flush().expect("flush stdin");
}

/// Read JSON-RPC lines until one carries the expected `id`, or the timeout elapses.
fn recv_id(rx: &Receiver<String>, id: i64) -> Value {
    let deadline = Instant::now() + READ_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out waiting for JSON-RPC response id={id}"
        );
        let line = rx
            .recv_timeout(remaining)
            .unwrap_or_else(|_| panic!("timed out waiting for JSON-RPC response id={id}"));
        if let Ok(v) = serde_json::from_str::<Value>(&line)
            && v.get("id").and_then(Value::as_i64) == Some(id)
        {
            return v;
        }
    }
}

/// Initialize the session and return the `tools/list` tool names.
fn handshake_and_list(child: &mut Child, rx: &Receiver<String>) -> Vec<String> {
    send(
        child,
        &json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "role-filter-test", "version": "0"}
            }
        }),
    );
    let init = recv_id(rx, 1);
    assert!(init.get("result").is_some(), "initialize failed: {init}");
    send(
        child,
        &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    );
    send(
        child,
        &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
    );
    let list = recv_id(rx, 2);
    list["result"]["tools"]
        .as_array()
        .expect("tools/list result.tools array")
        .iter()
        .map(|t| t["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

fn shutdown(mut child: Child) {
    drop(child.stdin.take());
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn hardened_server_hides_and_rejects_role_denied_tool() {
    let dir = tempfile::tempdir().unwrap();
    // caller_roles gives the stdio caller the "operator" role; deny mnemo.forget for
    // it. Every other tool falls through to allow_all, so exactly one tool is denied.
    let manifest = write_manifest(
        &dir,
        "\n[role_filter]\ncaller_roles = [\"operator\"]\ndefault = \"allow_all\"\n\n\
         [role_filter.deny]\n\"mnemo.forget\" = [\"operator\"]\n",
    );
    let mut child = boot(&dir, &manifest);
    let rx = line_reader(&mut child);

    let tools = handshake_and_list(&mut child, &rx);
    assert!(
        !tools.is_empty(),
        "expected a non-empty tool list from the booted server"
    );
    // (a) the denied tool is hidden from tools/list.
    assert!(
        !tools.contains(&DENIED_TOOL.to_string()),
        "SECURITY REGRESSION: '{DENIED_TOOL}' is denied by the manifest [role_filter] but \
         still appears in tools/list — the filter is not attached to the hardened server. \
         Visible tools: {tools:?}"
    );

    // (b) tools/call for the denied tool is rejected with -32601 (method not found).
    send(
        &mut child,
        &json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": DENIED_TOOL, "arguments": {"memory_ids": ["x"]}}
        }),
    );
    let resp = recv_id(&rx, 3);
    let code = resp["error"]["code"].as_i64();
    assert_eq!(
        code,
        Some(-32601),
        "SECURITY REGRESSION: calling denied tool '{DENIED_TOOL}' should return JSON-RPC \
         -32601 (method not found), got: {resp}"
    );
    assert!(
        resp["error"]["data"]["role_filter_reason"].is_string(),
        "expected a role_filter_reason in the -32601 error data, got: {resp}"
    );

    shutdown(child);
}

#[test]
fn no_role_filter_block_exposes_all_tools() {
    // The baseline: with no [role_filter] block, the denied tool is reachable — proving
    // it is the filter, not some unrelated gate, that removes it in the test above.
    let dir = tempfile::tempdir().unwrap();
    let manifest = write_manifest(&dir, "");
    let mut child = boot(&dir, &manifest);
    let rx = line_reader(&mut child);

    let tools = handshake_and_list(&mut child, &rx);
    assert!(
        tools.contains(&DENIED_TOOL.to_string()),
        "no [role_filter] block: expected '{DENIED_TOOL}' to be reachable (unchanged \
         behaviour), got: {tools:?}"
    );

    shutdown(child);
}
