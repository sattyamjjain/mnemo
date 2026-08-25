# `bench/minja_phase3` — non-adaptive Phase-3 exploitation

Tracks [#37](https://github.com/sattyamjjain/mnemo/issues/37). Design of record:
[ADR 0003](../../docs/adr/0003-minja-procedure-harness.md). Write-up and results:
[`docs/benchmarks/2026-08-25-minja-phase3-nonadaptive.md`](../../docs/benchmarks/2026-08-25-minja-phase3-nonadaptive.md).

> **This is not a MINJA number.** Phases 1 (indication-prompt injection) and 2
> (progressive shortening) are generative and adaptive; they need a model in the
> loop as the attacker and are out of scope pending a budget. ADR 0003 says
> removing the shortening step is exactly what makes a result stop being MINJA.
> This measures the **retrieval-time consequence of already-shortened records**.

## Run it

```bash
curl -sSL --fail -o model.onnx \
  https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx
curl -sSL --fail -o tokenizer.json \
  https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/tokenizer.json
MNEMO_ONNX_MODEL_PATH=./model.onnx cargo run --release --features onnx \
  -p mnemo-minja-phase3-bench --bin minja_phase3_bench
```

The weights are pinned by SHA-256 (`759c3cd2b7fe…`) and the run **refuses to
start** on a mismatch. A path is not a pin: two checkpoints can sit at the same
path and produce different numbers.

## Layout

| | |
|---|---|
| `src/fixture.rs` | derivation of the matched poison/benign pairs, and the invariants that keep them matched |
| `fixtures/phase3_records.json` | the **committed, pre-registered** corpus the run reads. Never regenerated at run time |
| `src/lib.rs` | the four arms, the two structural oracles, the cluster-aware rates |
| `src/bin/derive_fixture.rs` | re-derives the fixture. Run only on a deliberate change; the diff is the audit trail |
| `src/bin/minja_phase3_bench.rs` | the runner |
| `tests/committed_result_is_within_band.rs` | offline drift guard over the committed artifact, run by every `cargo test --workspace` |

## Tests run without the model

`cargo test -p mnemo-minja-phase3-bench` uses `DeterministicEmbedding`, which is
real and offline, so the suite stays green on a machine with no checkpoint on
disk. Only the published *number* needs the pinned weights.

## Reading the result

Four arms, because two would not be interpretable:

| arm | detector | what it answers |
|---|---|---|
| poison OFF | forced off | can a shortened record be exploited at all |
| poison ON | as shipped | does the shipped defense change that |
| benign OFF | forced off | **the floor** — how much is just topical retrieval |
| benign ON | as shipped | does the defense quarantine innocent records |

The benign twin shares the poisoned record's opening clause, tags **and its
restatement of the victim query**, differing only in that it resolves to no
answer. If both are retrieved at the same rate, the headline is measuring
topicality and the poisoning-specific effect is the **delta**, not the rate.

It is, as of 2026-08-25: both 129/135. The delta is `+0.0000`.
