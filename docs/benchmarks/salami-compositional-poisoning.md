# Salami — compositional memory-poisoning (measured, not defended)

> **The numbers** (n=300 trials, deterministic offline embedder): the
> individually-benign Salami slices are accepted by the write path at a
> **save rate of 100%** (1200/1200, Wilson 95% [99.7%, 100.0%]) and a single
> trigger recall reconstructs the harmful composition at a
> **retrieval-influence (assembly) rate of 100%** (300/300, Wilson 95%
> [98.7%, 100.0%]). The topic-matched **benign control** co-retrieves *just as
> much* (mean 4.0 slices in top-k, identical to the poison arm) yet **never**
> completes the harm — **assembly rate 0%** (0/300, [0.0%, 1.3%]).
>
> **Read this honestly.** This bench **measures a gap; it does not defend
> against one.** The point is that no per-write control rejects an
> individually-benign slice, and ordinary retrieval will happily reassemble the
> harmful whole. The benign control proves the measure is not merely firing on
> topical clustering — same co-retrieval, no harmful composition, rate ~0.

- Runner: [`bench/salami_poisoning/`](../../bench/salami_poisoning)
- Raw result (deterministic key order, no wall-clock): [`bench/results/salami_poisoning.json`](../../bench/results/salami_poisoning.json)
- One command: `cargo run --release -p mnemo-salami-poisoning-bench`
- CI-gated (byte-stable, offline): `cargo test -p mnemo-salami-poisoning-bench`

## The attack shape (arXiv:2608.01637)

Most poisoning benchmarks plant one obviously-toxic memory and ask whether a
per-write filter catches it. The **Salami** shape ([arXiv:2608.01637](https://arxiv.org/abs/2608.01637))
is harder: an attacker writes `N` **individually benign** slices — each one
passes any content or anomaly check in isolation — whose **union**, once
co-retrieved for a triggering query, reconstructs a harmful capability. The harm
lives in the composition, not in any single slice, so a defense that inspects one
write at a time cannot see it.

This is the compositional case of [issue #37](https://github.com/sattyamjjain/mnemo/issues/37).

## What is measured

Two rates on mnemo's shipped `remember()` / `recall()` path, each with a
**Wilson 95%** interval:

| Rate | Definition | Poison arm | Benign control |
|---|---|---|---|
| **Save rate** | share of slices the write path accepts (stored, not quarantined) | **100%** [99.7, 100.0] | 100% [99.7, 100.0] |
| **Retrieval-influence (assembly) rate** | share of trials where one trigger recall co-retrieves enough slices to **complete** the harmful composition | **100%** [98.7, 100.0] | **0%** [0.0, 1.3] |
| mean slices co-retrieved (top-k) | how many target slices land in the top-k | 4.0 | 4.0 |

The benign control is the load-bearing row: it shares the surface topic and
co-retrieves *identically* (4.0 slices), so the measure is not blind to it — it
simply carries no harm-completing fragment, so its assembly rate is ~0.

## Honesty / covered subset

- **Retrieval is real** — mnemo's shipped `recall()` over `DuckDbStorage` +
  USearch + Tantivy — but **deterministic and offline** via a lexical
  (bag-of-words) `DeterministicEmbedding`. Co-retrieval here is *lexical*
  co-retrieval. A semantic embedder (ONNX / Ollama / OpenAI) is the stronger,
  less reproducible measure and is disclosed **future work**.
- **Harm completion is a structural oracle**, not an LLM: a retrieved slice-set
  "completes the harm" iff every harm-completing fragment is jointly present. It
  stands in for a downstream model actually composing the slices.
- Covers the **compositional / Salami** subset of #37 only. Not covered:
  semantic paraphrase of the completing fragments, adaptive attackers,
  cross-session drip, and LLM-judged harm composition. #37 remains open for the
  full MINJA-procedure harness it originally scopes.

## Relationship to the other poisoning benches

- [ASI06 auditable resistance](asi06-poisoning.md) — tamper-evidence +
  attribution over the audit layer (poisoning cannot be *hidden*).
- [`bench/poisoning`](../../bench/poisoning) — write-time quarantine defense-delta.
- **This bench** — the *compositional* gap: individually-benign writes that no
  per-write layer rejects, reassembled at read time.

The mitigation direction is composition-aware retrieval scoring / provenance
clustering, not another per-write filter — a per-write filter is exactly what
the Salami shape is built to slip past.
