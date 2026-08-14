//! Write-time SHAPE detector for provider-returned opaque reasoning payloads.
//!
//! # Why this exists (arXiv:2608.09867)
//!
//! [arXiv:2608.09867](https://arxiv.org/abs/2608.09867) (2026-08-10) showed that
//! provider-returned **encrypted reasoning blocks** — the opaque `reasoning` /
//! `redacted_thinking` payloads some model APIs hand back — carry **no session,
//! user, or model binding**, and that of 315,320 such blocks scraped from public
//! repositories, 367 leaked PII artifacts and 182 leaked credentials. Any agent
//! that `REMEMBER`s a raw assistant turn is now plausibly persisting one of those
//! blocks into a durable, shareable store — where it can later be recalled or
//! shared without anyone realizing a credential rode along inside an opaque blob.
//!
//! So on the write path we flag content that has the **shape** of such a payload
//! and record the flag on the write's provenance (see
//! [`crate::model::write_provenance::WriteFlag`]). The write is **not rejected** —
//! a memory database that silently drops writes is worse than one that stores a
//! flagged write you can later revoke by principal or session.
//!
//! # What this deliberately does NOT do
//!
//! **Shape detection only. We never decode.** This module does not base64-decode,
//! decompress, JSON-parse the inner payload, or otherwise attempt to look inside
//! the blob, and it takes **no dependency that could** (no base64, no crypto, no
//! decompression crate). Two reasons: (1) decoding a provider-encrypted block
//! could require pulling in a parser/crypto surface that becomes its own attack
//! surface on untrusted input, and (2) materializing the decoded bytes would risk
//! surfacing the very secret we are trying to avoid touching. We match a shape and
//! record that we matched it. **A positive flag does NOT prove the payload
//! contains a secret** — it means the content looks like an opaque provider
//! reasoning payload, which is worth being able to find and revoke later.

/// Minimum length of a contiguous base64-ish run to be treated as an opaque
/// blob. Provider encrypted-reasoning blocks are hundreds to thousands of
/// characters; 256 is a conservative floor that ordinary prose (which is
/// whitespace-broken and not a single base64 run) does not reach.
const MIN_BLOB_LEN: usize = 256;

/// True if `c` is in the base64 / base64url alphabet (plus padding). Used only
/// to measure the *shape* of a run — never to decode it.
fn is_base64ish(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_')
}

/// Detect whether `content` has the shape of a provider-returned opaque
/// reasoning payload. Returns a short, human-readable reason for the match (for
/// logging / the flag audit trail), or `None`. **Shape only — this never decodes
/// the content and a match does not prove a secret is present.**
pub fn detect(content: &str) -> Option<&'static str> {
    // 1. Structured provider reasoning-block markers. These are the JSON shapes
    //    the APIs return; we look for the key co-occurrence, not the value.
    let lc = content.to_ascii_lowercase();
    let has = |needle: &str| lc.contains(needle);

    if (has("\"type\":\"reasoning\"") || has("\"type\": \"reasoning\"")) && has("encrypted_content")
    {
        return Some("openai-style reasoning block with encrypted_content");
    }
    if (has("\"type\":\"redacted_thinking\"") || has("\"type\": \"redacted_thinking\""))
        && has("\"data\"")
    {
        return Some("anthropic-style redacted_thinking block with opaque data");
    }
    if has("reasoning.encrypted_content") || has("encrypted_reasoning") {
        return Some("provider encrypted-reasoning field");
    }

    // 2. Bare opaque blob: a single long contiguous base64-ish run. We scan for
    //    the longest such run anywhere in the content (so a blob embedded in JSON
    //    quotes is caught even without the markers above), then require it to
    //    carry mixed case + digits — the entropy signature of an encoded blob —
    //    so a long lowercase hex string or a run of underscores does not trip it.
    if longest_base64ish_run_looks_opaque(content) {
        return Some("long high-entropy base64-shaped blob");
    }

    None
}

/// Scan `content` for the longest contiguous base64-ish run and decide whether it
/// looks like an opaque encoded blob (length ≥ [`MIN_BLOB_LEN`] and carrying
/// upper + lower + digit). Pure shape inspection; nothing is decoded.
fn longest_base64ish_run_looks_opaque(content: &str) -> bool {
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if is_base64ish(chars[i]) {
            let start = i;
            while i < chars.len() && is_base64ish(chars[i]) {
                i += 1;
            }
            let run = &chars[start..i];
            if run.len() >= MIN_BLOB_LEN {
                let has_upper = run.iter().any(|c| c.is_ascii_uppercase());
                let has_lower = run.iter().any(|c| c.is_ascii_lowercase());
                let has_digit = run.iter().any(|c| c.is_ascii_digit());
                if has_upper && has_lower && has_digit {
                    return true;
                }
            }
        } else {
            i += 1;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blob(n: usize) -> String {
        // Deterministic mixed-case+digit base64-ish run of length n (no decode
        // target — purely a shape fixture). Vary by index so it isn't a single
        // repeated char (which some readers might special-case).
        const ALPHA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        (0..n).map(|k| ALPHA[k % ALPHA.len()] as char).collect()
    }

    #[test]
    fn openai_reasoning_block_is_flagged() {
        let s = r#"{"type":"reasoning","summary":[],"encrypted_content":"gAAAAAB..."}"#;
        assert!(detect(s).is_some());
    }

    #[test]
    fn anthropic_redacted_thinking_is_flagged() {
        let s = r#"{"type":"redacted_thinking","data":"EvwBCkYI...opaque..."}"#;
        assert!(detect(s).is_some());
    }

    #[test]
    fn bare_long_blob_is_flagged() {
        assert!(detect(&blob(300)).is_some());
        // Embedded in surrounding text is still caught (longest-run scan).
        let wrapped = format!("assistant said: {} -- end", blob(300));
        assert!(detect(&wrapped).is_some());
    }

    #[test]
    fn ordinary_prose_is_not_flagged() {
        let s = "The user prefers dark mode and lives in Berlin. Remind them at 9am.";
        assert!(detect(s).is_none());
    }

    #[test]
    fn short_token_is_not_flagged() {
        // A normal API key-ish token below the blob floor is not a reasoning-blob
        // shape (this detector is for the opaque-reasoning shape, not secrets).
        assert!(detect(&blob(64)).is_none());
    }

    #[test]
    fn long_lowercase_hex_is_not_flagged() {
        // A 512-char all-lowercase hex string lacks the mixed-case entropy
        // signature, so it is not treated as an opaque encoded blob.
        let hex: String = std::iter::repeat_n("abcdef0123456789", 32).collect();
        assert_eq!(hex.len(), 512);
        assert!(detect(&hex).is_none());
    }
}
