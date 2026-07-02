# ASI06 memory-poisoning resistance — query-only MINJA variant

> 2026-07-02 — reproducible resistance micro-bench for OWASP **ASI06 (Memory & Context Poisoning)**. Measures mnemo's *existing* poisoning defense (`check_for_anomaly` → `quarantine` → recall skips quarantined) against a MINJA-style ([arXiv:2503.03704](https://arxiv.org/abs/2503.03704)) query-only attack. Not a new detector; not a full adversarial suite. See [`docs/security/ASI06.md`](../../../docs/security/ASI06.md).

- **200 trials/class**, top-5, seed `0xa510062026`, `NoopEmbedding` (BM25 carries retrieval).
- *Poisoning success* = the poison record is recalled in the target-query top-5.
- *Resistance* = 1 − defended poisoning-success rate (Wilson 95% interval).

| attack class | undefended poisoning | defended poisoning | quarantined | **resistance** (95% Wilson) |
|---|---:|---:|---:|---:|
| `canonical_minja` (bridging markers) | 100.0% | 0.0% | 200/200 | **100.0%** [98.1%, 100.0%] |
| `evasive_paraphrase` (markers stripped) | 100.0% | 100.0% | 0/200 | 0.0% [0.0%, 1.9%] |

**Headline:** mnemo quarantines the canonical MINJA query-only poison with **100.0% resistance** (95% Wilson [98.1%, 100.0%], n=200) — the poison is retrievable in an undefended store 100% of the time and is suppressed from recall in the shipped store.

**Honest limitation:** against an *evasive* marker-free paraphrase the always-on lexical layer resists only 0.0% — a semantic paraphrase that carries no bridging markers is not caught by lexical detection. The intended defense there is the opt-in embedding z-score baseline gate (`PoisoningPolicy::with_outlier_threshold`), which this single-embedder run does not exercise. Reproduce: `cargo run --release -p mnemo-locomo-bench --bin asi06_resistance`.
