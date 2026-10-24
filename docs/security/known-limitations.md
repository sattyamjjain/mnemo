# Known security limitations

Measured claims live in [`docs/benchmarks/index.md`](../benchmarks/index.md).
This page is the other half: things a reader might reasonably assume mnemo
measures or enforces, and does not.

It exists because an unmeasured gap tracked only as an open issue reads as work
in progress, and after four months across five releases that stops being true. A
limitation someone can find and plan around is worth more than an issue that has
outlived its own milestone.

## The hash chains are keyless

**Status: true of every release so far. Reported privately in October 2026. The
fix is not built yet.**

mnemo keeps three SHA-256 chains in the store:

| Chain | Each entry hashes | Verifier |
|---|---|---|
| memories | `content ‖ agent_id ‖ created_at`, linked to the previous record's content hash | `hash::verify_chain` |
| `agent_events` | the operation's source data, linked to the previous event's content hash | `hash::verify_event_chain`, which checks the links but does not recompute the content hash |
| write provenance | memory id, principal, capability, session, operation, time and flags, chained to the previous entry's hash | `verify_provenance_chain` |

None of them uses a key, and nothing outside the store records where a chain
should end. `chain_heads` holds the current memory and event heads, but only the
write path reads it, and it sits in the same store. The memory and event links
are pairwise: each one covers a record and the record before it, not everything
before it.

### What the chains catch

An edit by anyone who does not recompute the hashes: a bug, a bad migration, a
hand edit to a row, a write that goes around the engine. Within that model the
published numbers hold: 100% of single-byte mutations in the
[audit-conformance report](../../bench/audit_conformance/results/conformance.md),
and 100% of mid-chain deletes, reorders and hash forgeries in the
[audit-tamper report](../../bench/audit_tamper/results/audit_tamper.md).

### What they do not catch

Anyone who can write the store (the DuckDB file, or the PostgreSQL tables) can do
any of these, and every verifier still passes:

- **Rewrite a record.** In the memory chain that means recomputing its content
  hash, its own link and the next record's link: three hashes, no key. In
  `agent_events`, editing an event's payload needs no hashing at all. The
  provenance chain is cumulative, so there it means rehashing every later entry,
  which is still only hashing.
- **Cut the tail.** Delete the newest records. What is left is a valid chain.
- **Roll back.** Put back an older copy of the store.
- **Change content under its provenance.** `verify_provenance_chain` covers who
  wrote a memory, under which capability and in which session, not what the
  memory says.

`recall` does not check the chains, so a changed record is served like any other
until someone runs `verify`.

Two things narrow this today. Neither closes it.

- On PostgreSQL, the `prevent_event_modification` trigger rejects `UPDATE` and
  `DELETE` on `agent_events`, which stops these edits through ordinary SQL on that
  one table. A role that can drop or disable the trigger is not stopped,
  `memories` and `write_provenance` have no trigger, and DuckDB has none.
- Read-provenance receipts are HMAC'd with a key that is not kept in the store, so
  a record cited in a receipt cannot be changed later without that receipt
  failing, as long as the receipt and the key stay out of reach of whoever can
  write the store.

Checkpoints are not an anchor. They live in the same store, are not signed, and
record no head hash or count. Until 2026-10-25 the audit-tamper report named a
signed checkpoint as a shipped mitigation for tail truncation, and two pages said
`forget` appends a signed delete event. Neither was true. Both are corrected.

### What a user should assume

- Where these docs call a chain tamper-evident, read it as: it catches an edit
  that did not recompute the hashes. It is not evidence against someone with
  write access to the store. [SECURITY.md](../../SECURITY.md) puts that attacker
  out of scope, and the compliance pages say the same.
- To catch that attacker today, keep a copy of the log where the store's writers
  cannot reach it. Export it on a schedule
  ([how](../verify-my-log.md#2-export-the-chain)), keep every export outside the
  store, and check that each new export repeats the previous one record for
  record. Keeping only the last hash is not enough, because the links are
  pairwise.
- The fix has two parts: links that commit to the whole prefix, and a head that
  is signed or published outside the store and checked by every verifier. The
  provenance hash should cover the memory content too. None of it is built yet.

## The MINJA procedure is not measured

**Status: the non-adaptive Phase-3 slice is now measured; the full procedure is
not. Tracked by [#37](https://github.com/sattyamjjain/mnemo/issues/37), targeted
before 1.0.**

**What changed (2026-08-25).** The retrieval-time consequence of *already-shortened*
records is measured against a digest-pinned real embedder:
[`docs/benchmarks/2026-08-25-minja-phase3-nonadaptive.md`](../benchmarks/2026-08-25-minja-phase3-nonadaptive.md).
It reports three nulls, and it is **not a MINJA number** — Phases 1 and 2 are
generative and adaptive and remain unimplemented, which ADR 0003 says is exactly
what makes a result stop being MINJA. Two findings from it belong here:

- **The z-score lane quarantined 0 of 135 poisoned records**, mean z **1.08**
  against a 3.0 threshold. Defense delta **0.0000**. The pre-registered
  prediction in ADR 0003 held.
- **The attack rate is fully explained by topical retrieval.** A benign twin
  matched on opening clause, tags and query restatement is retrieved at the
  *identical* rate (129/135 both), so the poisoning-specific effect is
  **+0.0000**. A 95.6% figure quoted without that floor would badly mislead.

The rest of this section stands unchanged.

mnemo publishes four memory-poisoning benchmarks. **None of them is MINJA**
([arXiv:2503.03704](https://arxiv.org/abs/2503.03704)), and the gap is specific
rather than incidental.

| Bench | What it measures | Why it is not MINJA |
|---|---|---|
| `bench/poisoning` | defense-delta ASR, quarantine on vs off | static MINJA-*style* lexical attacks, not the interactive procedure |
| `bench/salami_poisoning` | compositional save and assembly rates | individually-benign slices; a different attack shape |
| `bench/asi06_poisoning` | cover-up and forgery resistance of the audit layer | detects cover-ups that do not recompute the chain; does not model an attacker corrupting memory |
| `bench/forged_reasoning` | forged chain-of-thought injection ASR | trust-filter evaluation, single-shot |

MINJA is a **procedure, not a corpus**: the attacker only ever issues *ordinary
queries* to the victim agent and corrupts its memory through that legitimate
channel. Nothing above models an interactive, multi-turn, query-only attacker.

### What a user should assume

- **mnemo has no measured resistance to the MINJA procedure.** The 2026-08-25
  measurement above covers Phase-3 exploitation only, non-adaptively, and it
  found no defense: the detector fires on nothing. Do not read the four benches
  below as covering the procedure either. They measure different attacks and say
  so individually.
- **Nothing here says anything about an adaptive attacker** — one who can observe
  the defense and iterate against it. That is a strictly harder threat model and
  it is not measured at all.
- The nearest applicable finding is uncomfortable and is published rather than
  buried: on a real dense embedder, poisoned content sits at roughly **1.5 sigma**
  against a **3.0 sigma** default threshold, so **the z-score outlier lane does not
  catch it** (see [`docs/BENCH_POISONING.md`](../BENCH_POISONING.md)). The raw
  result shows it directly: on the evasive variant the z-score lane leaves ASR at
  **1.0 with the defense on**, unchanged from off
  ([`bench/results/poisoning_real.json`](../../bench/results/poisoning_real.json)).
  The lexical lane does defend against the canonical static variant, **100% to
  0%**, with 0/300 benign false positives.
- If MINJA-shaped attacks are in your threat model, the controls that do apply
  are the write-provenance chain and read-time trust filtering, both of which are
  measured, and neither of which is a poisoning *detector*.

### Why it has not been built

The blocker is a budget, not engineering, and saying so plainly is the point.
MINJA Phase 2 (progressive shortening) is **generative and adaptive**: a faithful
harness needs a model in the loop *as the attacker*. There has been no LLM budget
for that since the issue was filed on 2026-04-24.

An issue whose blocker is funding, filed as though its blocker were engineering,
does not move on its own, and #37 did not move for four months.

### The design is already fixed

[ADR 0003](../adr/0003-minja-procedure-harness.md) is the design of record and
pre-registers the honesty properties **before** any numbers exist: defense-delta
rather than a bare ASR, a mandatory topic-matched benign control arm, Wilson-95
intervals, and a structural success oracle so the headline reproduces without an
LLM judge. It also records the prediction that the z-score lane will probably
catch nothing, so a negative result cannot later be reframed into whatever the
detector does catch.

A reduced, non-adaptive Phase-3-only measurement is scoped and needs no budget,
only time. That is what #37 now tracks. It will **not** be a MINJA number and
must never be labelled one: removing the adaptive shortening step is precisely
what ADR 0003 says makes a result stop being MINJA.
