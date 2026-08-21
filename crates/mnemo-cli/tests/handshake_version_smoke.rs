//! End-to-end handshake smoke test against the shipped `mnemo` binary.
//!
//! # The defect this exists for
//!
//! Until 0.5.26, `MnemoServer` never overrode `supported_protocol_versions()`,
//! so it inherited rmcp's default of every revision the SDK knows. rmcp derives
//! `server/discover` from that list, so mnemo advertised support for
//! `2026-07-28` while answering `initialize` with `2025-11-25` and implementing
//! none of the newer revision's server-side requirements.
//!
//! `mcp_2026_07_28_conformance.rs` pins that at the library level. This file
//! pins it one layer out, against the actual binary a user installs, over real
//! stdio JSON-RPC. The distinction matters here more than usual: the bug was
//! that a *derived advertisement* disagreed with *negotiated behaviour*, and a
//! library-level assertion on one of those two cannot catch a regression in the
//! wiring between them.
//!
//! # What is asserted
//!
//! 1. The version the server advertises in the handshake is a version it will
//!    actually negotiate.
//! 2. A client asking for a revision mnemo does not implement is negotiated
//!    **down**, never echoed. Echoing is the precise failure mode: it tells the
//!    client to speak a dialect the server does not.
//! 3. A supported older revision is still honoured, so the narrowing did not
//!    become a downgrade-everything.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};

/// The revision mnemo negotiates. Kept in step with the conformance page.
const NEGOTIATED: &str = "2025-11-25";
/// A revision mnemo knows of but does not implement.
const UNIMPLEMENTED: &str = "2026-07-28";

struct Server {
    child: Child,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Run one `initialize` against the real binary and return the negotiated
/// `protocolVersion`, plus the advertised server version.
fn handshake(requested: &str) -> (String, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut child = Command::new(env!("CARGO_BIN_EXE_mnemo"))
        .env("MNEMO_DB_PATH", dir.path().join("smoke.db"))
        .env("MNEMO_AGENT_ID", "handshake-smoke")
        // Keep the parent's inherited-secret guard from tripping the run.
        .env_remove("MNEMO_CAPABILITY_KEY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the `mnemo` binary should start");

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": requested,
            "capabilities": {},
            "clientInfo": { "name": "handshake-smoke", "version": "0" }
        }
    });

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(stdin, "{request}").expect("write initialize");
        stdin.flush().expect("flush");
    }

    let stdout = child.stdout.take().expect("stdout");
    let mut line = String::new();
    BufReader::new(stdout)
        .read_line(&mut line)
        .expect("the server should answer initialize");

    let _guard = Server { child };

    let parsed: serde_json::Value =
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("bad JSON {line:?}: {e}"));
    let result = parsed
        .get("result")
        .unwrap_or_else(|| panic!("initialize returned no result: {parsed}"));
    let negotiated = result
        .get("protocolVersion")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("no protocolVersion in {result}"))
        .to_string();
    let server_version = result
        .pointer("/serverInfo/version")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    (negotiated, server_version)
}

#[test]
fn the_binary_negotiates_the_version_it_advertises() {
    let (negotiated, server_version) = handshake(NEGOTIATED);
    assert_eq!(
        negotiated, NEGOTIATED,
        "the shipped binary answered `initialize` with `{negotiated}` when the client \
         asked for `{NEGOTIATED}`. The conformance page states mnemo negotiates \
         {NEGOTIATED}; one of the two is now wrong."
    );
    assert_eq!(
        server_version,
        env!("CARGO_PKG_VERSION"),
        "the binary reports a serverInfo.version that is not its own crate version"
    );
}

/// The regression test for the actual bug.
#[test]
fn a_revision_mnemo_does_not_implement_is_negotiated_down_not_echoed() {
    let (negotiated, _) = handshake(UNIMPLEMENTED);
    assert_ne!(
        negotiated, UNIMPLEMENTED,
        "the binary echoed `{UNIMPLEMENTED}` back to a client that asked for it. That \
         is the defect fixed in 0.5.26: mnemo does not implement that revision, so \
         agreeing to speak it tells the client to use a lifecycle (no `initialize`), \
         headers and result fields the server does not support."
    );
    assert_eq!(
        negotiated, NEGOTIATED,
        "an unsupported request should fall back to the server's own revision"
    );
}

/// Narrowing the advertised set must not have become a blanket downgrade.
#[test]
fn a_supported_older_revision_is_still_honoured() {
    let (negotiated, _) = handshake("2024-11-05");
    assert_eq!(
        negotiated, "2024-11-05",
        "mnemo still lists 2024-11-05 in `supported_protocol_versions()`, so a client \
         asking for it should get it rather than being pushed to {NEGOTIATED}"
    );
}
