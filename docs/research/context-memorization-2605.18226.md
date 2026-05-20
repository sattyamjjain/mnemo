# Context Memorization (arXiv 2605.18226) — attention-state-memory anchor

> Recorded 2026-05-20. **Composition anchor + substrate-only ship,
> NOT an integration claim.** mnemo's v0.4.5 ships the
> *substrate* the paper anchors against (typed store +
> `mnemo.attention_state.put` / `.get` MCP tools). End-to-end
> Context Memorization — extracting prefix states from a runtime,
> hashing them, re-injecting them on next generation — is a
> *producer + consumer* layer mnemo does not own. The standing
> overclaim phrasings (`Context-Memorization-compliant`,
> `attention-state-compatible`, `KV-cache-portable`) are blocked by
> the README marketing-phrase test extension that lands alongside
> this anchor.

## Citation

- **Paper:** Okoshi, Chen, Lu, Fan, Motomura, Fujiki — Institute of
  Science Tokyo + Imperial College London. Paraphrased title used in
  prose: *"the Context Memorization result"*. Literal title in
  §Sources.
- **arXiv:** [2605.18226](https://arxiv.org/abs/2605.18226)
- **Surfaced:** 2026-05-19

## What Context Memorization measures

The paper observes two structural limits of prefix-augmented
inference: (a) the prefix's influence fades as generation proceeds,
and (b) attention computation over the prefix scales linearly with
its length. Its contribution is a *training-free* externalization
of prefix attention states into a lightweight, lookup-based memory:
precompute attention over the prefix once, serialize the resulting
state, and on subsequent generations against the same prefix,
re-inject the cached state rather than recomputing.

Two operational consequences:

1. The prefix becomes a *cacheable substrate* — same shape as
   memory records in mnemo, different blob.
2. The mechanism is *runtime-only* — no training, no fine-tuning;
   the cache producer is the inference layer; the consumer is the
   next generation against the same prefix.

The paper's framing names the lookup-based memory directly: *"a
training-free approach that externalizes the prefix into a
lightweight, lookup-based memory of precomputed attention
states."* That is the substrate mnemo's v0.4.5 ships.

## Where mnemo fits

mnemo provides the *store*. v0.4.5 ships:

| Surface | Where it lives |
|---|---|
| `AttentionStateStore` typed trait | [`crates/mnemo-attention-state`](../../crates/mnemo-attention-state) |
| `InMemoryAttentionStateStore` reference impl | same crate; tests + short-lived sessions |
| `AttentionStateRecord` envelope (id / agent_id / prefix_hash / model / state_blob / blob_sha256_hex / ttl_seconds / created_at) | same crate |
| `mnemo.attention_state.put` MCP tool | `crates/mnemo-mcp/src/tools/attention_state.rs` + `server.rs` |
| `mnemo.attention_state.get` MCP tool | same |
| `MnemoServer::with_attention_state(...)` builder | server.rs — optional; unconfigured calls return a spec-shaped error |

The substrate keys are `(agent_id, prefix_hash)`. The `prefix_hash`
is opaque to mnemo — convention is the producer's SHA-256 of the
prompt tokens, but the store treats it as a string identifier. The
`state_blob` is also opaque — its format, quantization sensitivity,
and model-version compatibility are the *producer's*
responsibility. mnemo stores it; mnemo does not interpret it.

## What this anchor is NOT

- **NOT a Context Memorization implementation.** mnemo does not
  extract prefix attention states from any inference runtime.
  Producer is out of scope.
- **NOT an inference-runtime integration.** mnemo does not wire to
  vLLM, TGI, Triton, or any specific runtime. The mechanism is
  transport-agnostic by design — any producer that can write
  bytes can populate the substrate.
- **NOT a RECALL fast-path.** The existing semantic + BM25 + graph
  + recency hybrid retrieval does NOT consult the attention-state
  store. The two substrates sit orthogonal. A future v0.5.x row
  may explore a prefix-matched fast-path; today's surface is the
  store and the two MCP tools.
- **NOT a stability claim on the blob format.** The
  `AttentionStateRecord` schema is starter; pin the mnemo minor
  version if relying on byte-level layout. Producers chasing a
  specific quantization should encode model + quantization
  identity in the optional `model` field of the record.
- **NOT encrypted-at-rest at the storage trait.** The in-memory
  reference store holds bytes as `Vec<u8>`. Encryption is the
  operator's responsibility at the tool / engine layer using the
  existing `mnemo-core::encryption::ContentEncryption` helper.
- **NOT a benchmark.** No bench harness compares attention-state
  lookup cost vs prefix recomputation. The paper provides those
  numbers for the *complete* mechanism; mnemo's store is the
  storage substrate one of those numbers would baseline against.

## Operator recipe — putting the substrate to work today

An operator with an inference runtime that exposes prefix-state
extraction can use mnemo today as the cache substrate:

1. **Produce.** After the runtime emits the prefix attention state,
   compute a `prefix_hash` (convention: hex SHA-256 of the prompt
   tokens that produced the state).
2. **Encrypt (optional).** Wrap the state blob with
   `ContentEncryption::encrypt(...)` from mnemo-core.
3. **Store.** Call `mnemo.attention_state.put { agent_id,
   prefix_hash, state_blob_hex, model, ttl_seconds }`.
4. **Recall on the next prompt against the same prefix.** Call
   `mnemo.attention_state.get { agent_id, prefix_hash }`. On a hit,
   decrypt and re-inject the state in the runtime. On a miss, fall
   through to normal prefix recomputation.

mnemo's job ends at step 3 / step 4 (the store). The encryption,
re-injection, and end-to-end win/loss measurement are the
operator's job.

## Why no `RetrievalMode` integration

v0.4.4 introduced [`RetrievalMode::HarnessAware`](../research/grep-vs-vector-2605.15184.md)
for envelope-format reshaping. v0.4.5 deliberately does NOT add a
`RetrievalMode::AttentionStateBacked` variant — the two surfaces
have orthogonal contracts (memory recall over text/embedding vs
attention-state lookup). A future v0.5.x row may explore the
composition; today the substrates stay separate.

## Cross-references

- Substrate crate: [`crates/mnemo-attention-state`](../../crates/mnemo-attention-state)
- MCP tool inputs: [`crates/mnemo-mcp/src/tools/attention_state.rs`](../../crates/mnemo-mcp/src/tools/attention_state.rs)
- MCP tool methods: [`crates/mnemo-mcp/src/server.rs`](../../crates/mnemo-mcp/src/server.rs)
  (`attention_state_put` + `attention_state_get`)
- Companion grep-vs-vector anchor (v0.4.4): [`grep-vs-vector-2605.15184.md`](grep-vs-vector-2605.15184.md)
- Companion outcome-diff anchor (v0.4.4): [`delegate52-2604.15597.md`](delegate52-2604.15597.md)
- v0.4.5 carry list: [`../../CHANGELOG.md`](../../CHANGELOG.md) `[0.4.5]` section.

## Sources

- arXiv 2605.18226 — https://arxiv.org/abs/2605.18226 — *"Context Memorization: Training-Free Attention-State Externalization for Prefix-Augmented Inference"* (literal title, 2026-05-19).
