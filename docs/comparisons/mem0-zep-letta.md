# mnemo vs Mem0, Zep and Letta - long-form comparison

> Living doc, first written 2026-08-05 for the v0.5.21 cut. Mem0, Zep and Letta
> are the three names a 2026 search for "agent memory" returns first, so an
> operator weighing mnemo deserves an honest side-by-side. Competitor facts
> follow each vendor's public docs; mnemo's own number is cited by bench path,
> not asserted.

## Why this comparison exists

Mem0, Zep and Letta are the well-funded, well-marketed memory layers for AI
agents. Mem0 and Zep ship an open-source core plus a hosted platform. Letta,
formerly MemGPT, is open-source with Letta Cloud on top. All three lead with
LLM-judged question-answering accuracy on public benchmarks. mnemo does not
compete on that axis and will not pretend to. This doc states where the lines
actually are.

## Honest concession up front

mnemo is not the accuracy leader. Mem0 publishes an LLM-judged LoCoMo QA figure
of 92.5 on mem0.ai/research, community re-runs land lower, and Zep and Letta both
have established recall research lines. mnemo's own published numbers are
retrieval metrics, not end-to-end LLM-judged QA, and they are deliberately
modest. If the only thing that matters for a deployment is the headline QA
number, one of the other three is the honest recommendation.

What mnemo has that they do not document is a memory that runs entirely on the
operator's own infrastructure and whose every write is a tamper-evident,
offline-verifiable audit record. That is the axis the table below is built on.

## mnemo's own number, cited not claimed

mnemo's retrieval quality is measured by a committed bench, not a marketing line:
`bench/locomo/src/bin/semantic_recall_bench.rs`, run with a real local embedder
(Ollama nomic-embed-text, 768-dim) over a held-out LongMemEval slice. Latest run:
recall@1 0.739, recall@5 0.826, MRR 0.806, recorded in
`docs/benchmarks/baseline.json`. This is gold-document retrieval recall, not the
LLM-judged QA accuracy the competitors headline. The two are different axes and
are not comparable. Do not read 0.739 against a 92.5.

## The axes, stated as they are

| axis | mnemo | Mem0 | Zep | Letta |
|---|---|---|---|---|
| On-prem, no hosted tier to trust | Yes, in-process (embedded DuckDB or your PostgreSQL) | OSS core, plus a hosted platform | OSS core, plus a hosted platform | OSS, plus Letta Cloud |
| Cryptographic hash-chain audit log, offline-verifiable | Yes, SHA-256 `agent_events` chain an external verifier checks without the store | Not a documented primitive | Not a documented primitive | Not a documented primitive |
| Published LLM-judged QA accuracy, the leaderboard axis | No, retrieval-only and modest | Yes, its headline | Yes | Yes |
| Retrieval recall on mnemo's own bench | recall@1 0.739 (nomic-embed-text, `bench/locomo`) | Not run here | Not run here | Not run here |
| Regulatory mapping (EU AI Act Art.12, India DPDP, HIPAA) | Yes, per-clause docs wired to the audit bench | No equivalent published mapping | No equivalent published mapping | No equivalent published mapping |
| Temporal knowledge-graph memory | No, record plus graph relations, not a bitemporal KG | Partial | Yes, Zep's design centre | Partial |
| License | Apache-2.0, nothing gated | Mixed, OSS plus commercial | Mixed, OSS plus commercial | Mixed, OSS plus commercial |

The rows are chosen to be true, not flattering. mnemo loses the QA-accuracy row
and the temporal-KG row, which is Zep's whole design. It wins the on-prem row and
the tamper-evident-audit row, which is the reason it exists: a memory a regulated
operator can run and verify without trusting anyone else's store.

## When to pick which

- Pick Mem0, Zep or Letta when the deciding factor is published QA accuracy, a
  hosted option, or a temporal knowledge graph, and sending memory to a vendor is
  acceptable.
- Pick mnemo when the memory must run on your own infrastructure with no hosted
  tier, and every write and delete has to be an offline-verifiable audit record
  for EU AI Act Art.12, India DPDP or HIPAA.

## Sourcing

Mem0's 92.5 is its own published figure (mem0.ai/research); community re-runs land
materially lower. Vendor capability claims follow each product's public docs and
the Developers Digest 2026 memory-provider survey
(<https://www.developersdigest.tech/blog/best-ai-agent-memory-providers-2026>). A
"No" cell means the capability is not documented as a first-class feature, not
that it is impossible to build. Zep is Zep AI's open-source temporal-KG memory
(Graphiti); Letta is the open-source successor to MemGPT.
