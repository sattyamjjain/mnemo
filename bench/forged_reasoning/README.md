# Forged-reasoning memory-injection resistance benchmark

> **The number** (real embedder — Ollama `nomic-embed-text`, 768-dim, 3 seeds):
> planted forged-reasoning entries survive to recall at **ASR 100.0%
> [95% 96.9, 100.0]** with the defense **OFF**, and **0.0% [95% 0.0, 3.1]** with
> mnemo's reasoning-provenance trust filter **ON** — a **100.0-point** reduction —
> at **0/180 = 0.0% [95% 0.0, 2.1]** benign false-quarantine.
>
> Never read the headline without the two controls: the **Wilson 95%** interval
> (ASR_on's upper bound is **3.1%**, not a bare 0%) and the **benign
> false-quarantine rate**. A filter that quarantines everything would also drive
> ASR to 0 — the 0% FPR is what makes the ASR meaningful.

- Runner: [`bench/forged_reasoning/`](.)  ·  Raw JSON (sorted keys, no wall-clock): [`bench/results/forged_reasoning.json`](../results/forged_reasoning.json)
- One command: `MNEMO_ONNX_MODEL_PATH=… cargo run --release --features onnx -p mnemo-forged-reasoning-bench --bin forged_reasoning`

## Threat model — forged *reasoning provenance*

An attacker plants a memory whose stored **justification / chain-of-thought is
fabricated**, so that later retrieval surfaces it as **"already-reasoned truth"**:
a downstream agent sees a ready-made rationale ("step 1 … step 2 … therefore X")
and acts on the lie without re-deriving it.

This is **distinct** from the 2026-07-24 ASI06 content-poisoning bench
([`bench/asi06_poisoning`](../asi06_poisoning)). There the *content* is poisoned
and the auditable layer proves it cannot be *hidden*. Here the content can be
perfectly plausible — what is forged is the entry's **reasoning provenance** (who
actually produced the reasoning). The two benches target different attack
surfaces and are not duplicates.

The forged entries arrive via an **indirect-ingest** path (`SourceType::Retrieval`,
tagged `source:web`) and carry a `ReasoningProvenance { authorship: injected }`;
the clean corpus carries `authorship: model_authored`. Absent/unparseable
provenance **fails closed** to `unverified` (never trusted).

## The defense (shipped, wired into recall)

`retrieval::ReasoningTrustPolicy` on `RecallRequest.reasoning_trust`
([`crates/mnemo-core/src/retrieval.rs`](../../crates/mnemo-core/src/retrieval.rs))
— an **opt-in** filter enforced in the shared recall post-filter
`passes_filters` ([`crates/mnemo-core/src/query/recall.rs`](../../crates/mnemo-core/src/query/recall.rs)).
An entry is *admitted* iff its reasoning `authorship` is trusted
(model-authored / user-provided / tool-verified); otherwise the `Quarantine`
action **excludes** it from results (or `DownWeight` demotes it via
`ReasoningTrustPolicy::rerank`). Default `None` keeps the read path unchanged; the
filter composes with any retrieval strategy (it lives in the shared post-filter,
not a parallel pipeline). Extends the existing `RecallRequest`/`retrieval`
surface — same pattern as `DomainScope`.

## Method

The benchmark seeds one real [`MnemoEngine`] (in-memory DuckDB + USearch HNSW +
Tantivy BM25) with `--clean` model-authored entries and `--trials`
forged-reasoning injections, then recalls each forged entry's target query
**twice on the same store** — `reasoning_trust = None` (**OFF**) vs a
quarantine-untrusted policy (**ON**) — so the *only* variable is the trust filter.
Averaged over `--repeats` seeds.

- **ASR** = fraction of forged entries retrieved in the target-query top-`k`.
- **Benign false-quarantine** = clean entries retrieved OFF that vanish ON.
- Both carry a **Wilson 95%** interval. The runner **refuses to score under a
  no-op (zero-vector) embedder** (`guard_real_embedder`).

## Results

Embedder **Ollama `nomic-embed-text` (768-dim)**; corpus = 120 clean + 40 forged
synthetic entries per seed, 3 seeds; top-k = 5.

| arm | ASR | 95% CI | n |
|---|---:|---|---:|
| defense **OFF** (`reasoning_trust = None`) | 100.0% | [96.9%, 100.0%] | 120 |
| defense **ON** (quarantine untrusted) | **0.0%** | **[0.0%, 3.1%]** | 120 |
| **reduction** | **100.0 pts** | — | — |

**Benign false-quarantine:** **0 / 180 = 0.0%** [95% 0.0%, 2.1%]. Clean
model-authored entries are never dropped by the filter.

## Reproduce

```bash
# Default: local ONNX (all-MiniLM-L6-v2), no API key — model.onnx + tokenizer.json side by side.
MNEMO_ONNX_MODEL_PATH=/path/to/all-MiniLM-L6-v2/model.onnx \
  cargo run --release --features onnx -p mnemo-forged-reasoning-bench --bin forged_reasoning

# Or a local Ollama embedder (what the published numbers above used):
ollama pull nomic-embed-text
cargo run --release -p mnemo-forged-reasoning-bench --bin forged_reasoning -- --embedder ollama

# Or OpenAI:  --embedder openai   (needs OPENAI_API_KEY)
```

## Honesty — what this is / is NOT

- **Embedder- and dataset-specific.** The numbers are for `nomic-embed-text` on a
  **synthetic** corpus (40 forged + 120 clean fixtures). They demonstrate the
  *mechanism* — a metadata-provenance filter excludes injected-authorship
  reasoning at read time — not a claim about any particular production dataset.
  The ASR_on = 0% is a property of the hard filter (injected authorship is
  excluded by construction); the honest contribution is the **paired ASR_off and
  the 0% benign false-quarantine**, both with CIs, showing the filter removes the
  attack *without* collateral damage.
- **Provenance is caller-asserted.** The defense trusts the `authorship` signal a
  writer stamps; it is only as strong as the write-path that sets it (a trusted
  ingestion boundary must label indirect-ingest content `injected`/`unverified`).
  This bench models that boundary; it does not by itself authenticate the writer
  (pair with the `mnemo-compliance` signed audit log for that).
- **Not a poisoning-content detector.** Marker-free semantic content poisoning is
  a different surface — see [`bench/poisoning`](../poisoning) and
  [`bench/asi06_poisoning`](../asi06_poisoning).
- **Never a bare ASR.** Every ASR here ships with its Wilson interval and the
  benign false-quarantine control.
