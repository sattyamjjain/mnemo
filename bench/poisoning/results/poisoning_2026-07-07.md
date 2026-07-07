# poisoning_bench — defense delta (ASR with mnemo's quarantine defense ON vs OFF)

> Observed Attack Success Rate for two named memory-poisoning attacks, with mnemo's shipped poisoning-detector quarantine **OFF** (undefended store) vs **ON** (as shipped). The **delta** is the headline: how much the defense removes. **Deterministic, offline, byte-stable**; every rate carries a Wilson 95% interval. These are mnemo's OWN observed numbers — never a claimed one.

- Trials/attack: 200; top-k: 5; seed `0x901504202607`; embedder: hashed-bag-of-tokens (offline); vector index: exact brute-force (deterministic).
- Defense toggled: `check_for_anomaly` → `quarantine_memory` on write + recall's `quarantined` skip; z-score lane via `PoisoningPolicy::with_outlier_threshold(3)`.
- ON vs OFF isolate the quarantine bit on a **byte-identical** poison record.

## Attack Success Rate

| attack | defense lane | **ASR_off** [95%] | **ASR_on** [95%] | **delta** |
|---|---|---:|---:|---:|
| MINJA (canonical) | lexical / self-referential | 100.0% [98.1, 100.0] | 0.0% [0.0, 1.9] | **+100.0 pts** |
| MINJA (evasive, markers stripped) | lexical / self-referential | 100.0% [98.1, 100.0] | 100.0% [98.1, 100.0] | **+0.0 pts** |
| AgentPoison (low-rate trigger) | embedding z-score outlier gate | 100.0% [98.1, 100.0] | 3.5% [1.7, 7.0] | **+96.5 pts** |

## Benign control

Held-out **clean, in-distribution** memories (same vocabulary + case range as the corpus) written through the defended engine: **0/200 false-quarantine (0.0%)**. A trustworthy defense must not quarantine legitimate memories — the delta above is only meaningful at ~0% false-positive. **Caveat (disclosed):** a clean write bearing a brand-new token (e.g. a never-seen identifier) *can* trip the z-score gate when the baseline is sparsely populated — the 0% here holds because the baseline covers the embedding space; a smaller/narrower corpus raises false positives. That coverage dependence is a real property of the z-score lane, not hidden.

AgentPoison poison rate: **0.0998%** of the store (single trigger among 1001 benign) — a genuinely low-rate trigger (< 0.1%).

## Honest reading

- **MINJA canonical** carries the bridging phrasing the paper relies on; the always-on lexical / self-referential lane quarantines it. The **evasive** row strips those markers to a bare false fact: the lexical lane misses it (delta ≈ 0) — a disclosed blind spot, not hidden. The embedding z-score gate is not applied to MINJA here so the lexical lane's limit is visible.
- **AgentPoison** uses a novel-token trigger that is both a unique retrieval match and an embedding-space outlier; the z-score gate quarantines the large majority. The **residual ASR_on is not zero** and we report it as-is: with a finite-width (128-dim) hashed embedder, a novel token occasionally *hash-collides* into a dimension the benign baseline already covers, so that poison looks in-distribution and evades — an honest artifact of the embedder, disclosed not hidden. **Further limitation:** a poison written entirely in in-distribution vocabulary (semantic poisoning with no novel tokens) would not trip the z-score gate at all — that blind spot is real but needs a generative judge to make retrievable-and-deterministic, so it is noted, not benchmarked here.
- Not a claim that mnemo is poisoning-proof. It is a reproducible measurement of what the shipped quarantine buys on these two attacks. Reproduce: `cargo run --release -p mnemo-poisoning-bench`.
