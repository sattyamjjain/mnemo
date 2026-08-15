//! Per-request caller identity — the implementation of
//! [ADR 0002](../../../docs/adr/0002-request-identity-model.md).
//!
//! # The decision this implements
//!
//! ADR 0002 chose **(B) per-request identity**: every call carries its own
//! verifiable credential, and no call is authorised merely because an earlier
//! call on the same connection was. The rejected alternative — resolve a
//! principal once at handshake and cache it for the connection — is "a session
//! token wearing different clothes", and is the same *established once, then
//! assumed to still hold* shape this repo has had to repair three times
//! (`role_filter` #124, tool-catalog attestation v0.5.20, `LeaseStore` → ADR
//! 0001).
//!
//! # Why this works on stdio, with no new transport
//!
//! The original plan deferred all of this behind "an authenticated HTTP
//! transport". That turned out to be unnecessary: MCP requests carry a `_meta`
//! object, and rmcp surfaces it as [`rmcp::model::RequestMetaObject`] on
//! **every** transport including stdio. A capability can therefore ride each
//! individual request today.
//!
//! This does not make stdio multi-caller by itself — one pipe is still one
//! peer. What it does is make identity a *per-call verifiable fact* rather than
//! a boot-time assumption, which is exactly what #126's revisit gate asks for
//! ("distinct callers hold distinct identities"). The real case it serves now
//! is a gateway process that multiplexes several agents over one stdio pipe:
//! before this, every one of those agents was indistinguishable from the
//! operator.
//!
//! # Fail-closed rules
//!
//! The security property lives in what happens when verification *cannot*
//! succeed. In every such case this module returns an error rather than a
//! caller context — it never falls back to the boot identity once a capability
//! has been presented. A silent downgrade from "authenticated as principal P"
//! to "the operator" would hand a forged token the operator's authority, which
//! is strictly worse than having no capability support at all.
//!
//! | Request state | Result |
//! |---|---|
//! | no capability in `_meta` | boot-derived fallback (byte-identical to pre-ADR-0002 behaviour) |
//! | capability present, no issuer configured | **error** — cannot verify, so must not trust |
//! | capability present, malformed | **error** |
//! | capability present, bad signature / expired / unknown key | **error** |
//! | capability present and verifies | `CallerContext { caller_id: principal, roles }` |

use mnemo_core::model::capability::{Capability, CapabilityError, CapabilityIssuer};
use serde_json::Value;

use crate::role_filter::{CallerContext, RoleId};

/// `_meta` key carrying the request's capability token.
///
/// MCP asks that `_meta` keys be namespaced by a domain the author controls
/// (the spec's own keys use `io.modelcontextprotocol/...`), so this is
/// prefixed rather than a bare `capability` that could collide with another
/// server's extension.
pub const CAPABILITY_META_KEY: &str = "dev.mnemo/capability";

/// Scope tokens with this prefix become roles for the role filter.
///
/// A capability's `scope` is a space-separated token list. Tokens shaped
/// `role:<id>` contribute RBAC roles; every other token is an opaque scope
/// reserved for capability-leased reads (#126), which gates on scope strings
/// directly rather than on roles.
const ROLE_TOKEN_PREFIX: &str = "role:";

/// Why a request's identity could not be established.
///
/// Every variant is a *rejection*, never a downgrade — see the module docs.
#[derive(Debug, PartialEq, Eq)]
pub enum IdentityError {
    /// A capability was presented to a server holding no issuer key.
    ///
    /// Deliberately an error and not a fallback: a server that cannot verify a
    /// token has no basis to act on it, and treating the caller as the
    /// operator instead would grant a forged token more authority than it
    /// claimed.
    NoIssuerConfigured,
    /// The `_meta` value was present but is not a `Capability`.
    Malformed(String),
    /// The capability failed signature, expiry, or key-id verification.
    Rejected(CapabilityError),
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoIssuerConfigured => write!(
                f,
                "request carried a `{CAPABILITY_META_KEY}` capability but this server has no \
                 issuer key configured, so it cannot be verified. Start the server with a \
                 capability issuer key, or omit the capability."
            ),
            Self::Malformed(why) => write!(
                f,
                "`{CAPABILITY_META_KEY}` in request _meta is not a valid capability: {why}"
            ),
            Self::Rejected(err) => write!(f, "capability rejected: {err}"),
        }
    }
}

impl std::error::Error for IdentityError {}

impl From<CapabilityError> for IdentityError {
    fn from(err: CapabilityError) -> Self {
        Self::Rejected(err)
    }
}

/// Split a capability scope into RBAC roles.
///
/// Only `role:`-prefixed tokens become roles. Returns them in scope order with
/// duplicates preserved — the role filter builds its own set, and rewriting the
/// operator's ordering here would make the audit trail disagree with the token.
pub fn roles_from_scope(scope: &str) -> Vec<RoleId> {
    scope
        .split_whitespace()
        .filter_map(|tok| tok.strip_prefix(ROLE_TOKEN_PREFIX))
        .filter(|role| !role.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Non-role scope tokens, in scope order.
///
/// These are what #126's leases gate on. Exposed now so Stage C does not have
/// to re-parse the scope with a second, possibly divergent, tokeniser.
pub fn scopes_from_scope(scope: &str) -> Vec<String> {
    scope
        .split_whitespace()
        .filter(|tok| !tok.starts_with(ROLE_TOKEN_PREFIX))
        .map(str::to_owned)
        .collect()
}

/// Resolve the caller for one request.
///
/// `meta_value` is the `_meta[CAPABILITY_META_KEY]` entry, if the request had
/// one. `fallback_agent_id` is the boot-derived agent id used only when no
/// capability was presented at all.
///
/// See the module docs for the full fail-closed table.
pub fn resolve_caller(
    meta_value: Option<&Value>,
    issuer: Option<&CapabilityIssuer>,
    fallback_agent_id: &str,
) -> Result<CallerContext, IdentityError> {
    let Some(raw) = meta_value else {
        // No capability presented: the pre-ADR-0002 path, unchanged. On stdio
        // this is the ordinary case — one process is one caller.
        return Ok(CallerContext::new(fallback_agent_id.to_owned(), Vec::new()));
    };

    // Presented but unverifiable => reject. Checked before deserialising so the
    // operator gets the actionable error ("no issuer key") rather than a
    // parse complaint about a token the server was never going to accept.
    let Some(issuer) = issuer else {
        return Err(IdentityError::NoIssuerConfigured);
    };

    let capability: Capability = serde_json::from_value(raw.clone())
        .map_err(|err| IdentityError::Malformed(err.to_string()))?;

    // Signature, expiry, and key id. `verify` is constant-time on the MAC.
    issuer.verify(&capability)?;

    Ok(CallerContext::new(
        capability.principal.clone(),
        roles_from_scope(&capability.scope),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn issuer() -> CapabilityIssuer {
        CapabilityIssuer::new("k1", b"test-key-material")
    }

    fn meta_of(cap: &Capability) -> Value {
        serde_json::to_value(cap).expect("capability serialises")
    }

    #[test]
    fn absent_capability_falls_back_to_the_boot_identity() {
        let ctx = resolve_caller(None, Some(&issuer()), "boot-agent").expect("no capability is ok");
        assert_eq!(ctx.caller_id, "boot-agent");
        assert!(ctx.roles.is_empty());
    }

    #[test]
    fn absent_capability_is_fine_with_no_issuer_too() {
        // The default stdio deployment: no issuer key, no capability. This must
        // stay working or every existing operator breaks on upgrade.
        let ctx = resolve_caller(None, None, "boot-agent").expect("no capability, no issuer is ok");
        assert_eq!(ctx.caller_id, "boot-agent");
    }

    #[test]
    fn valid_capability_becomes_the_caller_identity() {
        let iss = issuer();
        let cap = iss.issue(
            "alice",
            "role:reader namespace:acme",
            Some(Duration::minutes(5)),
        );
        let ctx = resolve_caller(Some(&meta_of(&cap)), Some(&iss), "boot-agent")
            .expect("valid capability is accepted");
        assert_eq!(ctx.caller_id, "alice", "principal must replace the boot id");
        assert_eq!(ctx.roles, vec!["reader".to_string()]);
    }

    #[test]
    fn capability_without_an_issuer_is_rejected_not_downgraded() {
        let cap = issuer().issue("alice", "role:admin", None);
        let err = resolve_caller(Some(&meta_of(&cap)), None, "boot-agent")
            .expect_err("unverifiable capability must not be accepted");
        assert_eq!(err, IdentityError::NoIssuerConfigured);
    }

    #[test]
    fn forged_signature_is_rejected() {
        let iss = issuer();
        let mut cap = iss.issue("alice", "role:admin", None);
        cap.principal = "mallory".into(); // signature now covers the wrong principal
        let err = resolve_caller(Some(&meta_of(&cap)), Some(&iss), "boot-agent")
            .expect_err("tampered capability must be rejected");
        assert_eq!(
            err,
            IdentityError::Rejected(CapabilityError::SignatureMismatch)
        );
    }

    #[test]
    fn expired_capability_is_rejected() {
        let iss = issuer();
        let cap = iss.issue("alice", "role:admin", Some(Duration::seconds(-1)));
        let err = resolve_caller(Some(&meta_of(&cap)), Some(&iss), "boot-agent")
            .expect_err("expired capability must be rejected");
        assert!(matches!(
            err,
            IdentityError::Rejected(CapabilityError::Expired(_))
        ));
    }

    #[test]
    fn capability_from_another_key_is_rejected() {
        let theirs = CapabilityIssuer::new("other-key", b"someone-elses-material");
        let cap = theirs.issue("alice", "role:admin", None);
        let err = resolve_caller(Some(&meta_of(&cap)), Some(&issuer()), "boot-agent")
            .expect_err("capability signed by an unknown key must be rejected");
        assert_eq!(
            err,
            IdentityError::Rejected(CapabilityError::UnknownKey("other-key".into()))
        );
    }

    #[test]
    fn malformed_capability_is_rejected() {
        let err = resolve_caller(
            Some(&serde_json::json!({"nope": 1})),
            Some(&issuer()),
            "boot",
        )
        .expect_err("garbage in _meta must be rejected");
        assert!(matches!(err, IdentityError::Malformed(_)));
    }

    #[test]
    fn no_rejection_path_ever_yields_the_boot_identity() {
        // The property that matters, stated once over every failure mode: a
        // presented-but-unacceptable capability must never resolve to the
        // operator's identity. A silent downgrade would grant a forged token
        // MORE authority than it asked for.
        let iss = issuer();
        let mut forged = iss.issue("alice", "role:admin", None);
        forged.signature = vec![0; 32];
        let cases: Vec<(&str, Option<&CapabilityIssuer>, Value)> = vec![
            ("no issuer", None, meta_of(&iss.issue("alice", "x", None))),
            ("forged", Some(&iss), meta_of(&forged)),
            (
                "expired",
                Some(&iss),
                meta_of(&iss.issue("a", "x", Some(Duration::seconds(-1)))),
            ),
            (
                "malformed",
                Some(&iss),
                serde_json::json!("not-a-capability"),
            ),
        ];
        for (name, iss_opt, value) in cases {
            let result = resolve_caller(Some(&value), iss_opt, "boot-agent");
            assert!(result.is_err(), "{name}: must reject, got {result:?}");
        }
    }

    #[test]
    fn scope_splits_into_roles_and_opaque_scopes() {
        let scope = "role:reader role:writer namespace:acme recall";
        assert_eq!(roles_from_scope(scope), vec!["reader", "writer"]);
        assert_eq!(scopes_from_scope(scope), vec!["namespace:acme", "recall"]);
    }

    #[test]
    fn empty_and_roleless_scopes_yield_no_roles() {
        assert!(roles_from_scope("").is_empty());
        assert!(roles_from_scope("recall namespace:acme").is_empty());
        // `role:` with nothing after it is not a role.
        assert!(roles_from_scope("role:").is_empty());
    }
}
