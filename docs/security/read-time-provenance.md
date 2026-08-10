# Read-time provenance — what mnemo can and cannot prove about a recalled entry

> **Status:** honest, code-grounded. This page answers one question for a
> regulated buyer: **given a single entry returned by `recall`, what can mnemo
> tell you about where it came from, and how much of that can it prove?** Every
> claim below is tied to the code path that backs it. Where a capability is
> absent, this page says so plainly rather than implying it. It documents
> `0.5.x` behaviour and is written from the source, not the roadmap.

The short version: mnemo records a rich set of origin fields on write and
returns them on read, and it can hand you a cryptographic receipt that a recall
derived from a specific set of records. It does **not**, today, cryptographically
prove *what kind* of source an entry came from — in particular it cannot prove
that a recalled entry was a tool return rather than a user statement. The origin
type is stored, but it is optional, defaults to `Agent`, is outside the hash
chain, and is not carried in the read receipt.

---

## 1. What is recorded on write

Every `remember` persists a `MemoryRecord`. The provenance-relevant fields, as
they exist in `crates/mnemo-core/src/model/memory.rs`:

| Field | Type | What it records | Set by |
|---|---|---|---|
| `id` | `Uuid` (v7, time-sortable) | Stable identity | engine |
| `agent_id` | `String` | Owning agent | caller |
| `content` | `String` | The remembered text | caller |
| `content_hash` | `Vec<u8>` (SHA-256) | Integrity digest of the content | engine |
| `prev_hash` | `Option<Vec<u8>>` | Chain link to the previous record | engine |
| `source_type` | `SourceType` enum | **Origin class** (see below) | caller, **defaults to `Agent`** |
| `source_id` | `Option<String>` | Free-form origin identifier (e.g. a URL, a tool name) | caller |
| `created_by` | `Option<String>` | Actor that wrote it | caller |
| `tags` | `Vec<String>` | Labels, incl. `source:web` / `source:document` / … conventions | caller |
| `quarantined` / `quarantine_reason` | `bool` / `Option<String>` | Review-queue flag | engine / operator |
| `created_at`, `version`, `prev_version_id` | timestamps / lineage | When, and supersession history | engine |

`SourceType` (`model/memory.rs`) has nine variants:

```rust
pub enum SourceType {
    Agent, Human, System, UserInput, ToolOutput,
    ModelResponse, Retrieval, Consolidation, Import,
}
```

So the **data model can express** "this came from a tool return" (`ToolOutput`)
distinctly from "this came from a person" (`UserInput` / `Human`) or "this was
pulled from an external document" (`Retrieval` / `Import`).

**The integrity chain covers content, not origin.** The content hash is:

```rust
// crates/mnemo-core/src/hash.rs
pub fn compute_content_hash(content: &str, agent_id: &str, timestamp: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hasher.update(agent_id.as_bytes());
    hasher.update(timestamp.as_bytes());
    hasher.finalize().to_vec()
}
```

and records are chained with `prev_hash = SHA-256(content_hash ‖ prev.content_hash)`.
`verify_chain` (`hash.rs`) recomputes this per record and reports the first break.
Note what is **hashed**: `content`, `agent_id`, `created_at` — and nothing else.
`source_type`, `source_id`, `tags`, `importance`, and `metadata` are **not** in
the digest. They are persisted, but the hash chain does not make them
tamper-evident: an actor with write access can change a record's `source_type`
from `ToolOutput` to `Human` and `verify_chain` still passes.

**Write-time origin use.** If the operator attaches a `PoisoningPolicy`,
`query::poisoning::check_for_anomaly` runs on each write and *does* consult
`source_type`: `is_trusted_source` treats `ToolOutput`, `System`, `UserInput`,
`Human`, and `ModelResponse` as trusted to carry instruction-like text, while
`Agent`, `Retrieval`, `Import`, and `Consolidation` are not — a MINJA-style
self-referential marker is only flagged as injection when it arrives on an
untrusted / indirect-ingest record. This is a **detection heuristic at write
time**, not a signed provenance fact, and it inherits the "defaults to `Agent`"
caveat below.

---

## 2. What a caller can query at read time

`recall` returns a `RecallResponse` (`crates/mnemo-core/src/query/recall.rs`).
For each returned entry you get the full `MemoryRecord`, so **every field in §1
is readable at recall time**, including `source_type`, `source_id`, `tags`, and
`created_at`. Two further read-time facilities exist:

**a) A cryptographic read receipt — `ReadProvenance`.** When the caller sets
`RecallRequest.with_provenance = Some(true)` **and** the engine has a
`ProvenanceSigner` attached, the response carries a `ReadProvenance`
(`crates/mnemo-core/src/provenance.rs`):

```rust
pub struct ReadProvenance {
    pub read_id: Uuid,
    pub agent_id: String,
    pub query_hash: Vec<u8>,          // SHA-256 of the query string
    pub derived_from: Vec<RecordRef>, // the cited records, in rank order
    pub hmac: Vec<u8>,               // HMAC-SHA256 over read_id ‖ query_hash ‖ derived_from
    pub hmac_key_id: String,         // for key rotation
    pub ts: DateTime<Utc>,
}

pub struct RecordRef {
    pub id: Uuid,
    pub content_hash: Vec<u8>,
    pub prev_hash: Option<Vec<u8>>,
}
```

`verify_read_provenance` re-fetches each cited record, recomputes its
`content_hash`, and constant-time-compares the HMAC. It answers, offline and
after the fact: **"did this recall return exactly these records, unmodified
since?"** It detects (1) a source record mutated after the read, (2) a forged or
altered receipt, and (3) uses `hmac_key_id` so rotating the signing key does not
break old audits.

**b) Integrity verification.** `verify_chain` / `verify_event_chain` re-derive
the SHA-256 chain over a set of records/events and report the first break.

Optional read-path filters exist but are about **relevance and safety, not
origin proof**: the cost-aware *evidence budget* (`query::evidence`) returns the
smallest sufficient *prefix* of the already-ranked results — scored by cosine or
answer-impact, **never by source trust** — and a `reasoning_trust` post-filter
can drop entries whose stored reasoning-provenance metadata fails a policy.
Quarantined records are **excluded** from recall (`recall.rs` skips
`record.quarantined`); to review them, operators call
`query::poisoning::replay_quarantine`, which returns each held record *with* its
`source_type` and `quarantine_reason`.

---

## 3. What is NOT distinguishable or provable today

Stated plainly, so a buyer does not infer more than the code delivers.

**A recalled tool-return is not *provably* separable from a user-sourced entry.**
The `SourceType` enum can *represent* the difference, but three gaps mean mnemo
cannot prove it at read time:

1. **`source_type` is optional and defaults to `Agent`.** The write path is
   `source_type: request.source_type.unwrap_or(SourceType::Agent)`
   (`query/remember.rs`). Unless the caller explicitly tags every write, a tool
   return and a user statement are **both** stored as `Agent` and are
   indistinguishable. Separability is a property of caller discipline, not of the
   store.
2. **`source_type` is outside the hash chain.** As shown in §1, the content hash
   covers `content ‖ agent_id ‖ created_at` only. A flipped `source_type`
   survives `verify_chain`. So even a diligently-tagged origin is **not
   tamper-evident**.
3. **The read receipt does not carry origin.** `RecordRef` binds `id`,
   `content_hash`, and `prev_hash` — not `source_type`. `ReadProvenance` proves
   *which records* a recall cited and that their *content* is unchanged; it does
   **not** attest *what kind* of source they were. A verifier who also holds the
   records can *read* their (unsigned) `source_type`, but cannot cryptographically
   trust it.

Consequently, "was this recalled fact injected via a retrieved document, or
stated by the authenticated user?" is answerable **only** as far as the caller
populated `source_type`/`tags` correctly and you trust whoever had write access
to the row. It is not a signed, verifiable property.

Two further boundaries:

- **The read receipt is symmetric.** `ReadProvenance` is HMAC-SHA256 — only a
  party holding the key can verify it. It is not third-party non-repudiation. For
  externally-auditable signatures, pair it with the Ed25519-signed audit-log
  export in `mnemo-compliance` (a separate mechanism, not part of `recall`).
- **There is no per-entry trust score at read time.** Recall ranks by retrieval
  relevance; poisoning scoring runs at *write* time. A recalled entry carries no
  numeric "how trustworthy is this source" signal.

---

## 4. What the literature is asking for (open questions, not features)

These are framed as gaps between what regulated buyers are starting to ask for
and what mnemo does today. They are **not** implemented capabilities.

- **Provenance that changes decisions.** [arXiv:2608.06057](https://arxiv.org/abs/2608.06057)
  reports that untrusted-sourced memories flip an agent's decision in **32.1%**
  of cases — i.e. *where* a memory came from materially changes the answer, so a
  store that cannot prove origin at read time leaves that 32.1% unguarded. mnemo
  records `source_type` but, per §3, does not sign it or expose a
  trust-weighted recall. **Open question:** bring origin into the hash chain and
  into `RecordRef`, and offer a recall mode that down-weights or flags
  untrusted-source entries.
- **Type-conditioned decay.** [arXiv:2608.04746](https://arxiv.org/abs/2608.04746)
  argues that memories should decay *differently by source type* — e.g. a
  volatile tool return should age faster than a stated user preference. mnemo has
  per-record `decay_rate` / `decay_function`, but the decay path does **not**
  condition on `source_type` today. **Open question:** make decay a function of
  origin class.

Both are honest gaps. This page will claim them as features only when the code
does.

---

### Verify the claims on this page

```bash
# content hash covers content|agent_id|created_at only:
sed -n '/pub fn compute_content_hash/,/^}/p' crates/mnemo-core/src/hash.rs
# source_type defaults to Agent on write:
rg -n 'source_type: request.source_type.unwrap_or' crates/mnemo-core/src/query/remember.rs
# the read receipt binds id/content_hash/prev_hash, not source_type:
sed -n '/pub struct RecordRef/,/^}/p' crates/mnemo-core/src/provenance.rs
# quarantined records are excluded from recall:
rg -n 'quarantined' crates/mnemo-core/src/query/recall.rs
```
