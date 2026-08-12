# Opaque-reasoning-payload write flag

> **What it is:** a write-time detector that flags a memory whose content has the
> **shape** of a provider-returned opaque reasoning payload, records the flag on
> that write's provenance, and lets the write through. **It detects a shape. It
> does not prove the payload contains a secret.**

## Why (arXiv:2608.09867)

[arXiv:2608.09867](https://arxiv.org/abs/2608.09867) (2026-08-10) examined the
**encrypted reasoning blocks** some model APIs return — the opaque `reasoning` /
`redacted_thinking` payloads a caller is expected to pass back on the next turn.
It found that these blocks carry **no session, user, or model binding**, and that
of **315,320** such blocks scraped from public repositories, **367** leaked PII
artifacts and **182** leaked credentials.

The consequence for a memory database is direct: an agent that `REMEMBER`s a raw
assistant turn is now plausibly persisting one of those blocks into a **durable,
shareable** store — where it can later be recalled or shared without anyone
realizing a credential rode along inside an opaque blob. mnemo is exactly such a
store.

## What mnemo does

On every `REMEMBER`, before any at-rest encryption, mnemo runs a **shape** check
([`mnemo_core::opaque_reasoning`](../../crates/mnemo-core/src/opaque_reasoning.rs)).
If the content matches, it:

1. **records the flag on the write's provenance** — a
   [`WriteFlag::OpaqueReasoningPayload`](../../crates/mnemo-core/src/model/write_provenance.rs)
   on the same `WriteProvenance` record that already captures the principal and
   session (the 2026-08-11 provenance work). The flag is **hashed into** the
   record's `content_hash`, so it is tamper-evident — it cannot be stripped
   without breaking the chain.
2. **emits a `tracing::warn!`** naming the memory id and the matched shape.
3. **stores the write anyway.** The default is **warn-and-record, not reject**: a
   memory database that silently drops writes is worse than one that stores a
   flagged write you can find and revoke.

Because the flag lives on provenance, **cleanup is free**: the flagged write is an
ordinary memory attributed to a principal and session, so
[`forget_by_principal` / `forget_by_session`](./read-time-provenance.md) (FORGET
BY PROVENANCE) already sweep it — no separate path.

The flag surfaces on every provenance read: REST (`GET /v1/memories/{id}/provenance`
→ `flags`), the MCP `mnemo.provenance` tool, and all three SDKs
(`flags` on the provenance record).

```jsonc
// GET /v1/memories/<id>/provenance
{
  "principal": "assistant-agent",
  "session_id": "sess-42",
  "op": "remember",
  "flags": ["opaque_reasoning_payload"],   // <- shape match, recorded for revocation
  "content_hash": "…", "prev_hash": "…"
}
```

## What it detects (shape only)

Two shapes, neither decoded:

- **Structured provider blocks** — the JSON key co-occurrences the APIs return:
  an OpenAI-style `{"type":"reasoning", …, "encrypted_content": …}`, an
  Anthropic-style `{"type":"redacted_thinking","data": …}`, or a bare
  `reasoning.encrypted_content` / `encrypted_reasoning` field.
- **Bare opaque blobs** — a single long (≥ 256 char) contiguous base64/base64url
  run carrying mixed case + digits (the entropy signature of an encoded blob), so
  ordinary prose and long lowercase hex do not trip it.

## The limit, stated plainly

- **It detects a shape; it does not prove a secret is present.** A positive flag
  means the content *looks like* an opaque provider reasoning payload — worth
  being able to find and revoke — not that it *contains* a credential.
- **It never decodes.** The detector does not base64-decode, decompress, or parse
  the inner payload, and takes **no dependency that could**. Two reasons: decoding
  an attacker-supplied blob is its own attack surface, and materializing the
  decoded bytes would risk surfacing the very secret we are avoiding touching. It
  matches a shape and records that it matched.
- **False positives are possible** (any sufficiently long high-entropy blob) and
  **false negatives are possible** (a decoded/short/obfuscated payload). The flag
  is a cheap, revocable signal, not a guarantee.

## Reproduce

```bash
cargo test -p mnemo-core opaque_reasoning              # the shape detector's unit tests
cargo test -p mnemo-core --test write_provenance_test  # flag recorded on provenance + revocable
```
