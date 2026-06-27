//! Minimal shared-secret bearer-token check for the network entrypoints
//! (REST + gRPC). This is the floor — "don't run an open memory server" — not
//! a full auth system: one operator-held secret in `MNEMO_AUTH_TOKEN`, compared
//! in constant time. There are no users, scopes, or rotation here; per-record
//! authorization is the separate ACL/RBAC layer in [`crate::model::acl`].

/// Constant-time byte comparison. Returns `false` immediately on a length
/// mismatch (length is not secret here), otherwise compares every byte so the
/// time taken does not leak how many leading bytes matched.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Validate an HTTP/gRPC `Authorization` header value against the expected
/// shared secret.
///
/// Accepts `Authorization: Bearer <token>` (case-insensitive scheme) or a bare
/// token. Returns `true` only on an exact, constant-time match. A missing
/// header (`None`) is always `false`.
///
/// `expected` is assumed non-empty; an empty expected secret means "auth not
/// configured" and callers should not even reach this function (run open mode
/// + warn instead of accepting an empty token).
pub fn bearer_token_matches(authorization_header: Option<&str>, expected: &str) -> bool {
    if expected.is_empty() {
        return false;
    }
    let Some(raw) = authorization_header else {
        return false;
    };
    let token = match raw.split_once(' ') {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("bearer") => rest.trim(),
        _ => raw.trim(),
    };
    constant_time_eq(token.as_bytes(), expected.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_header_is_rejected() {
        assert!(!bearer_token_matches(None, "s3cret"));
    }

    #[test]
    fn empty_expected_never_matches() {
        assert!(!bearer_token_matches(Some("Bearer "), ""));
        assert!(!bearer_token_matches(Some("Bearer x"), ""));
    }

    #[test]
    fn correct_bearer_matches() {
        assert!(bearer_token_matches(Some("Bearer s3cret"), "s3cret"));
        // case-insensitive scheme, bare token also accepted
        assert!(bearer_token_matches(Some("bearer s3cret"), "s3cret"));
        assert!(bearer_token_matches(Some("s3cret"), "s3cret"));
    }

    #[test]
    fn wrong_token_is_rejected() {
        assert!(!bearer_token_matches(Some("Bearer nope"), "s3cret"));
        assert!(!bearer_token_matches(Some("Bearer s3cre"), "s3cret"));
        assert!(!bearer_token_matches(Some("Bearer s3cretX"), "s3cret"));
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        // `Bearer <token> ` trims to the bare token and still matches.
        assert!(bearer_token_matches(Some("Bearer  s3cret "), "s3cret"));
    }

    #[test]
    fn length_mismatch_is_rejected() {
        assert!(!constant_time_eq(b"ab", b"abc"));
        assert!(constant_time_eq(b"abc", b"abc"));
    }
}
