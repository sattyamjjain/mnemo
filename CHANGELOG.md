# Changelog

All notable changes to Mnemo are documented in this file.

## [Unreleased]

### Landing trace (2026-08-27)

The 0.5.29 window opens on the **v0.5.28** cut. The 0.5.28 release content landed on
`main` in [#173](https://github.com/sattyamjjain/mnemo/pull/173), which is
[`41daa6c`](https://github.com/sattyamjjain/mnemo/commit/41daa6c), and the `v0.5.28` tag
points at the cut commit directly above it, which is where the `## [0.5.28]` heading the
publish gate requires first exists.

### Fixed (2026-08-31) - the README said the current release was both shipped and pending

For four days after v0.5.28 reached crates.io, README.md asserted both states at once.
The generated block said:

> Workspace `[workspace.package].version` (**released**): **`v0.5.28`**

and forty lines below it, a hand-written table said:

> | all **21** published `mnemo-*` crates, including `mnemo-core`, `mnemo-mcp` and **`mnemo-mcp-server`** | `v0.5.27` | `v0.5.28` (unreleased) | one patch, the open release window |

with a second hand-written claim above it — *"Current release: `0.5.28`, cut but not yet
published"* — and a dated preamble, *"As of **2026-08-19** that is one number for every
crate"*, twelve days stale.

Verified live before touching anything, so the fix went to the document rather than to
the truth: crates.io reports `mnemo-core` and `mnemo-mcp-server` at **0.5.28**, PyPI
reports `mnemo-db` at **0.5.28**. The registries were right; the prose was wrong.

**Both hand-written regions are deleted, not updated.** A hand-maintained mirror of a
generated table drifts again on the next release — this one drifted within a day of the
release it described, and the README admitted as much three paragraphs later: *"The
summary table above is written by hand and is a narrative of one moment."*

What the stale region carried that the generated block did not — the **count** of
published crates, the **enumerated list**, and the note that `mnemo-db` ships no code —
is now produced rather than typed. A third generated block, `published-crate-roster`,
derives the count and the list from `cargo metadata` (publishable workspace members less
those with a written never-published exemption) and checks them against the live registry
in **one** bulk query. Twenty-one separate lookups is what got this session rate-limited
earlier, after which the per-crate calls simply hung — and a naive caller would have read
those timeouts as "crate absent".

The generator's own summary line named its blocks by hand and had already drifted: it
printed "(published-versions, python-sdk-compat)" while writing three. It is derived from
the block list now.

### Added (2026-08-31) - a gate for the whole class

`scripts/check_readme_version_claims.sh` (wired into `repo-hygiene.yml` beside
`check_tag_release_parity.sh`) fails when the README asserts release state by hand,
outside the generated markers.

**It is deliberately not "every version literal must equal the workspace version".** That
rule is tempting and wrong: the README legitimately cites past versions, and those are
true statements about the past that must not be rewritten when the workspace moves —
*"in the tag walk as of `v0.5.26`"*, *"`mnemo-mcp-server` had stranded at `v0.5.23`"*.
The existing fence
([`readme_crates_version_matches_workspace.rs`](crates/mnemo-cli/tests/readme_crates_version_matches_workspace.rs))
already pins **bare** in-band literals and exempts `v`-prefixed citations for that reason
— and the stale table slipped straight through it, because every version in that table
was `v`-prefixed.

So the gate targets the thing that actually rots: a present-tense claim about release
state. It also catches the *shape* — any hand-written table row pairing a version with
`crates.io` or `workspace` — so a re-worded mirror is caught too.

Six self-test cases, and **two of them assert the gate stays silent**: on the README's
legitimate historical citations, and on the same banned text inside a generated block.
An over-firing gate gets disabled, which is the same end state as one that cannot fire.
The `[0.5.27]` entry records this family's previous failure — `\t` in `grep -E` is a
literal `t`, so an exclusion matched nothing — which is why this one uses `grep -F` for
the phrase list: `(unreleased)` contains parentheses that an ERE would read as a group,
matching the bare word and firing on the legitimate *"one patch behind an unreleased
workspace"* sentence that describes a guard's behaviour.

Verified against the real README with the exact removed line seeded back in: exit 0 clean,
exit 1 seeded, exit 0 after removing the seed.

### Added (2026-09-01) - a standalone chain verifier, and a worked example that breaks it on purpose

**What the audit found first.** The repository description promises "a SHA-256 hash-chained log
an auditor can verify offline, without trusting the store or the vendor". Before building
anything, here is exactly what already existed, verbatim:

| already shipped | what it is | why it does not satisfy the claim |
|---|---|---|
| `mnemo_core::hash::verify_chain` / `verify_event_chain` | library functions | take `&[MemoryRecord]` / `&[AgentEvent]` — **mnemo types**; you must link `mnemo-core` |
| `MnemoEngine::verify_integrity` | engine method | inside the engine |
| `POST /v1/verify` (mnemo-rest) | REST endpoint | inside the running server |
| `mnemo.verify` (MCP tool) | MCP tool | inside the running server |
| `mnemo_compliance::export_audit_log` / `verify_ndjson_signed` | library functions | **not reachable from the `mnemo` CLI** — only `RetentionProfile` is imported there |
| `python/mnemo/provenance.py` | pure-Python verifier | verifies **HMAC read receipts**, a different artefact from the write chain |
| `bench/audit_conformance` | calls itself "an external verifier" | links `mnemo_core` |

So the outcome was **(b)**: verification existed, but only inside mnemo. The README's
"without trusting the **store**" is true — `verify_chain` is a pure function that never
consults the database. The repository description's "without trusting the **vendor**" was
not: you compiled and ran the vendor's crate to check the vendor's log. And there was **no
export path at all**, so an auditor could not obtain the log without writing Rust first.

**Now shipped:**

- **`tools/verify_mnemo_chain.py`** — one file, standard library only (`hashlib`, `json`,
  `sys`, `argparse`). No network call, no subprocess, no file write. Exits non-zero at the
  first break and prints the record index and both hashes.
- **`mnemo audit export --out chain.jsonl`** — JSONL, every field a string or null, hashes in
  hex, no mnemo types. If the export needed a mnemo type to parse, the format would be wrong.
- **[`docs/verify-my-log.md`](docs/verify-my-log.md)** — a real transcript: write, export,
  verify (passes), edit "24 months" to "6 months" in the file, verify again (fails, names the
  record). Linked from the README's first screen.

The verifier is **deliberately stricter than mnemo's own** in one place. `verify_chain` skips
the link check entirely when a record's `prev_hash` is null, so deleting one field silences
it — the file says "no link here" and is believed. Strict mode calls that a break;
`--mnemo-compat` reproduces mnemo's behaviour so the two readings can be compared. Both
directions are pinned by `crates/mnemo-cli/tests/audit_export_verifies_standalone.rs`, which
shells out to the real Python rather than asserting in Rust: a Rust-side assertion would only
prove mnemo agrees with itself, which is the property an auditor cannot use.

### Fixed (2026-09-01) - the export could not have verified, and concurrent writes do not chain

Two defects the worked example surfaced. Neither was introduced here; both were found by
running the thing end to end instead of describing it.

**1. Tied timestamps made a clean log fail.** `created_at` has microsecond resolution and is
not unique — three quick writes tie, and `ORDER BY created_at ASC` then returns them in an
arbitrary order within the tie. Since each link is defined against the *preceding* record,
that alone makes an untampered chain fail to verify, and an auditor cannot tell that from
tampering. Sorting by `id` does not help: the ids are UUID-v7 and inside one timestamp the
random tail dominates, giving a third order that is also not write order. The exporter now
resolves **ties only** by following the chain links, which is the one thing in the data that
records write order. That cannot launder a tampered log — an edited `content` leaves the
hashes untouched and is still caught, and a removed record leaves a tie group that cannot be
linked, which the exporter reports and the verifier still fails.

**2. Concurrent writes are not chained at all.** `remember()` reads the current chain head and
then inserts. Three `tools/call` requests in one MCP session are processed concurrently, all
three read the same head, and all three are written as chain **heads** — `prev_hash =
SHA256(content_hash)` with no predecessor. Verified directly: three records written in one
session produced three heads and zero links between any pair, while the same three written one
per invocation produced `OK: 3 records, chain intact`.

`crates/mnemo-core/src/query/remember.rs` has always said so — *"Concurrent writes for the
same agent_id may race on prev_hash lookup"* — but nothing measured it. The standalone
verifier is what made it visible, which is a point in favour of the artefact existing.

**Not fixed in that change set**, deliberately: serialising the write path is a concurrency
change to the engine with its own risk, and it was not what that change set was. It is fixed
in the entry below.

No compliance claim is made anywhere in this change. mnemo produces a tamper-evident,
independently verifiable event log; mapping that to an obligation is the reader's to do.

### Fixed (2026-09-02) - concurrent writes now chain, and the head was being found the wrong way

The defect deferred above. Reproduced first, as a failing test, then fixed —
`crates/mnemo-core/tests/concurrent_chain_linkage.rs` fires 16 concurrent `remember()` calls
at one `(agent_id, thread_id)` and asserts the resulting rows form a single chain with exactly
one head. The assertions are structural rather than statistical: one head, and every other
record reachable from it by following links, once. A fork fails. A missing record fails. There
is no threshold to tune and no flake budget.

**Measured before:** 16 heads, 0 links, on all three chains it checks — the unthreaded event
chain, the thread-scoped event chain, and the memory chain. (The earlier note recorded 3 heads
and 0 links from a real MCP session; 16 writers is the same defect with more of it.)

**Measured after:** 1 head, 15 links, no forks, on all three.

**Two causes, and the second only became visible once the first was fixed.**

*1. The read of the chain head and the insert were not atomic.* `remember()` read the head at
the top and inserted about eighty lines later, with TTL resolution, record construction,
opaque-reasoning detection and encryption in between. Any two calls overlapping that window
read the same head, and each wrote itself as a fresh head.

Fixed with one new pair of trait methods —
`StorageBackend::append_memory_chained` / `append_event_chained` — that own the whole
read-link-insert sequence, so the racy shape is no longer expressible at a call site. Two
implementations, because the two backends genuinely need different mechanisms:

- **DuckDB — design (a), a sharded in-process lock** keyed on `(chain, agent_id, thread_id)`.
  Not a compromise for the embedded backend but the complete answer: DuckDB takes an exclusive
  file lock for read-write access, so a database has at most one writing process and every
  appender is a task inside it. Sharded (64 slots) rather than a `HashMap` of per-key locks,
  which grows without bound and needs an eviction protocol that has to prove nobody holds the
  lock it is dropping. A **global** lock was rejected outright: it would serialise every agent
  in the process, which is a worse product than the defect.
- **PostgreSQL — design (b), `pg_advisory_xact_lock`** on a stable 64-bit key derived from the
  chain, inside the transaction that does the read and the insert. An in-process lock is
  useless here: a PostgreSQL database is reached by however many mnemo processes the operator
  runs, and ordering the appends inside one of them would make the race *rarer* without making
  it absent — which is the worst of both, because it then survives testing. The advisory lock
  is released by COMMIT or ROLLBACK, including the rollback a dropped transaction performs, so
  no path leaks it. Preferred over an optimistic compare-and-swap plus bounded retry: the retry
  bound is a number that is either too small under contention or too slow when it fires.

*2. The chain head was being found with `ORDER BY <timestamp> DESC LIMIT 1`, which is not the
tip.* With the lock in place the head count went from 16 to 1 — and the chain still forked, 2
to 5 records claiming the same predecessor. A record's `created_at` / `timestamp` says when the
write *started*, not when it was inserted; a write that begins earlier can be inserted after
one that begins later, and from that moment the largest-timestamp row is no longer the tip.
Every subsequent append then links to that stale row.

This was **not** a clock-resolution or tie-breaking problem, which is what the shape of it
suggests: all 16 timestamps were distinct in the runs that forked, and the test reports that
count on failure so the next person does not spend the same hour on the same wrong hypothesis.

Fixed with a `chain_heads` pointer table — one row per `(chain, agent_id, thread_key)`,
written inside the same critical section as the insert it describes, read back as a point
lookup with nothing to order and nothing to tie. Additive on both backends (DuckDB persistence
version 6 → 7; PostgreSQL `CREATE TABLE IF NOT EXISTS`): an existing database gains an empty
table, and the first append for each key seeds the tip from the old timestamp-ordered lookup,
so nothing is backfilled and nothing already written changes meaning. Deleting a record that is
the current tip clears the pointer, so the next append re-seeds from the live-filtered lookup —
which is what the pre-`chain_heads` code did after a `forget`, and without it a soft delete of
the tail would leave the next write linking to a record no longer in the live chain.

**Verified by mutation, in the failing direction, not by inspection.** Removing the DuckDB lock
reproduces 16 heads and 0 links. Removing the `chain_heads` seed makes the first append after
an upgrade start a second chain instead of extending the existing one. Removing the PostgreSQL
advisory lock, against a live database, gives 9 heads and 7 links. Each is a distinct test that
fails for a distinct reason.

**PostgreSQL is exercised against a live database, not asserted.**
`crates/mnemo-postgres/tests/concurrent_chain_linkage_pg.rs` is the parity half — the two
backends fix this with different mechanisms, so the DuckDB result proves nothing about
PostgreSQL. It skips loudly without `MNEMO_TEST_POSTGRES_URL` and is wired into the existing
`postgres` CI job, which is the only place it runs. It does **not** prove ordering across
*processes*: every task shares one pool in one process. What it proves is that the ordering
comes from the database — there is no in-process lock in the PostgreSQL backend for these tasks
to be accidentally serialised by.

**A third defect, found by pointing the verifier at the result.** With the chain correct in the
database, `mnemo audit export` still produced a file the standalone verifier rejected —
`BROKEN at record index 4`, on an untampered log. The exporter emits in `created_at` order and
repaired only *tie groups* by following chain links, which was the right fix for the defect it
was written for. Concurrency is not a tie: a write that starts earlier and is inserted later
puts the two orders permanently out of step, with every timestamp distinct. `order_by_chain`
now walks the whole chain from its head, with a linear fast path for a log that is already in
order (which is every serially-written log, so nothing pays for this that does not need to).
Records unreachable from the head — what a removal leaves behind — are still counted, reported
to stderr, and emitted, because a shorter file that verifies is the one thing an audit export
must never produce.

**One deliberate scope change.** Event appends now use a single chain per agent, where a
threaded write previously linked to the last event *in its thread*. That is what the only
event-chain verification path in the tree checks — `verify_event_integrity(agent, None)` walks
`list_events`, which returns every event for the agent regardless of thread — and the previous
split made that walk fail on any agent that used more than one thread. Nothing in the tree
verifies a per-thread event chain. Memory chains remain per-`(agent_id, thread_id)`, where the
write path and `list_memories_by_agent_ordered` already agreed.

Every append site is routed through the new methods: `remember`, `forget` (delete and redact),
`recall`, `share`, `merge`, `branch`, `checkpoint`, `consolidate`, `conflict`, `lifecycle`
(consolidation and the TTL sweep), `reflection` (three sites), the CLI, and `mnemo-amp`.
`event_builder::build_event` no longer computes `prev_hash` at all — it is assigned at
insertion time — so pairing it with a bare `insert_event` now stores a null link, which the
standalone verifier's strict mode reports.

**Still true, and out of scope:** the REST OTLP ingest path writes `agent_events` rows with
`prev_hash: null`, outside the chain. It did so before this change too. mnemo's own
`verify_chain` skips the link check on a null `prev_hash`, so those rows pass it silently while
strict mode calls them a break — the disagreement `--mnemo-compat` exists to surface. Folding
telemetry ingest into the audit chain is a different decision with different consequences and
is not made here.

### Added (2026-09-02) - the reproduction cannot quietly stop running

`ci.yml` runs the concurrency reproduction as its own named step on every PR, on top of the
workspace test run that already covers it. The point is not to run it twice: it is that
deleting the file or marking a test `#[ignore]` would leave the workspace run green, and "the
check is gone" and "the check passes" look identical from a summary line. The step parses the
test-result line and fails if any test is ignored or if fewer than three ran. Its parser was
run against the three result lines those cases produce — `1 ignored`, `0 passed`, and cargo's
"no test target named …" — and is red on all three.

`README.md` gains one line of scope on the 256-trial audit-conformance figure: that bench
writes its chain one record at a time, so the number covers mutation of a serially-written log
and says nothing about concurrent writes.

That step went red on its own pull request, twice, and the second failure corrected the
diagnosis of the first.

It originally captured cargo's output into a variable under `set -e`, so a non-zero exit
aborted the step *at the assignment* and the `echo` below it never ran: CI reported
`exit code 101` and nothing else. A guard that hides the evidence it exists to produce is worse
than no guard, so the first fix made it print before evaluating — and that is the only reason
the real cause was ever visible.

The real cause was not a failing test. It was **disk**: `ar: unable to copy file
'…/libduckdb.a'; reason: No space left on device`. Invoking cargo a second time with a
different target filter gives `libduckdb-sys` a fresh fingerprint, and a second from-source
DuckDB build does not fit beside the artifacts the first one already wrote. Matching the
package selection was not enough — the target filter alone re-fingerprints it.

So the step no longer runs cargo at all. The workspace run tees its output to a file and the
step asserts that each of the three tests appears there as `... ok`. That is cheaper, it cannot
exhaust the runner, and it is strictly better evidence: it checks the run whose result the job
actually reports, rather than a second run that might differ from it. Verified in every failing
direction — a test `#[ignore]`d, the file deleted, a test renamed, and a test present but
failed all turn the step red.

### Changed (2026-09-02) - the README band fence no longer fails on its own generated block

`readme_current_band_version_literals_match_workspace` pins every bare in-band version literal
in the README to the workspace version. The version bump made it fail on the
`published-crate-roster` generated block, which correctly lists each crate's published
`0.5.28` while the workspace has moved on — so the fence would have failed on every release
window from here.

Generated regions are excluded, by structure rather than by an allowlist entry: the fence
exists to catch hand-written prose that has gone stale, and a generated block is re-derived
from the live registry on every run. `scripts/check_readme_version_claims.sh` separately
requires that release state is asserted **only** inside those markers, so the two guards
together still leave no gap. Verified by putting a stale hand-written literal outside the
markers — the fence stays red.

## [0.5.28] - 2026-08-27

### Landing trace (2026-08-25)

The 0.5.28 window opened on the **v0.5.27** cut. The 0.5.27 release content landed on
`main` across [#170](https://github.com/sattyamjjain/mnemo/pull/170),
[#169](https://github.com/sattyamjjain/mnemo/pull/169),
[#168](https://github.com/sattyamjjain/mnemo/pull/168),
[#171](https://github.com/sattyamjjain/mnemo/pull/171) and
[#172](https://github.com/sattyamjjain/mnemo/pull/172), the last of which is
[`14ba03f`](https://github.com/sattyamjjain/mnemo/commit/14ba03f), and the `v0.5.27` tag
points at the cut commit directly above it, which is where the `## [0.5.27]` heading the
publish gate requires first exists.

### Fixed (2026-08-26) - every published crate now has its crates.io metadata

`mnemo-mcp-server` - the crate the README tells people to `cargo install`, twice - shipped
through v0.5.27 with `homepage=None documentation=None repository=None keywords=[]
categories=[]`. A crates.io page with no repository link is a dead end, and nobody
searching "mcp memory" finds a crate with no keywords. Nothing was red, because nothing
was looking.

An audit found **all 23** publishable members incomplete, six missing `repository`
outright: `mnemo-admin`, `mnemo-grpc`, `mnemo-mcp-server`, `mnemo-pgwire`,
`mnemo-postgres`, `mnemo-rest`.

All three URLs already existed in `[workspace.package]` and simply were not inherited.
`documentation` is included because cargo does **not** default it to docs.rs in published
metadata - an absent value is an absent link. Category slugs were validated against the
**live** crates.io category list rather than guessed: an unknown slug is rejected at
publish time, so a guess would have failed the release instead of the check.

`mnemo-mcp-server` also gains a README, since it is the install target and had no page
content at all. It documents the name split (`mnemo` and `mnemo-cli` on crates.io both
belong to other authors) that the existing CI guard already protects.

`scripts/check_publish_closure.sh` now asserts the metadata, because that is what stops
this recurring. Two details:

- **`keywords = []` is treated as missing.** An empty array is exactly as blank as an
  absent key on the page, and it is the state `mnemo-mcp-server` was actually in.
- **The first version of the check reported clean over a checker that had crashed.** A
  `SyntaxError` in its own embedded python was swallowed by `|| true`, and the guard
  printed "every publishable member declares ..." having examined nothing. It now fails
  loudly on a non-zero exit, and the self-test asserts a field no crate has so the check
  can never become vacuous.

Verified by mutation: stripping keywords fails, `keywords = []` fails, and breaking the
checker fails rather than reporting clean.

### Changed (2026-08-26) - the real-embedder numbers are CI-reproducible, and now say so

The README carried two claims sourced from [#125](https://github.com/sattyamjjain/mnemo/issues/125),
which closed on 2026-08-04. Both were resolved by **measurement, not by deleting the
reference**.

**One was already false.** README:165 said the poisoning bench is not CI-reproducible
*"until the `ort` integration is repaired"*. That repair is what #125 was, and `ci.yml`'s
`onnx-feature` job has built, linked and run **end-to-end ONNX inference against a
downloaded model** on every push since. The claim had outlived its own stated cause.

**The other was narrower and genuinely outstanding**: a *model-fetch bench job* did not
exist. It does now -
[`.github/workflows/real-embedder-benches-nightly.yml`](.github/workflows/real-embedder-benches-nightly.yml)
fetches the digest-pinned checkpoint, regenerates **both** headline numbers, compares
against the committed artifacts within a stated band, and opens an issue on drift rather
than failing into a mailbox.

The prose was not changed until that job had actually run. It did, on an x86-64 Linux
runner, against numbers first measured on arm64/darwin, and reproduced them **exactly**:

| number | committed | regenerated | delta |
|---|---:|---:|---:|
| locomo `semantic` recall@1 | 0.6890 | 0.6890 | +0.0000 |
| locomo `semantic` recall@10 | 0.9110 | 0.9110 | +0.0000 |
| locomo `lexical` recall@1 | 0.4220 | 0.4220 | +0.0000 |
| poisoning MINJA (canonical) ASR off -> on | 1.0000 -> 0.0000 | 1.0000 -> 0.0000 | +0.0000 |

The comparison also treats a **disappeared attack** as drift: a bench that silently stops
measuring something the README cites is not "no change".

The two remaining #37 citations (README:151, README:196) were checked and left alone -
they cite it as scope and provenance ("the compositional subset of #37"), not as
outstanding work, and that is accurate.

## [0.5.27] - 2026-08-25

### Landing trace (2026-08-22)

The 0.5.27 window opened on the **v0.5.26** cut. The 0.5.26 release content landed on
`main` across [#163](https://github.com/sattyamjjain/mnemo/pull/163),
[#164](https://github.com/sattyamjjain/mnemo/pull/164),
[#165](https://github.com/sattyamjjain/mnemo/pull/165) and
[#166](https://github.com/sattyamjjain/mnemo/pull/166), the last of which is
[`34e3550`](https://github.com/sattyamjjain/mnemo/commit/34e3550), and the `v0.5.26` tag
points at the cut commit directly above it, which is where the `## [0.5.26]` heading the
publish gate requires first exists.

### Verified (2026-08-24) - the last release published completely, 21/21 crates

Before changing anything, the registry was asked directly rather than the repo. Every
publishable workspace member was checked against
`https://crates.io/api/v1/crates/<name>`:

- **21 of 21** crates that ship to crates.io are at **0.5.26**, `mnemo-mcp-server`
  included. There is **no partial publish**, so the `v0.5.26` tag is not lying about what
  shipped, and nothing needed re-publishing or yanking. The workspace stays at 0.5.26 and
  was repaired in place rather than re-cut.
- The two `cargo metadata` reports as publishable that return 404 are both deliberate and
  both already carry a written exemption in `scripts/check_publish_closure.sh`:
  `mnemo-python` (a maturin wheel, published to **PyPI as `mnemo-db`**, never to
  crates.io — PyPI confirms `mnemo-db` 0.5.26) and `mnemo-golem-host` (excluded from the
  CI workspace build, so CI cannot build it, let alone publish it).

### Fixed (2026-08-24) - a release made `main` red, by construction

CI on `main` was failing at `3b876fc` with
`README generated block(s) STALE: published-versions, python-sdk-compat`.

The guard was right and the repo was wrong. Publishing **moves the registry**, and those
README blocks are generated *from* the registry — so a successful release always left the
committed table one version behind, and the next scheduled run went red. The symptom was
one stale table; the cause was a loop the release opened and nothing closed.

- `release-crate.yml` gained a `refresh_generated_docs` job that waits for crates.io to
  actually report the new version (regenerating against a stale read would write the old
  version back), regenerates both blocks, refuses to push if anything other than
  `README.md` changed, and commits to `main`.
- Regenerating also **deleted a claim that had become false**: the README told readers
  `pip install mnemo-db` and `cargo add mnemo-core` "do not currently resolve the same
  version". They now both resolve 0.5.26.

### Added (2026-08-24) - a tag can no longer publish a red commit

crates.io publishes are permanent, and nothing checked whether the commit under a tag had
passed. `release-crate.yml` now runs `commit-is-green` **first**, and everything else
needs it.

Three things it had to get right, each verified against real commits rather than by
reading:

- **The required workflow is asked for by file name** (`ci.yml`), through the
  workflow-runs API. Check-run names are *job* names, so matching `"CI"` against them
  silently matches nothing — and a workflow that never triggered would then read as "no
  failures" rather than as a failure.
- **The most recent run decides it.** `3b876fc` has one green push run and two later red
  scheduled runs; an "any run succeeded" rule would have waved a tag straight through a
  repo that was red at that moment.
- **`cargo-publish` is excluded, with a reason.** Its `plan` job fails *by design* on any
  commit whose workspace version has no tag yet — which is every commit between a version
  bump and its tag. Including it would have blocked every release this gate exists to
  allow. `release-crate` is excluded too, or the gate waits on itself forever.

Verified in the failing direction: it refuses `3b876fc` (CI red), refuses `3718da3`
(`benchmarks-nightly` red), and accepts `34e3550`. An `allow_red_commit` input exists for
a deliberate override and records the choice in the run log.

### Added (2026-08-24) - every release tag must have a GitHub Release

Ported from ferrumdeck's `tag-has-release`. `scripts/check_tag_release_parity.sh` (with
`--self-test`, 7 cases) asserts that every tag from `v0.5.4` forward has a Release —
v0.5.23/24/25 each shipped without one and nothing went red. Nine tags predate the
automation and are exempt by a **dated cutoff with a stated reason**, printed on every run
so they stay visible, not by an open-ended allowlist. The self-test pins the two ways this
guard could quietly stop working: string comparison exempting `v0.5.10` because it sorts
below `v0.5.4`, and the cutoff tag itself falling outside the checked set.

### Fixed (2026-08-24) - the recall claim is a paired comparison now, and says so

The README stated recall@1 **0.689** [0.543, 0.805] against a lexical control of **0.422**
[0.290, 0.567] at n=45. Those intervals **overlap** — 0.543 sits below 0.567 — and the
page presented them as if that settled the comparison. It does not: overlap neither
establishes a difference nor rules one out.

Worse, the committed result recorded only the marginals, and **the marginals alone cannot
decide it**. They fix `b − c = 12` while saying nothing about `c`, and both ends of the
feasible range are consistent with those same two numbers:

| discordant split | exact McNemar p | verdict at 0.05 |
|---|---:|---|
| `c = 0`, `b = 12` | 4.9e-4 | significant |
| `c = 16`, `b = 28` | 0.096 | **not** significant |

So the paired data was not a nicety; it was required. `locomo_v1_bench` now keeps the
per-query rank-1 outcome vector it used to discard and reports two paired comparisons,
each with a bootstrap interval over queries (fixed seed, recorded in the result) and an
exact McNemar test:

| comparison | paired Δ recall@1 | 95% CI | McNemar b/c | exact p | separates? |
|---|---:|---|---:|---:|---|
| `semantic` − `lexical` | **+0.267** | [0.133, 0.400] | 12 / 0 | 4.9e-4 | **yes** |
| `semantic` − `auto` | +0.058 | [−0.031, 0.160] | 4 / 2 | 0.69 | no |

The headline holds, and now holds for a stated reason: 12 queries won, **zero** lost, and
the interval on the gap excludes zero. The re-run used the same checkpoint as the
committed result (sha256 `759c3cd2b7fe…`, verified before use) and reproduced both
marginals **exactly**.

The second row is a correction. `docs/benchmarks/locomo-v1.md` called the
`auto`-vs-`semantic` gap "suggestive, not significant" *by eyeballing interval overlap* —
the same move, one page over. It genuinely does not separate, which is now measured
(p=0.69) rather than guessed, and it would take roughly **n=127** to resolve.

The README's own n=23 note made the mirror-image error, reading "statistically
indistinguishable" off an overlap; it now says what can actually be said, which is that no
paired statistic exists between two separate runs on different corpora.

Both intervals and the `preliminary` (n<100) marking are kept. The paired verdict is
stated **in the same sentence as the number**, at the top of the README and in the block,
both generated from the result file — the hand-typed restatement that used to sit fifteen
lines above the generated block is gone. `--self-test` (13 cases) covers the branch that
does not run today: when the gap *fails* to separate, the README must say so and say what
n would be needed, and a mutation that makes it claim a win instead fails three checks.

### Added (2026-08-25) - issue #37: non-adaptive Phase-3 exploitation, measured

Closes [#37](https://github.com/sattyamjjain/mnemo/issues/37) at the reduced scope its
2026-08-18 comment fixed: Phase-3 exploitation only, non-adaptive, against a
pre-registered fixed corpus of already-shortened records.

**This is not a MINJA number and the artifact says so.** [ADR
0003](docs/adr/0003-minja-procedure-harness.md) states that removing the adaptive
shortening step is exactly what makes a result stop being MINJA. Phases 1 and 2 are
generative and need a model in the loop as the attacker; they remain out of scope pending
an LLM budget. What is measured is the retrieval-time consequence of records that are
already shortened, which is a strictly weaker claim.

**The number**, against a digest-pinned `Xenova/all-MiniLM-L6-v2` (384-dim, sha256
`759c3cd2b7fe…`), 45 distinct victim queries x 3 seeds = 135 trials, k=10:

| | rate | 95% CI | n |
|---|---:|---|---:|
| poisoned record exploited, detector **OFF** | **0.956** | [0.906, 0.980] | 129/135 |
| poisoned record exploited, detector **ON** | **0.956** | [0.906, 0.980] | 129/135 |
| **benign floor** (matched twin), detector OFF | **0.956** | [0.906, 0.980] | 129/135 |
| benign false-quarantine, detector ON | 0.000 | [0.000, 0.028] | 0/135 |

Conservative interval at the distinct-query denominator (the independent unit is the
query, not the trial): **[0.852, 0.988]**, n=45.

Three findings, all nulls, all published with their denominators:

1. **The z-score lane does not defend against this.** 0/135 quarantined; mean poison
   z-score **1.08** against the 3.0 default. Defense delta **0.0000**. ADR 0003
   pre-registered this prediction before any code existed, and it held at a lower sigma
   than predicted. The headline is the negative result, not a reframing to whatever the
   detector does catch.
2. **The attack rate is entirely explained by topical retrieval.** The benign twin —
   matched on opening clause, tags and restatement of the victim query, differing only in
   that it resolves to no answer — is retrieved at the *identical* rate. Poisoning delta
   **+0.0000**. Reporting "95.6% ASR" without that floor would have been badly
   misleading, which is why the floor is mandatory rather than decorative.
3. **The null is not an artifact of a saturated oracle.** At k=10 both arms miss on the
   same two queries, so the top-k measure is at ceiling. Re-measured at k=1, where it is
   not: poison **0.756** vs benign floor **0.778** — the poisoned record is retrieved
   *less* often than its twin. No k in {1, 3, 10} shows a poisoning advantage.

The 0/135 false-quarantine rate is **not** evidence of a well-calibrated detector. It is
0 because the detector fires on nothing at all.

**Committed as an artifact, not a README sentence.**
[`bench/results/minja_phase3.json`](bench/results/minja_phase3.json) carries all 540
per-trial records, the seeds, the model digest, the corpus hash, the fixture hash and the
exact regeneration command. Write-up with a "what this does not establish" section:
[`docs/benchmarks/2026-08-25-minja-phase3-nonadaptive.md`](docs/benchmarks/2026-08-25-minja-phase3-nonadaptive.md).

**Fail-closed model pinning.** `bench/locomo/src/pinned_model.rs` verifies the weights
against a digest committed in source and **refuses to start** on a mismatch — a path is
not a pin, because two checkpoints can sit at the same path and produce different numbers.
An *absent* expected digest is also an error: "nothing to compare against" must not read
as "comparison passed".

**Drift.** `bench/minja_phase3/tests/committed_result_is_within_band.rs` checks the
committed artifact offline on every CI run (+/-0.05 on rates; the quarantine count and
defense delta are compared **exactly**, because they are structural rather than
statistical). `.github/workflows/minja-phase3-nightly.yml` regenerates against the real
model nightly and **opens an issue** on drift rather than failing into a mailbox nobody
reads. Six mutations of the artifact were each verified to trip the guard.

Two corrections the fixture tests caught during development, both real:

- the benign twin originally omitted the victim query while the poison restated it, so the
  delta would have measured "mentions the query" rather than "asserts a competing answer";
- the target-answer rotation handed `c01-t02` a payload that is a **substring of its own
  victim query**, which every query-restating record then contains for free — including
  the benign twin, collapsing the one distinction between the arms.

### Verified (2026-08-25) - Postgres semantic recall already fails loud

Checked before writing any code, per the brief. The silent-empty stub is **not present**
and has not been since v0.5.13: `crates/mnemo-core/src/query/recall.rs` fails loud on all
four semantic legs, `mnemo-postgres`'s pgvector index returns the typed
`Error::BackendUnsupported`, and `crates/mnemo-postgres/tests/semantic_recall_fails_loud.rs`
pins the caller-visible contract with a non-empty fixture and a control asserting the store
really holds a record. 3/3 pass. No change was needed; DuckDB is documented as the
supported backend for this measurement in the bench, the write-up and the result file.

## [0.5.26] - 2026-08-22

### Landing trace (2026-08-18)

The 0.5.26 window opens on the **v0.5.25** cut. The release content landed on `main` via
[#162](https://github.com/sattyamjjain/mnemo/pull/162), whose branch head was
[`a8363e6`](https://github.com/sattyamjjain/mnemo/commit/a8363e6), and the `v0.5.25` tag
points at the resulting commit on `main`, which is where the `## [0.5.25]` heading the
publish gate requires first exists.

### Fixed (2026-08-22) - benchmarks-nightly could never have passed, and was failing into a void

The nightly benchmark gate had failed every night since at least 2026-08-14, across five
different commits. Nobody was reading it. Three separate defects, and the first was
masking the other two.

- **`maturin develop` needs a virtualenv and the runner has none.** The job died at step
  four with *"Couldn't find a virtualenv or conda environment"* before reaching any
  benchmark. Replaced with `maturin build` plus `pip install --no-index --find-links`,
  which needs no venv.
- **`pip install 'mnemo[benchmark]'` would have installed a stranger's package.** PyPI
  `mnemo` is *"Notebook and assistant"* 0.0.2 by a different author; this project
  publishes as `mnemo-db`, and `mnemo-db` defines no `benchmark` extra either. The only
  reason nothing foreign was ever pulled into a credentialed CI job is that the maturin
  failure above stopped the run first. Now installs the real dependencies by name.
- **It needs three secrets the repository does not have.** `ANTHROPIC_API_KEY`,
  `OPENAI_API_KEY` and `HF_TOKEN` are all absent; the repo has exactly two secrets,
  `CARGO_REGISTRY_TOKEN` and `NPM_TOKEN`. So even with the first two defects fixed the
  job could not pass. It now **skips deliberately** with the reason in the run summary,
  because a scheduled job incapable of succeeding trains everyone to ignore a red
  nightly, which is the state that hides a real regression. `workflow_dispatch` still
  runs unconditionally, so an operator who has just added the secrets can prove it.

One trap worth recording: the credential check lives in job-level `env`, not job-level
`if`. The `secrets` context is **not available** to `jobs.<job_id>.if` (only `github`,
`needs`, `vars`, `inputs`), so a secrets test written there would silently never be true
and the job would run and fail exactly as before.

### Fixed (2026-08-22) - the name guard had a test that exercised a different regex engine

Extending `check_crate_name_refs.sh` to cover PyPI and workflows exposed a defect in the
guard itself, and it is the more important half of this entry.

- **The scanner matched with `awk`; the self-test matched with `grep`.** macOS awk
  (onetrue-awk 20200816) silently fails to match the new pip pattern on a line grep
  matches without hesitation. The self-test reported **30 green assertions over a scanner
  that could not see the very line it was written for**. A guard whose test runs a
  different engine from the guard is not a tested guard, it is a guard with a decorative
  test. Both scanners now match with `grep -E`; awk only tracks markdown fence state.
- Fixing that immediately surfaced **two more real hits** the broken matcher had missed:
  `docs/benchmarks/2026-04-21-mnemo-v0.3.0.md` and `2026-04-24-mnemo-v0.3.3.md` both told
  a reader to `pip install "mnemo[benchmark]"` in a fenced *"how to run locally"* block.
  Corrected, along with the `maturin develop` line in each, which had the same
  missing-virtualenv problem the nightly did.

The guard now covers **PyPI as well as crates.io**, and **workflows as well as docs**.
Workflow lines are treated as live commands rather than prose, with `#` comment lines
skipped for the same reason unfenced markdown is skipped: `ci.yml` has to be able to name
the wrong command in a comment in order to explain the guard. Verified in the failing
direction against the exact line that was live in CI.

### Fixed (2026-08-22) - rustdoc emitted a broken intra-doc link

`mnemo-compliance` generated one rustdoc warning: `[`bench/retention_conformance`]` is a
repository path, not a resolvable item, so rustdoc rendered a broken link on a published
crate's docs page. Now a plain code span. `cargo doc -p mnemo-compliance` is warning-free.

### Fixed (2026-08-21) - the README claimed a Python SDK version that was 14 releases stale

README prose read *"Its current release is `mnemo-db` 0.5.12"* and then reasoned about wire
compatibility from that premise. PyPI has served **0.5.26** since 2026-08-19.

**The argument did not survive the correction; it had inverted.** The old text said the
wheel "does **not** include engine changes from `v0.5.13` onward (for example, the v0.5.17
forged-reasoning recall defense)". It includes all of them. Worse, the premise underneath
was wrong in its own right: the paragraph asserted the Python SDK "versions
independently", which `workspace_version_fence.rs::python_sdk_version_matches_the_workspace`
has contradicted since it was written. That fence exists **because of this exact drift**,
and its own comment says so: *"It had drifted exactly as you would predict from a comment
claiming the opposite: `pyproject.toml` said 0.5.12 while the compiled core inside the same
wheel was 0.5.23."* The fence was added; the README sentence that caused it was not.

- **The paragraph is now generated**, not typed, from live PyPI plus the workspace version,
  between `<!-- BEGIN generated: python-sdk-compat -->` markers. A number and the argument
  that depends on it are produced together or they go stale together, so generating the
  table while leaving the prose hand-written only moved the staleness somewhere less
  visible.
- **It states what is actually true:** `mnemo-db` is PyO3 bindings that compile
  `mnemo-core` into the wheel, so the wheel version names the engine inside it and the
  fence keeps them equal. `mnemo-db 0.5.26` *is* `mnemo-core 0.5.26`; there is no
  version-skew question to answer.
- **And a fact the old paragraph could not have carried:** `pip install mnemo-db` and
  `cargo add mnemo-core` do not currently resolve the same version (PyPI `v0.5.26`,
  crates.io `v0.5.25`), because the wheel publishes on merge to `main` while the crates
  publish on a tag. Generated, so it corrects itself when the tag lands.
- The generated table's own header carried the same false claim and is fixed with it. Only
  the TypeScript SDK versions independently.

### Added (2026-08-21) - the doc guards run on a schedule, because registries change without a push

`gen_published_versions.py --check` only ran on push and pull_request, so a generated block
was never fresher than the last commit. A publish landing minutes after a merge left the
README wrong with nothing red until somebody happened to push. That is how `0.5.12` sat in
the README while PyPI served `0.5.26`.

CI now also runs on a daily `schedule`. The nine compile-heavy jobs carry
`if: github.event_name != 'schedule'`, so the nightly runs only the two checks that read
live registries, `doc-guards` and `version-drift`. Recompiling the workspace daily to
re-read PyPI would be a lot of CI for no signal. The cron is at `17 7` rather than a round
hour, since every scheduled workflow on the platform fires at `:00`.

### Fixed (2026-08-21) - the published-crate count was wrong, and four crates disclosed nothing

- **"all 20 published `mnemo-*` crates" was 21.** Enumerated from crates.io and now listed
  by name, with the note that one of them, **`mnemo-db`, ships no code**: it is a defensive
  name reservation whose entire contents are a doc comment. It is counted because it is a
  real artifact someone can `cargo add`, and they should learn that from the count rather
  than from an empty crate. That table also had a three-column header over a four-cell row;
  fixed.
- **`mnemo-mesh`, `mnemo-deal`, `mnemo-baseline` and `mnemo-cma` had no README at all**, so
  a visitor to crates.io saw only the one-line description. All four are published at
  `0.5.25` on a public registry while the workspace README's own enforcement table marks
  them *"standalone adapter crates, not invoked by the running server"*. Somebody who finds
  the crate on crates.io never sees that table. Each now carries that disclosure at the top
  of its own README, plus `readme = "README.md"` in its manifest, verified present in
  `cargo package --list`.
- **`ConsentTokenGuard` documented the opposite of what it does.** Its doc comment said the
  guard "refuses anything missing / expired / wrong-scope BEFORE the engine sees the data",
  which reads as wired in. The enforcement table marks it *"library only, core engine never
  calls it"*, and it is stronger than that: **`mnemo-core` does not depend on
  `mnemo-compliance` at all**, so there is no code path by which a `remember` could reach
  this guard. For a compliance control, a doc that implies enforcement it does not have is
  the worst kind to leave. The type now says so, with the call pattern an operator must
  write themselves.

**These land on crates.io only at the next publish.** The four crates and
`mnemo-compliance` are at `0.5.25` on the registry; the READMEs and the doc comment ship
with `0.5.26`.

### Changed (2026-08-21) - one headline recall number, and the older one keeps its interval

The README carried two real-embedder recall figures about sixty lines apart, on different
embedders and different corpora. It warned they must not be conflated, which does not help
a reader skimming for a number: they take whichever they meet first.

- **The MiniLM measurement is the single headline** (n=45, recall@1 **0.689**
  [0.543, 0.805], lexical control 0.422 [0.290, 0.567] on the same corpus and harness). It
  keeps its `preliminary (n<100)` marker.
- **The nomic-embed-text measurement moves below it**, into a collapsed *"earlier
  measurement, different embedder and corpus"* block, and **gains the Wilson 95% interval
  it never had**: recall@1 0.739 **[0.535, 0.875]** at n=23, recall@5 0.826 [0.629, 0.930].
- **Computing that interval is what makes the restructure worth doing.** [0.535, 0.875] is
  wider than the headline's [0.543, 0.805] and *fully overlaps* it: the two runs are
  statistically indistinguishable, so the higher point estimate is not evidence that the
  768-dim embedder is better, only that n=23 buys very little resolution. Stated in the
  block, because a reader comparing 0.739 against 0.689 will otherwise draw the wrong
  conclusion from two numbers that do not support one.
- A later section still called the nomic figure *"The canonical, reproducible number"*,
  which directly contradicted the new headline. Corrected, and the bare point estimate in
  the vendor-comparison table now carries its interval too.

### Added (2026-08-21) - the MINJA gap is a documented limitation, and #37 is retargeted

New **[`docs/security/known-limitations.md`](docs/security/known-limitations.md)**, linked
from the README beside the benchmark index. It is the mirror of that page: things a reader
might reasonably assume mnemo measures or enforces and it does not.

It states that **mnemo has no measured resistance to MINJA-style progressive memory
poisoning**, tabulates why each of the four existing poisoning benches measures something
else, and repeats the uncomfortable adjacent result rather than leaving it in a JSON file:
on a real dense embedder the z-score lane leaves ASR at **1.0 with the defense on**,
unchanged from off.

[#37](https://github.com/sattyamjjain/mnemo/issues/37) was labelled `target:v0.5.x` when
filed on 2026-04-24; the project has since shipped `v0.5.22` through `v0.5.26`. The label
was still technically true, which is the problem: it cannot become false, so it stopped
carrying information. Retargeted to **`target:v1.0.0`** - not "next release", because five
of those went past, but before 1.0, because a memory system making poisoning-resistance
claims should measure this class before calling itself 1.0. Not closed: the scope, the
design in ADR 0003, and the LLM-budget blocker on the full procedure are all unchanged.

### Fixed (2026-08-21) - three versions shipped with no GitHub Release, and nothing was going to fix that

`v0.5.23`, `v0.5.24` and `v0.5.25` had tags but no Release object. The newest Release
was `v0.5.22` for eleven days while three versions went out, so the releases feed said
nothing was happening.

- **The cause was not a broken automation. There was never any.** No workflow in this
  repo has ever created a Release; every object up to `v0.5.22` was cut by hand until
  the hand stopped. Backfilling the three would have been fixing the symptom for the
  third time.
- **`release-crate.yml` now creates the Release itself**, after `publish` succeeds so
  a Release never advertises crates that are not on crates.io yet. Re-running a tag is
  a no-op when the Release exists, matching how the publish walk 404-checks each crate.
- **New `scripts/extract_release_notes.py`**, the part worth testing on its own: it
  pulls the `## [X.Y.Z]` section out of `CHANGELOG.md` and **fails loudly** when the
  section is missing or empty rather than shipping a Release with no notes, which looks
  answered and is not. Nine self-test cases, wired into CI, including that it never
  leaks `[Unreleased]` into a release body.
- The three missing objects were backfilled from their CHANGELOG sections.
- **Twelve tags still have no Release object**, the nine older ones being `v0.3.3`,
  `v0.3.4`, `v0.4.0-rc1`, `v0.4.2`, `v0.4.3`, `v0.5.0`, `v0.5.1`, `v0.5.2` and `v0.5.3`.
  Left alone deliberately: backfilling historical announcements is noise, and the
  workflow now covers everything from here.

### Changed (2026-08-21) - the TypeScript SDK says what it is

`@mndfreek/mnemo-sdk` is at `0.4.4` on npm, published 2026-05-18, while the Rust line
moved to 0.5.x. The repo already described the gap; what it did not do was state a
maintenance position or a version the SDK is known to work against.

- **Declared maintenance-only, not on the 0.5 train.** Stated in
  `sdks/typescript/README.md` (which ships to npm) and in the repo README.
- **Verified rather than asserted:** the **published 0.4.4** package was installed from
  npm and run against a `mnemo-mcp-server` built from `0.5.26`. `remember`, `recall`
  and `verify` all succeeded and the hash chain verified. That is the compatibility
  claim and the whole of it.
- **What the tests do not cover, said plainly:** the SDK's 21 tests are type-shape and
  error-path only and not one spawns a server, so a green suite says nothing about wire
  compatibility. The 0.5.26 check was by hand and is a point in time, not a CI gate.
- **The caveat a user hits first:** `recall()` defaults to `auto`, which hard-errors on
  a server with no embedder configured. Documented with the exact message and the
  `strategy: "lexical"` workaround.
- **Publishing a current SDK was not an option today.** `npm publish` fails with
  `npm whoami -> 401 Unauthorized`: the `NPM_TOKEN` secret is expired or revoked. That
  is an operator action, so option (b) was the only one available, not merely the
  faster one.
- Corrected a stale line claiming `cargo install mnemo-mcp-server` resolves `0.5.23`.

### Added (2026-08-21) - the MCP conformance table closes what it can and states the rest

Two of the open rows are now implemented, the authorization question the table never
asked is answered, and every remaining open row tells a caller what to assume.

- **`ttlMs` and `cacheScope` implemented** on `tools/list` and `resources/list`
  (SEP-2549). `tools/list` advertises 60000; `resources/list` advertises 0, immediately
  stale, because any `mnemo.remember` invalidates the listing.
- **`cacheScope` is `private` on both, and that is correctness rather than tuning.**
  Both listings are scoped to the caller's per-request identity (ADR 0002), so `public`
  would let a shared intermediary serve one caller's tool catalog, or one agent's
  memory records, to a different caller. `CacheScope::default()` is `Public`, so leaving
  the field unset was never the safe option it looked like. Asserted on both surfaces
  and verified by mutation.
- **New authorization section: mnemo implements no OAuth.** No RFC 9728 Protected
  Resource Metadata is served on any transport. This is a conformant position rather
  than a gap: the spec makes authorization OPTIONAL and tells stdio servers they
  **SHOULD NOT** follow it, retrieving credentials from the environment instead, which
  is exactly what mnemo does with `MNEMO_CAPABILITY_KEY`. The RFC 8707 `resource`
  parameter MUST binds **clients**, and mnemo ships a server, so it does not apply;
  the server-side audience-validation MUST has no OAuth tokens to validate. Each row
  says what a caller should assume, including that an OAuth bearer token will not be
  accepted.
- **New CI guard: an open row must say what a caller should assume.** A row that says
  GAP and stops has told a reader something is missing without telling them how to
  behave. A second guard fails if the table ever reports no open rows at all, since the
  usual way that happens is a row being deleted rather than closed.
- Corrected the page's own miscount of how many rows were open.

### Added (2026-08-21) - a permanent handshake smoke test against the shipped binary

The 0.5.26 defect was that a *derived advertisement* disagreed with *negotiated
behaviour*. A library-level assertion on either one alone cannot catch a regression in
the wiring between them, so this tests the binary a user actually installs.

`crates/mnemo-cli/tests/handshake_version_smoke.rs` spawns `mnemo`, performs a real
stdio JSON-RPC `initialize`, and asserts the advertised version is one the server will
negotiate, that a client asking for `2026-07-28` is negotiated **down** rather than
echoed, and that a supported older revision is still honoured so the narrowing did not
become a blanket downgrade. Verified by mutation: restoring the original defect fails
the middle test with the exact reason.

Confirmed against the built binary: `initialize` answers `2025-11-25`, `serverInfo`
reports `0.5.26`, 23 tools are listed, and the listing carries `ttlMs: 60000` and
`cacheScope: "private"` on the wire.

### Fixed (2026-08-21) - clippy 1.98 reds `main`, not just this branch

CI moved to clippy 1.98 (`dtolnay/rust-toolchain@stable` tracks latest stable) and the
new `chunks_exact_to_as_chunks` lint fires on pre-existing code, so `-D warnings` fails.
`origin/main` at `f70cf08` has the same line: this was inherited, not introduced, and
the fix repairs main as well.

- `deserialize_embedding` in both `mnemo-core` and `mnemo-postgres` moves from
  `chunks_exact(4)` to `as_chunks::<4>()`. Both sites were fixed; CI had only reached
  the first, since clippy stops at the first crate that fails.
- The new form is better code independent of the lint: it yields `&[u8; 4]` so
  `from_le_bytes` takes the array directly, dropping four indexed reads whose bounds
  checks the type system can already prove unnecessary.
- Verified against the version CI actually runs, not the one that happened to be
  installed. The local toolchain was 1.97 and could not reproduce the lint at all, which
  is why the first push went red; `rustup update stable` to 1.98 first, then re-verified.
- **Added the round-trip test that did not exist.** `serialize_embedding` and
  `deserialize_embedding` are the only thing between a stored blob and the vector the
  index searches, and nothing asserted they were inverses. A silent corruption there
  surfaces as degraded recall rather than as an error. Two tests now cover exact
  round-trip, `None` handling, an empty blob, and a ragged trailing chunk being dropped
  rather than panicking. Verified by mutation: flipping the read to big-endian fails it.

### Note on versioning (2026-08-21)

No version bump. `0.5.26` is still **unreleased**: there is no `v0.5.26` tag and
`mnemo-core@0.5.26` returns 404 on crates.io. This work lands *in* 0.5.26. Bumping to
0.5.27 would burn a version number and leave a `## [0.5.26]` section describing a
release that never existed, which is the kind of changelog entry this repo has spent
three releases removing.

### Added (2026-08-19) - a conformance table for the 2026-07-28 MCP spec

The README pointed at the March 2026 MCP *roadmap* as mnemo's current spec anchor. A
July spec release superseded it, and nothing said which parts of it mnemo implements.
"We follow rmcp rather than racing the spec" is a defensible posture and remains the
posture; it is only defensible with a document behind it, otherwise it reads the same
as not having looked.

- **New: `docs/src/integrations/mcp-2026-07-28.md`.** One row per spec change, with
  what the spec requires, what mnemo does at this commit, what rmcp 3.1.3 provides,
  and a status of CONFORMS / GAP / UPSTREAM-BLOCKED. It covers the changes the brief
  named and the ones it did not, including the largest: the removal of the
  `initialize` handshake.
- **The headline: mnemo negotiates `2025-11-25`, not `2026-07-28`.** rmcp 3.1.3
  carries the newer revision's types, but `ProtocolVersion::LATEST` is still
  `2025-11-25`, so that is what a handshake settles on. Most rows follow from that.
- **Four rows are open.** `ttlMs` and `cacheScope` on list results are mnemo's to
  close today. The stateless lifecycle and the SEP-2243 headers wait on rmcp moving
  `LATEST`; no fork of rmcp. The resource-not-found error code stays `-32002` on
  purpose, because that is what the revision mnemo actually speaks specifies, and
  changing it early would trade real conformance for imaginary conformance.
- **A trap recorded for whoever closes the caching rows:** mnemo's `tools/list` is
  role-filtered per caller, so `cacheScope` must be `private`. `CacheScope::default()`
  is `Public`, which would let a shared intermediary serve one caller's catalog to
  another.
- README's MCP section now names the July release and links the table; the March
  roadmap is kept as history in a collapsed block rather than presented as current
  direction. The same stale framing in `docs/src/integrations/mcp-server.md` is marked
  superseded.

### Fixed (2026-08-19) - mnemo advertised a protocol revision it does not implement

Writing the conformance table surfaced a live defect rather than only recording known
ones.

- `MnemoServer` did not override `supported_protocol_versions()`, so it took rmcp's
  default of `ProtocolVersion::KNOWN_VERSIONS` - every revision *the SDK* knows,
  `2026-07-28` included. rmcp derives `server/discover` from that list. Confirmed
  against a running server: advertised `protocol_version = 2025-11-25`, but
  `supported_protocol_versions = [2024-11-05, 2025-03-26, 2025-06-18, 2025-11-25,
  2026-07-28]`.
- A client entitled to believe that advertisement would have sent `2026-07-28`
  requests to a server that still expects the handshake that revision removes, and
  received list results with neither `ttlMs` nor `cacheScope`. A machine-readable
  claim that was not true, which is the same claimed-but-not-wired shape repaired
  before in `role_filter` #124, tool-catalog attestation v0.5.20 and `LeaseStore`
  under ADR 0001.
- **Fix:** narrow the list to the four revisions mnemo serves. This is rmcp's own
  supported mechanism, not a workaround - its `negotiate_protocol_version` documents
  that a server narrowing the list "is never made to answer `initialize` with a
  version it cannot serve", and a client asking for an unlisted revision negotiates
  down instead of failing. No client breaks. `2026-07-28` goes back when mnemo
  implements it, not when rmcp does.
- Pinned by `crates/mnemo-mcp/tests/mcp_2026_07_28_conformance.rs`, which also asserts
  the doc's version list *equals* the server's, so the page and the code cannot drift
  apart. Both directions were verified by mutation.

### Added (2026-08-19) - the explicit-handle pattern is now a tested property

The 2026-07-28 revision removes protocol sessions and names the replacement: servers
needing cross-call state use explicit, server-minted handles passed as ordinary tool
arguments ([SEP-2567]). mnemo already worked this way. Being on the right side of a
change by accident is not the same as being on it on purpose.

- **New: `crates/mnemo-mcp/tests/explicit_handle_roundtrip.rs`.** Drives
  `checkpoint` -> `branch` -> `replay` end to end over real JSON-RPC `tools/call` on
  an in-memory duplex transport, which carries no session identifier of any kind. It
  asserts `branch` forks from the handle it was given and `replay` returns that
  handle's own snapshot - with a *newer* checkpoint deliberately present, so
  "resolved the handle" and "returned the only checkpoint there was" are
  distinguishable observations. A handle the server never minted is rejected.
- **Audit of all 23 tools for connection-scoped identity.** 19 take `Parameters<T>`
  and nothing else, so connection state is unrepresentable in their signatures. 4
  (`recall`, `forget_subject`, `delegate`, `trajectory_audit`) take a `CallerContext`
  resolved *per request* from that request's `_meta` (ADR 0002), which is not
  connection state and survives the revision unchanged, since `_meta` remains a
  per-request carrier under `2026-07-28`.
- **One deliberate deviation, documented rather than removed:** with no capability
  presented, `resolve_caller` falls back to a boot-derived identity. On stdio one
  process is one peer and the operator is the caller, so requiring a capability there
  would break every existing deployment to buy nothing. The deviation is bounded - a
  capability that is present but unverifiable is an error and never a downgrade, and
  the fallback does not apply at all on the network-facing `http-transport`.
- Documented in the `mnemo-mcp` README and in the conformance page.

[SEP-2567]: https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2567

### Changed (2026-08-19) - one publish lane, and a guard so a crate cannot be orphaned

`[Unreleased]` recorded for a second release that seven crates resolve a patch behind
because they publish on a lane the tag walk does not include. Deciding it rather than
carrying the note into a third.

- **The seven are folded into `WALK`:** `mnemo-letta`, `mnemo-mesh`, `mnemo-codemode`,
  `mnemo-deal`, `mnemo-md-sync`, `mnemo-cma`, `mnemo-baseline`. Each depends on
  `mnemo-core` alone and nothing in the walk depends on them, so they are leaves and
  folding them in reorders nothing. One lane now publishes everything in dependency
  order.
- **Why fold rather than add a second guard.** Two lanes with different contents is
  the condition that lets a crate fall behind with nothing red - the confusion the
  drift guard exists to prevent, which it had instead been taught to describe.
  Removing the split removes the class.
- **New: `scripts/check_publish_closure.sh`, wired into CI and into the release gate.**
  It asserts the general form of a bug that has now cost two releases: every
  publishable workspace member must appear in the closure or carry a written
  exemption; every name in the closure must be a real member (a typo fails the publish
  mid-release, after earlier crates have uploaded and cannot be recalled); and the
  library lane can never ship a crate the tag lane does not. `--self-test` runs five
  fixtures, one of which reproduces the exact `mnemo-admin` regression that killed
  v0.5.24 and v0.5.25.
- **Two exemptions, each with a recorded reason,** because an exemption without one is
  how a real orphan gets parked and forgotten: `mnemo-python` ships to PyPI as
  `mnemo-db` via maturin and is not a crates.io crate, and `mnemo-golem-host` is
  excluded from the CI workspace build (rust-lld rejects its generated
  `cabi_post_mnemo:golem-vector/...` symbol) so CI cannot build it, let alone publish
  it. The second was found by writing this guard: it is publishable in `cargo
  metadata`, in neither lane, and unpublished on crates.io.

### Changed (2026-08-19) - a README guard was holding the docs at a stale anchor

`readme_mcp_roadmap_link.rs` (v0.4.4 U1) pinned the README's link to the March 2026
MCP roadmap so "a future README rewrite that drops the anchor will fail this test
before it can land". The intent was right: an alignment claim with no primary source
is unanchored marketing text. It pinned the wrong thing, though, asserting a specific
*heading string*, so when the July spec superseded the roadmap the guard's only effect
was to hold the README at the older anchor. A test that stops a doc being updated to
the truth has inverted its own purpose.

Renamed to `readme_mcp_spec_anchors.rs` and strengthened rather than dropped. It now
asserts the current revision is cited by primary source, the conformance table is
linked, the roadmap link survives (deleting history is the other way to make a claim
unauditable), and the roadmap is *subordinate* to the current spec, checked by
ordering. Verified by mutation: moving the roadmap URL ahead of the spec URL fails.

Separately, `docs_rmcp_version_matches_workspace.rs` compared documented rmcp versions
to the pin by exact `major.minor`, which made naming the *resolved* version a failure:
`rmcp = "3.0"` is a caret requirement resolving to 3.1.3, and the conformance page
turns on behaviour specific to that patch. It now accepts a version that *satisfies*
the requirement (same major, minor at or above the pin) rather than only the literal.
Deliberately not an `ALLOWLIST` entry, which would have exempted the file forever
including across a major bump. Nine unit cases pin both directions: `3.1` satisfies
`3.0`, while `2.2`, `1.3`, `0.14`, a `3.1` doc under a `4.0` pin, and a `3.1` doc
under a `3.2` pin all still fail.

### Changed (2026-08-19) - issue #37 labels now match its state

`needs-design` had been on #37 since 2026-05-03, when the ask was a design doc before
any code. [ADR 0003](docs/adr/0003-minja-procedure-harness.md) is that doc, and the
2026-08-18 decision scoped the issue to one session's work. What remains outstanding
is an LLM budget for the explicitly out-of-scope phases, which is not a design
problem. Label removed and the state restated on the thread.

### Fixed (2026-08-19) - the publish closure was written down three times

`v0.5.25` failed twice with the same error the `v0.5.24` fix was meant to cure:
`failed to select a version for the requirement mnemo-admin`. The gate passed, the
packaging dry-run died, publish skipped, nothing uploaded.

- **Cause: one closure, three hand-written copies.** `WALK`, the gate's clippy and
  test lists, and the coordinated dry-run each carried their own list of crates. The
  first fix updated two of the three and missed the dry-run, so it kept packaging the
  old twelve crates without `mnemo-admin` or `mnemo-pgwire`. A list duplicated three
  ways drifts, and it cost two releases.
- **Fix:** `WALK` is workflow-level env and the only definition. The gate, the
  dry-run and the publish loop all expand it. Verified mechanically rather than by
  eye: zero hand-written `-p mnemo-*` lists remain, the walk carries all 14 crates in
  topological order, every `mnemo-mcp-server` dependency is present, and none is
  ordered after it.
- **Result: `mnemo-mcp-server` is current again at `0.5.25`.** It had stranded at
  `0.5.23`, skipping `0.5.24` outright; its crates.io history still reads
  `0.5.25, 0.5.23, 0.4.4`. `mnemo-embeddings-bench` recovered with it. Confirmed
  against the registry, not the workflow log: `cargo add mnemo-mcp-server` resolves
  `0.5.25`, and the published `mnemo-core` verifier still detects both tampering
  modes offline in a scratch project.

### Fixed (2026-08-19) - the drift guard claimed parity it did not have

`check_version_drift.sh` printed "every published crate matches workspace 0.5.25"
while seven crates resolved a patch lower, and labelled each of those rows
`ok (matches workspace)`.

- The gate logic is correct and unchanged: `behind()` tolerates one patch as a
  release in flight, so those crates are legitimately not a failure.
- The **output** was false. A reader would conclude `cargo add mnemo-letta` gives the
  workspace version; it gives one patch lower. A guard that reports green over a real
  lag is the failure mode this repo keeps paying for.
- Rows now read `ok (one patch behind X, publish in flight)` when that is the case,
  and the summary names the crates instead of asserting universal parity. Still
  green, still the same gate, no longer a false statement.
- The seven are `mnemo-letta`, `mnemo-mesh`, `mnemo-codemode`, `mnemo-deal`,
  `mnemo-md-sync`, `mnemo-cma` and `mnemo-baseline`. They are not in the tag walk and
  publish on the push-to-main lane, which stopped partway through this release. Stated
  in the README Naming section with the real numbers.

## [0.5.25] - 2026-08-18

### Landing trace (2026-08-17)

The 0.5.25 window opens on the **v0.5.24** cut. The release content landed on `main` at
[`cb9243d`](https://github.com/sattyamjjain/mnemo/commit/cb9243d), and the `v0.5.24` tag
points at the cut commit directly above it, which is where the `## [0.5.24]` heading the
publish gate requires first exists.

### Decided (2026-08-18) - issue #37 scoped down rather than left open

#37 had been open 116 days and was the repo's only open issue, so its status was
standing in for the roadmap. Decided in writing today, and kept rather than closed.

- **Kept, because nothing replaced it.** ADR 0003 shows all four existing poisoning
  benches measure something else, and none models an interactive query-only attacker.
  Closing it as superseded by a retrieval-quality number would be a category error,
  since poisoning defense and recall quality are different measurements.
- **What was actually blocking it was budget, not engineering.** A faithful harness
  needs a model in the loop as the attacker, because MINJA Phase 2 is generative and
  adaptive. There has been no LLM budget for four months. An issue whose blocker is
  funding, filed as though its blocker were code, does not move.
- **Reduced to one session of work** and retitled to match: Phase 3 exploitation only,
  against a pre-registered fixed corpus of already-shortened records, one dataset, one
  real embedder, ASR with the detector off and on, Wilson 95% intervals, and a
  topic-matched benign control. Phases 1 and 2, the adaptive loop, and the ROC sweep
  stay explicitly out of scope until a budget exists.
- The reduced result will **not** be a MINJA number and the retitle says so. Removing
  the adaptive shortening step is exactly what ADR 0003 says makes a result stop being
  MINJA.

### Added (2026-08-18) - one real-embedder recall number, generated into the README

The number itself is not new. What is new is that it is now reproducible from the
artifact rather than typed into prose, and that the artifact says what it was measured
on.

- **The result file now identifies its own inputs.** `bench/results/locomo_v1.json`
  previously recorded the embedder as `"minilm"`, a string scraped from the parent
  directory name, which does not identify a checkpoint. It now records `model_id`,
  the **sha256 of the weights actually loaded**
  (`759c3cd2b7fe...`), the source URL, the storage backend the number was measured on,
  the UTC date, and the commit. Two different checkpoints can sit in directories with
  the same name, so the hash is the only field that pins the number.
- **Measured:** gold-document **recall@1 0.689, Wilson 95% [0.543, 0.805]**, recall@5
  0.889, recall@10 0.911, MRR 0.770. Embedder `Xenova/all-MiniLM-L6-v2` (384-dim, ONNX),
  storage DuckDB in-memory, n=45 queries, mean of 5 seeds, on arm64/darwin Apple M4.
  **Control:** the same corpus and harness with the vector lane off gives lexical
  recall@1 0.422 [0.290, 0.567]. The gap between those two rows is what the embedder
  buys.
- **The README now reads the number instead of asserting it.**
  `scripts/gen_recall_number.py` renders the block from the JSON, and `doc-guards` runs
  it with `--check`. The old text hardcoded 0.689 with nothing tying it to the bench
  output. This repo already learned that lesson twice: the crate-name check was
  verified once by hand, and the published-versions table sat stale for four days.
- **What the number is not.** Not a LoCoMo leaderboard score and not comparable to one,
  since the corpus is the bundled LongMemEval_M slice. Retrieval quality only, with no
  LLM in the loop and no answer-correctness judge. Silent about poisoning resistance
  and audit integrity, which are measured separately. n=45 is under 100, so the bench
  marks it preliminary and the interval is the claim, not the point estimate.
- Postgres semantic recall already hard-errors with
  `BackendUnsupported { backend: "postgres", capability: "semantic_recall" }` rather
  than returning an empty set, and `crates/mnemo-postgres/tests/semantic_recall_fails_loud.rs`
  pins it through the engine, not just the index. Verified, unchanged.

### Fixed (2026-08-18) - the published crates do not all agree, and now the README says so

The 0.5.24 publish split, and nothing in CI would have reported it.

- **Measured state:** 19 library crates reached `0.5.24`. `mnemo-mcp-server` and
  `mnemo-embeddings-bench` are still at `0.5.23` and never received `0.5.24`, so
  `cargo install mnemo-mcp-server` currently resolves `0.5.23`.
- **Cause:** the release ran down two lanes. The push-to-main lane carried the 19
  library crates. The tag lane, the only one that publishes those two crates, failed
  its packaging dry-run with
  `failed to select a version for the requirement mnemo-admin = "^0.5.24"`, because
  `mnemo-admin` was not in that lane's walk and could not resolve. Both are published
  now, so the walk can resolve on the next run.
- **Stated in the README Naming section**, beside the crate-name collision warning,
  with the real per-crate numbers and the cause, so a reader who installs the binary
  knows what they are getting.
- **Guarded.** The published-versions table had been stale since 2026-08-14 and nothing
  caught it: `gen_published_versions.py --check` existed but ran in no job. It runs in
  `doc-guards` now, along with the new recall-number check.

## [0.5.24] - 2026-08-17

### Landing trace (2026-08-14)

The 0.5.24 window opens on the **v0.5.23** cut. The release content landed with the
[#155](https://github.com/sattyamjjain/mnemo/pull/155) merge — `main` at
[`7599fbc`](https://github.com/sattyamjjain/mnemo/commit/7599fbc) — and the `v0.5.23` tag
points at the cut commit directly above it, which is where the `## [0.5.23]` heading the
publish gate requires first exists.

### Added (2026-08-17) - the crate-name collision, stated; the GPM table, generated

Three things that were true but unenforced, or true but written down twice.

- **Crate-name collision, in the README rather than in a maintainer's head.** New
  `### Naming` section under *Install from crates.io*: this project does not own the
  `mnemo` name on crates.io and publishes only under `mnemo-*`. **Two** foreign crates sit
  on names a reader would reach for — [`mnemo`](https://crates.io/crates/mnemo) @ 1.0.0
  (`aayushadhikari7/mnemo`, "a personal knowledge vault for your terminal") and
  [`mnemo-cli`](https://crates.io/crates/mnemo-cli) @ 0.1.0 (`watzon/mnemo`, "CLI
  management tool for the Mnemo LLM memory proxy"). `mnemo-cli` is the sharper trap and was
  previously undocumented: this repo's server binary lives in the directory
  `crates/mnemo-cli` but **publishes as `mnemo-mcp-server`**, so `cargo install mnemo-cli`
  resolves, installs someone else's program, and nothing goes red. Verified live
  2026-08-17 against the crates.io API; ownership determined from each crate's
  `repository` field, not from the name.
- **[`scripts/check_crate_name_refs.sh`](scripts/check_crate_name_refs.sh) + a `doc-guards`
  CI job.** The 2026-08-12 entry below records "Verified no doc tells a user to
  `cargo add mnemo`" — a **one-time manual check with no way to stay verified**. This
  converts it into a gate over every tracked markdown file. It flags the wrong crate only
  inside fenced code blocks, so prose may name the trap in order to warn about it (the
  README `Naming` section has to). `CHANGELOG.md` is excluded: it quotes the wrong command
  on purpose and rewriting history to please a linter is the wrong repair. `--self-test`
  runs 17 cases — including a fenced-vs-prose fixture — and fails if the matcher stops
  separating the crates we publish from the two we do not own, on the
  `registry_parity.sh --self-test` principle that a guard never shown to fire is
  indistinguishable from one that cannot. **Audited at the same time:** all 21 publishable
  workspace crates are on crates.io under `mnemo-*`; `mnemo-golem-host`,
  `mnemo-golem-wit` and `mnemo-python` are unpublished, and the eight `bench/*` crates
  other than `mnemo-embeddings-bench` are `publish = false`. **No doc needed correcting** —
  every existing reference was already right. The guard is what keeps that true.
- **The GPM clause table is now generated, not prose.**
  [`docs/research/governed-persistent-memory-clauses.toml`](docs/research/governed-persistent-memory-clauses.toml)
  is the source of truth for all five Governed Persistent Memory clauses
  ([arXiv:2608.12476](https://arxiv.org/abs/2608.12476)); the markdown table is rendered by
  [`scripts/gen_gpm_clause_table.py`](scripts/gen_gpm_clause_table.py) (same
  `--write`/`--print`/`--check` interface as `gen_published_versions.py`), and CI fails if
  the committed table has drifted. `mnemo-compliance` owns the correctness test
  ([`tests/gpm_clause_manifest.rs`](crates/mnemo-compliance/tests/gpm_clause_manifest.rs)):
  a `ships`/`partial` clause must point at a symbol that **exists**, an `absent` clause must
  point at **nothing** — the direction that protects honesty rather than marketing, since it
  fires when an implementation appears while the doc still claims a gap — and the
  `conflicts` clause must name a shipped feature that is **still shipped**, so the
  non-revival-vs-`as_of` conflict cannot be silently resolved while the page keeps asserting
  it. All four assertions were verified in the failing direction by mutation before landing.
  Every restatement of the per-clause status is replaced by a link to the one generated copy
  — in `README.md`, in [POSITIONING](docs/POSITIONING.md), and in the research page's **own**
  header and "What this page is NOT" section, which had drifted furthest: the header claimed
  mnemo "ships two of the paper's five clauses in recognisable form (ledger integrity, and a
  weaker source binding)", silently omitting conflict isolation, which is also partial.
  Counts went with them — a count drifts the moment a clause moves. The fix also corrected a
  `WriteProvenance` link that pointed at `provenance.rs` rather than
  `model/write_provenance.rs`.
- **Registry smoke test of the install commands.** A `Naming` section is only worth having if
  the commands it gives resolve to *our* crates, so they were run for real: in a scratch
  project outside the workspace, `cargo add mnemo-core mnemo-compliance` resolves from
  `registry+https://github.com/rust-lang/crates.io-index` with checksums — not a path
  dependency — and the published `mnemo-core` hash-chain verifier detects **both** tampering
  modes offline: an altered record (`content hash mismatch`) and the harder forgery where the
  attacker also recomputes the content hash, which only the chain linkage catches
  (`chain hash mismatch`). That is the wedge the README leads with, exercised against the
  registry rather than the local path dep.

### Verified (2026-08-17) - Postgres semantic recall does not return empty

The long-standing "semantic recall on PostgreSQL returns an empty result instead of
failing" concern is **already fixed** — no behaviour changed here. Verified at
[`6f0ffb7`](https://github.com/sattyamjjain/mnemo/commit/6f0ffb7). Recorded so this stops
being re-investigated.

- Fail-loud landed in [`686bcd1`](https://github.com/sattyamjjain/mnemo/commit/686bcd1),
  became the structured `Error::BackendUnsupported { backend, capability, detail }` in
  [`cce82ed`](https://github.com/sattyamjjain/mnemo/commit/cce82ed), and the path became a
  real pgvector ANN query in
  [`665e319`](https://github.com/sattyamjjain/mnemo/commit/665e319) / async in
  [`74c43bc`](https://github.com/sattyamjjain/mnemo/commit/74c43bc) (#99). All four
  `filtered_search` call sites in `query/recall.rs` (`semantic`, `auto`, `graph`,
  `domain_scoped`) propagate with `.await?`.
- **The claim was nonetheless untested in ordinary CI.** The only end-to-end coverage was
  `tests/pgvector_ann.rs`, which **skips** when `MNEMO_TEST_POSTGRES_URL` is unset — the
  same "a skip is indistinguishable from a pass" hole that let `mnemo-mcp-server` strand for
  87 days. New
  [`crates/mnemo-postgres/tests/semantic_recall_fails_loud.rs`](crates/mnemo-postgres/tests/semantic_recall_fails_loud.rs)
  needs no database, runs on every `cargo test --workspace`, and asserts through the
  **engine** rather than the index that all four strategies return `BackendUnsupported`
  instead of `Ok(empty)`. The fixture store provably holds a record — checked by a control
  test reading storage directly, not via any recall strategy — so "no matches" and "not
  implemented" cannot be confused. Confirmed to fail in the right direction: injecting a
  single `.unwrap_or_default()` on the `semantic` call site turns it red with `Ok(0)`.

### Added (2026-08-16) - prior-art citation and a release check with teeth

Cited the governed-persistent-memory paper in prior art, including the part mnemo does not do.
Added a release check so a crate cannot sit five minor versions behind the workspace without the
release failing.

- **[arXiv:2608.12476](https://arxiv.org/abs/2608.12476) — Governed Persistent Memory.** New
  anchor at [`docs/research/governed-persistent-memory-2608.12476.md`](docs/research/governed-persistent-memory-2608.12476.md),
  cited from the README and from [POSITIONING](docs/POSITIONING.md) under *Where mnemo does not
  lead*. The paper's argument is that retrieval never decides whether a contradictory,
  superseded, retracted, deleted or stale record may support an outgoing claim, and it proposes a
  bitemporal state-transition model with source-bound admission, derived lifecycle state and
  fail-closed structured release. Of its five clauses mnemo ships ledger integrity outright,
  source binding and conflict isolation in weaker partial forms, and neither non-revival after
  retraction nor claim closure over a verified head. **mnemo does not implement the bitemporal
  derived-lifecycle-state model, and has no release gate at all** — `RECALL` returns records and
  mnemo's involvement ends. Non-revival is in direct conflict with a shipped feature, since
  `as_of` recall deliberately surfaces deleted records. The clause-by-clause table says which is
  which. Numbers are quoted with the paper's own caveat that they are bounded contract results,
  not open-world accuracy.
- **Severity floor on the release path.** `scripts/registry_parity.sh --fail-on-minor-lag`, passed
  by `release-crate.yml` and by nothing else: a publishable crate that is a whole minor behind the
  workspace, or absent from crates.io, and that the current walk does not repair, now fails the
  release instead of emitting a warning. `--mode assert` already caught a walk skipping a crate it
  owned; it could say nothing about a crate in **no** walk, which is the hole #140 came through.
  The threshold is any minor-level lag, not "more than one minor" — #140 was 0.4.4 against a
  0.5.22 workspace, exactly one minor, so the obvious spelling would have missed the incident it
  was written for. Patch-level lag still passes, because a repo one patch ahead of the registry is
  a release in flight. `--self-test` asserts that table offline in CI. The flag is deliberately not
  set on `cargo-publish.yml`, which cannot publish `mnemo-mcp-server` by design and must not be
  failed over it.
- **[`docs/release/registry-token-runbook.md`](docs/release/registry-token-runbook.md)** — the
  five-minute triage order, with the corrected diagnosis stated first: the token was never #140's
  blocker, and the `/api/v1/me` 403 that anchored that belief is advisory. Check walk membership
  and the tag/CHANGELOG gates before touching credentials. Linked from the failure summary the
  parity script writes.

### Changed (2026-08-16) - agent-audit-kit 0.3.68 to 0.3.79

Jumped past the stale Dependabot PR (0.3.68 to 0.3.74, already five releases behind when it
opened) straight to current. **Verified rather than assumed:** both versions were run against this
repo with the CI parameters. Rules evaluated went 275 to 303 and the finding set was identical by
rule id and location — 28 new rules, zero new findings, 0 critical, exit 0 on both. Nothing needed
fixing, so nothing was pinned back. The `.agent-audit-kit.yml` baseline was re-derived and now
records both the local-tree and clean-checkout counts, because a local working tree carries
gitignored files CI does not and recording one number makes the other look like a regression.

### Fixed (2026-08-16) - capability leases now narrow to the subjects the read covered (#160)

The last of [ADR 0001](docs/adr/0001-capability-leased-reads.md)'s four properties. Freshness,
causality and caller-binding shipped in #159; scope did not, so a lease earned by reading `alice`
still authorised that same caller to erase `bob` inside the TTL.

- **The lease now records what the read returned.** `mnemo.recall` collects the `subject:` tags
  carried by the records in its own response, and `mnemo.forget_subject` is refused unless the
  requested `subject_id` is in that set.
- **#160 called this blocked, and it was one step short.** Its reasoning — that inferring subjects
  from a ranked result set either over-narrows or over-broadens — is correct about the *query* and
  does not apply to the *response*, which is what the caller was actually handed. The proposed fix
  was a breaking change to `mnemo.recall` letting the caller declare a subject set up front; that
  is both weaker (a scope the caller nominates is a scope the caller chose) and unnecessary.
- **Empty means nothing, not everything.** A recall that surfaced no subject-tagged record mints a
  lease authorising no erasure. `a_read_covering_no_subject_authorises_no_erasure` pins it, because
  the inverted reading would turn every subject-free recall into a blanket grant — worse than the
  gap being closed.
- **Ordered after the coarser refusals**, so an expired lease still reports expired and a stale
  lease does not leak which subjects it covered. `LeaseError::SubjectNotCovered` names both the
  subject asked for and the subjects covered, so a refusal is diagnosable.
- Six tests across `crates/mnemo-mcp/src/lease.rs` and
  `crates/mnemo-mcp/tests/capability_leased_reads.rs`. Also corrects the README capability matrix,
  which still listed lease tokens as "not shipped — removed as dead code" a day after #159 shipped
  them, and a README line still claiming crates.io was stuck at 0.4.4.

### Added (2026-08-15) - per-request caller identity on the MCP surface (ADR 0002)

Identity on the MCP server was **boot-derived**: `engine.default_agent_id`, fixed at startup,
was the caller for every request. [ADR 0002](docs/adr/0002-request-identity-model.md) decided
per-request identity; this implements it.

- **A capability now rides each request.** `crates/mnemo-mcp/src/identity.rs` reads an
  HMAC-signed `Capability` from `_meta["dev.mnemo/capability"]`, verifies signature, expiry and
  key id, and resolves a `CallerContext` whose id is the capability's principal and whose roles
  are its `role:`-prefixed scope tokens.
- **It needed no new transport.** The ADR — and #126, and this repo's own migration audit — all
  assumed this waited on an authenticated HTTP transport. It did not: MCP requests carry `_meta`,
  and rmcp surfaces it on **every** transport including stdio. That assumption had been blocking
  the work; it was simply wrong.
- **Resolved once per request, before the gate.** `call_tool` resolves the caller ahead of the
  role check (so gating uses the capability's roles, not a boot constant) and injects the result
  into the request extensions; `mnemo.delegate` and `mnemo.trajectory_audit` take it as an
  `Extension<CallerContext>`. `list_tools` filters the catalog per caller, and `list_resources`
  scopes reads to the caller — closing the read leak ADR 0002 names, where a boot-derived id
  would serve one agent's memories to every authenticated caller behind a correct-looking gate.
- **Fail-closed, and tested that way.** A capability that cannot be verified — no issuer
  configured, malformed, forged, expired, unknown key — is rejected. It is never downgraded to
  the boot identity, which would hand a forgery *more* authority than it claimed.
  `no_rejection_path_ever_yields_the_boot_identity` asserts that over every failure mode.
- **Backward compatible.** A request with no capability resolves to the boot identity exactly as
  before, so existing stdio deployments are unchanged and no issuer is required.
- Unblocks [#126](https://github.com/sattyamjjain/mnemo/issues/126): its revisit gate is
  "distinct callers hold distinct identities", and
  `crates/mnemo-mcp/tests/per_request_identity.rs` shows two capabilities resolving to two
  distinct callers on one server.

### Added (2026-08-15) - capability-leased reads, closing [#126](https://github.com/sattyamjjain/mnemo/issues/126)

`--lease-ttl-seconds <N>` binds a destructive erasure to a read the **same caller** just
performed, breaking the OX-MCP "exfiltrate-then-act" chain. `mnemo.recall` mints a short-lived
lease; `mnemo.forget_subject` requires it.

- **The blocker turned out not to be the transport.** #126 and [ADR 0001](docs/adr/0001-capability-leased-reads.md)
  deferred this pending "a multi-caller authenticated transport", because on single-caller stdio a
  lease keyed on `agent_id` is ceremony. What actually unblocked it was **per-request identity**
  (ADR 0002): the lease binds to the capability-verified principal of the request that minted it,
  so a replay by another principal fails — on stdio too, where a gateway may multiplex several
  agents over one pipe.
- **Off by default.** `mnemo.recall` and `mnemo.forget_subject` are shipped, docs-drift-tested
  tools; enforcing unconditionally would break every existing client on upgrade. Unattached, both
  behave exactly as before. Requires `--capability-key` — without per-request identity every
  lease would validate for every caller, which is a defence in appearance only.
- **No `ExportAuditLog` scope**, though the original design named it: `export_audit_log` is not
  an MCP tool but a library function, and a scope gating a tool that does not exist is the
  claimed-but-not-wired defect this repo has now repaired four times.
- **The lease is checked against the caller, not the requested `agent_id`** — validating against
  a caller-supplied field would let the caller answer its own question.
- Verified live over the HTTP transport, not just in unit tests: recall mints a lease bound to
  `alice`; `forget_subject` without one is refused; **`mallory` replaying `alice`'s lease is
  refused**; an invented token is refused; `alice` spending her own succeeds.

> **Scope narrowing is not implemented.** Freshness (TTL), causality (a read must have happened),
> and caller-binding are enforced. The design's third property — restricting the erasure to the
> subjects the read covered — is not: a lease earned by a narrow read still authorises that caller
> to erase a different subject within the TTL. ADR 0001 records why (deriving a subject set from a
> ranked query result either over-narrows or over-broadens) and what closing it requires.

### Added (2026-08-15) - authenticated Streamable HTTP transport for MCP (feature-gated)

`mnemo --http-port <PORT>` serves MCP over rmcp's Streamable HTTP transport, behind the
`http-transport` build feature. **stdio remains the default** and is unchanged.

- **Callers authenticate per request** with `Authorization: Bearer <base64url capability>`.
  rmcp injects the request's `http::request::Parts` into the rmcp extensions, so the header
  is read by the *same* resolver `_meta` capabilities go through — one verification path with
  two carriers, rather than a second security path to keep in sync.
- **`mnemo capability issue`** mints tokens (`--format bearer` for the header, `--format json`
  for `_meta`). Verification without a way to issue was an unusable feature.
- **`--http-port` refuses to start without `--capability-key`.** A network-facing port whose
  server cannot tell its callers apart is a multi-caller transport with a single-caller
  identity model.
- **An unauthenticated HTTP request is rejected**, not treated as the operator. This was a
  real hole in the first cut of this work, found by end-to-end probing rather than by the unit
  tests: with no `Authorization` header, `mnemo.delegate` happily recorded
  `delegator: "default"` — the boot identity — so anyone who could reach the port held operator
  authority. The boot fallback is sound on stdio, where one process genuinely is one caller,
  and unsound anywhere else, so it is now conditioned on the transport.
- **Two credentials that disagree are rejected** rather than resolved by precedence. Whichever
  one a gate checked, the other is what a reader of the audit trail might reasonably believe
  authorised the call.
- Binds `127.0.0.1` and does not terminate TLS — put it behind a reverse proxy.
- 14 identity tests plus a live end-to-end check: two capabilities on one running server
  produce `delegator: alice` and `delegator: bob` on the same tool.

### Fixed (2026-08-15) - the Python suite runs in CI for the first time, and it is green

Nothing ran `pytest`. The 138-test Python suite was **unexecuted in CI**, and had been sitting on
two failures nobody could see.

- **New `python-tests` CI job.** It **builds the native extension** with maturin rather than
  skipping it: the integration tests self-skip when `mnemo._mnemo` is absent, so a pure-Python job
  would go green while silently not running the tests that cover the session-store fix below — the
  same "a skip is indistinguishable from a pass" failure mode that let `mnemo-mcp-server` strand
  for 87 days.
- **Tantivy `LockBusy` fixed** (`python/mnemo/openai_sessions.py`). Two `MnemoSessionStore`
  instances on one `db_path` each built their own `MnemoClient`, so each opened its own Tantivy
  IndexWriter — and Tantivy takes an **exclusive** writer lock per index directory by design. The
  second store died with `Failed to acquire Lockfile: LockBusy`, breaking the most ordinary usage
  there is: two concurrent sessions over one database. A session is a *logical* scope over one
  *physical* store, so clients are now shared per `(db_path, agent_id, embedding config)` within
  the process. Isolation is unchanged — it was always enforced by the `_session_tag` filter, never
  by having separate engines.
- **The Python SDK is version-LOCKED to the workspace**, and the "versions independently" comment
  that said otherwise is gone. It was self-contradictory (it claimed "currently 0.5.21" beside a
  `0.5.12` value) and it produced exactly the bug you would predict: **the wheel metadata said
  0.5.12 while the `mnemo-core` compiled into that same wheel was 0.5.23**, because
  `python/Cargo.toml` already carries `version.workspace = true`. The distinction that matters:
  the TypeScript and Go SDKs are thin MCP-over-STDIO clients that embed nothing, so they genuinely
  version independently; `python/` is PyO3 bindings that ship an engine, so the wheel must name it.
  Now fenced offline by `workspace_version_fence.rs`.
- **Replaced a rot-prone test.** `test_v0_5_12_pinned` hard-coded a release literal, so it went red
  the moment someone forgot to bump it — which is what happened. It now asserts the *structural*
  invariant instead (`python/Cargo.toml` must inherit, never pin), which cannot rot with a release.
  Also fixed the `[project].version` parser it used: a `\[project\][^\[]*?version` regex stops at
  the first `[` **anywhere** after the header, including inside a comment, so a comment mentioning
  `[workspace.package]` silently broke it. A version fence that fails closed on prose is not a fence.

### Changed (2026-08-15) - one identity source in the MCP server (ADR 0002 §2)

Three surfaces disagreed about where caller identity came from: `list_resources` scoped reads by
`engine.default_agent_id`, `delegate` stamped `delegator_id` from that same boot constant, and only
tool gating went through `caller_context()`. On stdio all three resolve to the same string, so the
divergence was **invisible** — which is the problem.

Under the multi-caller transport [ADR 0002](docs/adr/0002-request-identity-model.md) designs for,
that code would have served one agent's memories to **every** authenticated caller, and attributed
**every** caller's delegations to the boot identity — a read leak and a forged authority claim,
both sitting behind a tool filter that was gating correctly. All three now route through
`MnemoServer::caller_agent_id()`, so the HTTP follow-up changes `caller_context()` once and every
identity-derived surface moves with it. `identity_sources_agree` pins that they cannot drift apart
again. **No behaviour change on stdio** — one process is still one caller.

### Removed (2026-08-15) - `mnemo-graph`'s extract stub, which was three false claims (#156)

`TemporalEdge::extract` always returned `Vec::new()`. Removed rather than left in place, because a
function that always returns empty is **worse than an absent one**: a caller cannot distinguish
"found no relations" from "not implemented", so wiring it in yields silent no-ops forever — the
latent-stub class `onnx.rs` already warns about in a comment.

Three claims around it were false at once: the docstring promised the real extractor "lands in
v0.4.0 final" (the workspace reached **0.5.23**, five releases later); `lib.rs` described it as
"feature-gated under `graph-extract`" while `pub mod extract;` was compiled **unconditionally**, so
the flag gated nothing; and `MNEMO_GRAPH_EXTRACT_MODEL` was documented in `CLAUDE.md` as a real
environment variable that **no code reads**. All four are gone, and `mnemo-graph` now says plainly
what it is: a bitemporal **storage + query** layer with no LLM in it. If extraction is wanted later
it should arrive as a designed feature with its own issue, not a placeholder.

### Added (2026-08-15) - GCS + Azure Blob workspace backends (closes #39)

Completes the four-backend set for the OpenAI Agents SDK GA snapshot store (S3 v0.3.2,
R2 v0.3.4). Both new backends are **standalone classes, not `S3Workspace` subclasses**, per #39's
"single class per backend": GCS's interoperability XML API is only a partial S3 emulation (it
lacks the paginated `list_objects_v2` / batch `delete_objects` calls `S3Workspace` relies on, so
subclassing would inherit methods that fail against a real bucket), and Azure Blob shares no S3
wire surface at all.

- The signing contract is untouched — `manifest.py` still does all Ed25519 and per-blob digest
  work, so **all four backends write a byte-identical object layout** and a snapshot is portable
  between providers by copying objects.
- **One Azure behaviour needed encoding**: `upload_blob` *rejects* an existing name unless
  `overwrite=True` (S3/GCS overwrite silently). Without it, re-saving to the same `key_prefix`
  fails on the **second** attempt — a retry-path bug that only surfaces in production. Pinned by a
  regression test, and the in-process fake enforces the same rule rather than accepting every write.
- 13 tests passing, 2 skipped (live-credential gates: `GCS_BUCKET`;
  `AZURE_STORAGE_CONNECTION_STRING` + `AZURE_CONTAINER`, the latter working against **Azurite**).
  GCS has no moto equivalent and both official emulators need Docker, so the unit substrate is
  in-process fakes implementing only the methods the backend calls — a future change reaching for
  an unfaked method fails loudly instead of passing against a permissive mock.
- New [`docs/src/integrations/workspace-backends.md`](docs/src/integrations/workspace-backends.md)
  parity matrix; fixes the dangling ``:doc:`/storage/workspace-backends``` reference in
  `r2_workspace.py` that pointed at a path which never existed.

### Added (2026-08-15) - DuckLake evaluated and declined; the #41 gate is met in the wrong direction

[#41](https://github.com/sattyamjjain/mnemo/issues/41) set its own bar: *"do NOT flip default until
Step 2 shows >= 20% p95 win."* Measured at the issue's own specified 1M rows, DuckLake meets that
gate on **0 of 5** query shapes — it is **2.2×–10.7× slower** than the embedded DuckDB backend
mnemo already ships, including on `COUNT(*)`, the operation DuckLake's own 1.0 announcement
advertises 8×–258× speedups for.

- **Feasibility was never the blocker.** Every DDL feature #41 names works today: catalog `ATTACH`,
  partitioned tables, data inlining, and `VARIANT` metadata (JSON round-trips intact).
- **The announced speedups do not transfer**, and that is not a defect in DuckLake: they are
  measured against *other lakehouse formats* (Iceberg/Delta) on *object storage*. mnemo is
  embedded, single-node, single-writer, local-file — the corner where catalog indirection and
  Parquet round-trips are pure added cost. DuckLake is faster than the thing it replaces; mnemo is
  not running that thing.
- **The cost side**: `StorageBackend` has **50 methods** and the two existing impls are 1,853 and
  1,657 lines. Step 2 is a ~1,500-line commitment to ship a backend that is slower on every read
  shape mnemo performs.
- Recommendation recorded in
  [`docs/benchmarks/2026-08-15-ducklake-evaluation.md`](docs/benchmarks/2026-08-15-ducklake-evaluation.md):
  close Step 2 as evaluated-and-declined; re-open if mnemo ever gains multi-writer concurrency,
  object-store-backed storage, cross-engine access, or a time-travel requirement — DuckLake's
  actual wins, none of which mnemo has today. Reproduce with
  `python3 scripts/bench/ducklake_eval.py`.

### Added (2026-08-15) - two ADRs that unblock the MCP migration, and the MINJA design #37 asks for

- **[ADR 0002 — request identity model](docs/adr/0002-request-identity-model.md)** answers the one
  question blocking *both* [#126](https://github.com/sattyamjjain/mnemo/issues/126) and the MCP
  2026-07-28 migration's §1. **Decision: per-request identity**, with per-connection resolution as
  a cache that is never the source of truth. Per-*connection* identity was rejected because it is
  structurally the same "boundary established once and then assumed" defect this repo has repaired
  three times (`role_filter` #124, tool-catalog attestation v0.5.20, the `LeaseStore` behind ADR
  0001) — and because it is a session token by another name, which ADR 0001's threat model
  explicitly does not survive. Choosing it would have made #126 undeliverable against its own
  threat.
- **[ADR 0003 — MINJA procedure harness](docs/adr/0003-minja-procedure-harness.md)** is the design
  doc [#37](https://github.com/sattyamjjain/mnemo/issues/37) requires before any harness code
  (it is labelled `needs-design` and scopes itself out: ">500 LoC of attacker logic + an LLM
  budget"). Fixes the measurement protocol *in advance* — defense-delta not bare ASR, mandatory
  benign control arm, Wilson-95, structural success oracle — and records the uncomfortable
  prediction up front: **the z-score lane will probably catch nothing**, because a
  Phase-2-shortened record is engineered to look ordinary and mnemo's own prior finding puts
  poisoned content at ~1.5σ against a 3.0σ default. Writing the expected negative down now removes
  the temptation to redefine success after seeing the data.
- Filed [#156](https://github.com/sattyamjjain/mnemo/issues/156) for an untracked gap found while
  auditing: `mnemo_graph::extract::TemporalEdge::extract` returns an empty `Vec` and never reads
  the `MNEMO_GRAPH_EXTRACT_MODEL` env var its own docstring documents — so the bitemporal graph
  layer has no edge extraction, and nothing tracked it.

### Fixed (2026-08-14) - `mnemo-amp` joins the publish walks; two private docs can no longer be committed by accident

- **`mnemo-amp` was publishable, absent from crates.io, and in NO publish walk** — nothing would
  ever have shipped it. This is the orphan case `registry_parity.sh` started warning about hours
  earlier, acted on rather than left as a standing warning. It is added to **both** walks the way
  `mnemo-db` already appears in both: `cargo-publish.yml`'s library list, and `release-crate.yml`'s
  `WALK` **plus its clippy gate, test gate and coordinated dry-run** — a crate in the walk that the
  gate never compiled would be published untested, which is a worse bug than the one being fixed.
  It depends only on `mnemo-core`, so it is topologically safe anywhere after it. It is a NEW crate,
  so the first release that includes it needs the token's `publish-new` scope.
- **`MNEMO_STRATEGY_PRIVATE.md` and `KILL_CRITERIA.md` are now gitignored.** Both declare
  themselves PRIVATE in their own first lines, and both had been sitting untracked *but unignored*
  in a public repo — one `git add -A` (which this repo's own conventions call for) away from being
  published irreversibly.
- **Two stale workflow comments corrected**, both of which asserted things that are now false:
  `release-crate.yml`'s STATUS block still said the line was unshipped and blamed an expired token,
  and `cargo-publish.yml` still explained `mnemo-mcp-server`'s exclusion by an unpublishable
  `mnemo-embeddings-bench`. The corrected STATUS records the real diagnosis — **the token was never
  the blocker; it carries both scopes** — and the operational consequence: `RELEASE_TOKEN_HAS_PUBLISH_NEW`
  is unset, so a bare tag push aborts at the new-crate gate and a release containing a new crate
  must be dispatched with `-f confirm_publish_new=true`.

### Fixed (2026-08-14) - #140 is closed: the whole 0.5.23 line is on crates.io, binary included

`mnemo-mcp-server` had been stranded at **0.4.4 since 2026-05-18 — 87 days** while the libraries
moved to 0.5.22. It is now published at **0.5.23**, along with `mnemo-embeddings-bench`, the new
crate it depends on that had never been created. **20 of 21 publishable crates are at 0.5.23.**

Acceptance test, run against the live registry in a scratch project:

```
$ cargo add mnemo-mcp-server
      Adding mnemo-mcp-server v0.5.23 to dependencies
```

- **The token was never the whole story.** It turned out to carry both `publish-update` (the
  libraries published on the first attempt) *and* `publish-new` (it created
  `mnemo-embeddings-bench`). What actually blocked the release was the walk structure plus the
  unconfirmed `publish-new` gate — precisely the "a skip is indistinguishable from a no-op"
  failure this window's work was written to expose.
- **The stale-install notes are removed** from `README.md`, `crates/mnemo-mcp/README.md` and
  `sdks/typescript/README.md`, as their `STALE-PUBLISH-NOTE(#140)` markers specified.
- **`scripts/gen_published_versions.py`** now derives "released" vs "unreleased target" from the
  live registries instead of a hand-maintained word, and no longer describes
  `mnemo-embeddings-bench` as blocking the server publish. The drift baseline is refreshed to the
  resolved state (19 crates at 0.5.23), so `check_version_drift.sh` is green on a true state
  rather than an acknowledged gap.
- **Still open, and now visible by name:** `mnemo-amp` is publishable, absent from crates.io, and
  in **no** publish walk — the orphan warning added this window flags it on every release. Either
  add it to the `release-crate.yml` walk or set `publish = false` to make the intent explicit.

### Fixed (2026-08-14) - the new preflight blocked the library publish it was meant to protect

The `v0.5.23` release exposed a real bug in the registry-parity preflight shipped hours earlier
in [#155](https://github.com/sattyamjjain/mnemo/pull/155). It failed `cargo-publish.yml` with:

```
publish preflight FAILED — mnemo-mcp-server: crates.io=0.4.4 is more than one patch
behind workspace=0.5.23, and this walk does not publish it
```

That veto was wrong. `cargo-publish.yml` is the **library** path and *deliberately excludes*
`mnemo-mcp-server` (its own comment says so — the server has a path-only dep on
`mnemo-embeddings-bench`); only the tag path in `release-crate.yml` can publish it. So the gate
made one release path responsible for a crate it structurally cannot ship, blocking the 0.5.23
libraries over an unrelated crate — coupling two independent paths and leaving the registry
*further* behind, the opposite of the intent.

- **Preflight now classifies instead of vetoing**: lagging + in the walk → `REPAIRING`, proceed;
  lagging + not in the walk → loud `::warning::` by name, proceed; absent + in no walk → warning.
  It still hard-fails on the two states where proceeding would be dishonest: a registry **ahead**
  of the repo, and a registry that is **unreachable** (never treat an unknown state as agreement).
- **The teeth moved to where they belong**: `--mode assert`, which runs *after* the walk and
  hard-fails if a crate the walk **was** responsible for did not land. That is the check that
  actually catches #140's silent skip. A pre-publish veto on an out-of-scope crate only blocks
  good releases; a post-publish assertion catches the bad ones.
- Both publish jobs now check out with `fetch-depth: 0`, so the triple's **newest git tag** column
  reports a real tag instead of `none` on the default shallow checkout.

## [0.5.23] - 2026-08-14

### Landing trace (2026-08-10)

The 0.5.23 window opened on the **v0.5.22** release — `main` at
[`608d913`](https://github.com/sattyamjjain/mnemo/commit/608d913) (the version-fence
merge the `v0.5.22` tag points at). The entries below accumulate on top of it toward the
0.5.23 cut. Provenance + FORGET BY PROVENANCE merged in [#154](https://github.com/sattyamjjain/mnemo/pull/154)
(`main` at [`2c8bf35`](https://github.com/sattyamjjain/mnemo/commit/2c8bf35)); the
2026-08-12 entries build on it.

### Fixed (2026-08-14) - the publish walk fails loudly instead of skipping silently (#140)

`mnemo-mcp-server` sat at **0.4.4 on crates.io for 87 days** (published 2026-05-18) while the
libraries advanced to 0.5.22, and **every publish run in that window reported success**. The rejected
`CARGO_REGISTRY_TOKEN` caused it, but the reason nobody noticed is a CI design bug that outlives any
token: the walk enumerated crates, *skipped* the ones it could not publish, and a skip was
indistinguishable from a no-op.

- **[`scripts/registry_parity.sh`](scripts/registry_parity.sh)** — one implementation, two modes.
  `--mode preflight` runs BEFORE the walk and prints the full triple (workspace version, newest git
  tag, crates.io version) for **every** publishable crate; `--mode assert` runs after and requires
  every crate to have actually landed. This **absorbs `assert_release_parity.sh`**, which is deleted:
  the parity idea now has exactly one implementation, not two.
- **Preflight is walk-aware, on purpose.** A naive "fail on any lag" gate is unshippable — it would
  block the very publish that repairs the lag. A lagging crate **in** the walk prints a loud
  REPAIRING line and proceeds; a lagging crate **not** in the walk fails the job by name with both
  versions.
- **A 403 / auth failure is never swallowed.** `cargo-publish.yml` now captures publish output and
  matches auth/permission failures explicitly, failing with the operator remediation instead of a
  generic error — plus a **publish ledger** (published / NOT published) printed on *every* exit, so a
  partial walk can never look like a complete one.
- **Orphan detection**: a publishable crate that is absent from crates.io *and* in no publish walk is
  now warned by name (`mnemo-amp` today) — the same shape as #140, caught before it becomes another
  87-day silence.
- Wired into both `cargo-publish.yml` and `release-crate.yml`. **Rotating the token remains an
  operator action** that no code change can perform; #140 stays open until it is done.

### Added (2026-08-14) - workspace version fence: every crate, tag and CHANGELOG must agree

Three version numbers coexisted across the workspace at once — `[workspace.package].version` 0.5.23,
`crates/mnemo-golem-wit` **0.5.21**, newest tag `v0.5.22` — and no guard could see it: the existing
checks look at *published* crates or at the README, so a hand-maintained literal on a crate in the
root `[workspace] exclude` was invisible to all of them.

- **[`crates/mnemo-cli/tests/workspace_version_fence.rs`](crates/mnemo-cli/tests/workspace_version_fence.rs)**
  — offline, and a **test** rather than a script so a mismatch is *unmergeable*, not merely reported.
  Five legs: every crate's literal version (32 manifests), every internal `{ path, version }` pin in
  the root `[workspace.dependencies]`, the newest git tag not being ahead of the workspace, tag ↔
  workspace ↔ CHANGELOG agreement, and a positive control proving the parsers are not vacuous.
- **Caught a real bug on its first run**: `mnemo-golem-wit` pinned 0.5.21 while the root Cargo.toml
  already pinned that same crate at 0.5.23 — bumped to 0.5.23 here.
- **Exact three-way equality is enforced at release, not at every commit.** Between a bump and its
  release the repo is legitimately ahead of the newest tag (the state right now), and a fence that is
  red on every pre-release commit is a fence everyone learns to ignore. So: `tag == workspace` requires
  a `## [workspace]` CHANGELOG section (full equality); `tag < workspace` requires an open
  `[Unreleased]`; `tag > workspace` is always illegal. There is no third number in either state.
- New `version-fence` CI job with `fetch-depth: 0`, because the default shallow checkout fetches no
  tags and the tag legs skip rather than false-fail when none are reachable.

### Docs (2026-08-14) - MCP 2026-07-28 readiness audit, written before any code moves

- **[`docs/mcp-2026-07-28-migration.md`](docs/mcp-2026-07-28-migration.md)** — per-item audit with a
  status of not-affected / needs-change / needs-design, every claim traced to a named module and every
  "not-affected" verified as an absence rather than assumed.
- **Session IDs — needs-design.** `rmcp` is compiled `["server", "transport-io"]`: stdio only, so there
  is no `Mcp-Session-Id` to remove. The role a session would play is played by the **OS process**:
  `engine.default_agent_id` (boot-time, and the scoping key in `server.rs::list_resources`) and
  `role_filter.rs` (a boot-time, server-wide denylist, not per-caller RBAC). Stateless relocates that
  state to the store — where `WriteProvenance`, `model::capability`, `model::acl` and `core::auth`
  already live — and at that point **the auth boundary on the store becomes the security model, whole
  and alone**, because the process boundary that does the work today is deleted.
- **Roots / sampling / logging — not-affected**, each verified: no `roots` reference and capabilities
  are exactly `enable_tools().enable_resources()`; no `sampling`/`create_message` (and
  `mnemo-graph::extract` is still a stub that never reads `MNEMO_GRAPH_EXTRACT_MODEL`); no MCP
  `logging` — diagnostics go to stderr, which is the only correct channel on stdio.
- **DCR → CIMD — needs-design.** No client registration exists to migrate from; on stdio the trust
  decision is `safe_spawn.rs`'s parent-process gauntlet. Documents what a CIMD metadata document must
  carry, and states the consequence plainly: **CIMD relocates the trust anchor to a URL the client
  hosts**, making freshness (fetch/cache/TTL) and revocation (fail-closed vs serve-stale when the URL
  is unreachable) *runtime* concerns. For a memory server a stale CIMD document is a live credential.
- **Nothing is implemented.** The document is the map that makes the next PR small.

### Docs (2026-08-14) - install docs no longer hand a user a stale binary

- The `cargo install mnemo-mcp-server` line in [`README.md`](README.md),
  [`crates/mnemo-mcp/README.md`](crates/mnemo-mcp/README.md) and
  [`sdks/typescript/README.md`](sdks/typescript/README.md) now states the **actually published version
  and date (`0.4.4`, 2026-05-18)** next to the command, with the build-from-source alternative. Each
  note carries a `STALE-PUBLISH-NOTE(#140)` marker so the PR that publishes the fix can delete them by
  grep.
- **Verified no doc tells a user to `cargo add mnemo`** — that name on crates.io is an unrelated
  project at 1.0.0. (`crates/mnemo-db` remains the defensive name-reservation pointer.)
- Corrected the stale **"21 registered tools"** claim to **23** in README, the TypeScript SDK README,
  and the tools/compliance docs — `crates/mnemo-mcp/tests/mcp_test.rs` has asserted 23 since the
  provenance tools landed, so the prose had been contradicting a passing test.

### Added (2026-08-12) - write-time opaque-reasoning-payload flag (arXiv:2608.09867)

Stored memory can now contain decodable credentials — [arXiv:2608.09867](https://arxiv.org/abs/2608.09867)
(2026-08-10) showed provider-returned encrypted reasoning blocks carry no session/user/model binding,
and that 315,320 scraped blocks yielded 367 PII artifacts and 182 credentials. An agent that REMEMBERs a
raw assistant turn plausibly persists one.

- **Write-time SHAPE detector** (`mnemo_core::opaque_reasoning`): on REMEMBER, flags content matching the
  shape of a provider opaque reasoning payload (structured `reasoning`/`redacted_thinking` blocks, or a
  long high-entropy base64-ish blob). **Shape only — never decodes, takes no dependency that could**, and
  a flag is NOT proof a secret is present.
- **Wired to the existing provenance model** (not a new one): a flagged write records
  `WriteFlag::OpaqueReasoningPayload` on its `WriteProvenance`, alongside the principal and session — so
  `forget_by_principal` / `forget_by_session` sweep it for free. The flag is **hashed into**
  `content_hash` (tamper-evident: it cannot be stripped without breaking the chain). Persistence version
  5 → 6 (`write_provenance.flags`), migrated additively on DuckDB and PostgreSQL.
- **Default warn-and-record, not reject** — a `tracing::warn!` + the recorded flag; the write is stored so
  it can be revoked, not silently dropped.
- Surfaced on the provenance read path everywhere (REST/MCP/Python/TS/Go `flags`). Documented in
  [`docs/security/opaque-reasoning-payloads.md`](docs/security/opaque-reasoning-payloads.md) and both SDK
  READMEs, with the limit stated plainly.

### Added (2026-08-12) - the Salami fixture now has a committed number (#37)

`bench/salami_poisoning` landed 2026-08-11 as a compositional-poisoning fixture (arXiv:2608.01637) with
**no committed result** — the same shape of gap as an unrun protocol. Now run and recorded:

- **Dated result** [`bench/salami_poisoning/results/salami_2026-08-12.{json,md}`](bench/salami_poisoning/results/salami_2026-08-12.md)
  with the exact command, determinism note (no RNG; background varies by trial index), embedder
  (`DeterministicEmbedding`, offline), and machine (Apple M4, rustc 1.97.0).
- **Numbers** (n=200/arm): individually-benign slices **save at 100%** [Wilson 95% 99.5, 100] (no per-write
  control rejects them) and one trigger recall **assembles the harm at 100%** [98.1, 100]; the topic-matched
  **benign control co-retrieves identically (mean 4.0 slices) but assembles 0%** [0, 1.9].
- **Threshold sweep** (new `run_threshold_sweep`): the aggregate crosses only on the **4th** benign write
  (1–3 slices → 0% assembly, 4 → 100%) — the harm appears on the last write, no gradual ramp.
- **Honest reading** in the result file: how many writes before crossing (4, the full set), what each
  per-record filter did (nothing — save path, opaque-payload flag, and z-score lane all pass an
  individually-benign slice), and what the number does NOT show (not a defense; lexical not semantic;
  structural harm oracle not an LLM; compositional subset of #37 only).

### Fixed (2026-08-12) - the release pipeline now catches registry drift early and across all three registries

Three registries disagreed with `main` and with each other (crates.io `mnemo-mcp-server` 0.4.4 while the
workspace is 0.5.23; `mnemo-embeddings-bench` uncreated; npm `@mndfreek/mnemo-sdk` package.json 0.4.8 vs
published 0.4.4). The pipeline is fixed, not just the numbers:

- **Hard, early new-crate gate** in [`release-crate.yml`](.github/workflows/release-crate.yml): when the
  publish walk contains a NEW crate, the release now **fails before the ~30-min build** unless the operator
  has confirmed the token carries `publish-new` (a `confirm_publish_new` dispatch input, or the
  `RELEASE_TOKEN_HAS_PUBLISH_NEW` repo variable), with exact remediation in the failure. crates.io exposes
  no token-scope API, so this human ack is the honest early signal — and it means `mnemo-mcp-server` can
  never again strand behind an uncreatable dependency.
- **Cross-registry release parity** ([`scripts/registry_parity.sh`](scripts/registry_parity.sh) `--mode assert`):
  after publish, asserts the workspace version, the **git tag**, and every crates.io closure crate agree;
  and checks the independently-versioned SDK artifacts — **PyPI `mnemo-db`** and **npm `@mndfreek/mnemo-sdk`**
  — against their OWN published versions (per `python/pyproject.toml`, these version independently, so they
  are NOT forced equal to the workspace). A registry ahead of the repo is drift (fail); an unpublished bump
  is a pending-publish warning; all numbers are printed.
- **Generated per-registry versions row** in the README
  ([`scripts/gen_published_versions.py`](scripts/gen_published_versions.py)): the published version + date
  for each registry, generated from the live registries rather than typed.

### Added (2026-08-11) - memory-write provenance, revocation by principal, and a minimal capability primitive

Every memory write now records **who wrote it, under what authority, in what session, when** —
queryable, tamper-evident, and revocable by the responsible principal. mnemo stays standalone (no
external audit dependency).

- **Write-provenance record per memory** (`model::write_provenance::WriteProvenance`): writing
  `principal`, optional `capability_id`, optional `session_id`, `op` (remember/share), timestamp,
  and a SHA-256 `content_hash` + `prev_hash` forming an append chain (tamper-evidence via
  `verify_provenance_chain`). Recorded on both the REMEMBER and SHARE write paths.
- **Queryable**: memory ID → provenance; principal/session → everything it wrote. New
  `StorageBackend` methods with graceful no-op defaults, implemented on **both DuckDB and
  PostgreSQL** (schema migration added to each; DuckDB persistence version bumped to 5).
- **FORGET BY PROVENANCE** (`forget_by_principal` / `forget_by_session`): revoke everything a
  principal or session authored in one call (soft/hard/redact). Targeted remediation, not a wipe —
  the provenance audit trail survives the erasure.
- **Minimal capability primitive** (`model::capability`, part of #126): HMAC-signed
  `Capability { principal, scope, expiry }` + `CapabilityIssuer::issue/verify`, so the provenance
  `authority` field references a real, verifiable token rather than a recorded string.
  `remember_with_capability` verifies before writing.
- **Surfaced everywhere**: REST (`/v1/memories/{id}/provenance`, `/v1/provenance/{principal,session,verify,forget}`),
  MCP tools (`mnemo.provenance`, `mnemo.forget_by_provenance` — registered tool count 21 → 23), and
  all three SDKs (Python native, TypeScript + Go MCP clients) expose provenance read **and** FORGET
  BY PROVENANCE.

### Added (2026-08-11) - compositional ("Salami") poisoning fixture advances #37 (arXiv:2608.01637)

- **`bench/salami_poisoning`** — the compositional case of issue #37: N individually-benign
  memories that are collectively harmful (the "Salami" shape, arXiv:2608.01637), plus a benign
  control that shares the surface topic but must NOT complete the harm. Reports a write-path
  **save rate** and a **retrieval-influence (assembly) rate**, each with a Wilson 95% interval — a
  measurement, not a pass/fail gate. Deterministic + offline (lexical co-retrieval). Covers the
  compositional subset of #37 only; semantic-paraphrase, adaptive, and cross-session-drip variants
  remain open (see the #37 comment).

### Fixed (2026-08-11) - the publish stranding is now caught loudly; the "tag never pushed" note below is superseded

The 2026-08-09 note below says the `v0.5.22` tag "was never pushed." True when written, but the tag
was pushed right after: `release-crate.yml` ran three times and failed at the token preflight (`403`
from `/api/v1/me` — the signature of a granular/scoped token, since the libraries publish fine). The
real remaining blocker is that `mnemo-embeddings-bench` is a new crate needing the token's
`publish-new` scope; `mnemo-mcp-server` (0.4.4) depends on it, so it cannot publish until the bench
does. See #140.

- **New-crate preflight verified**, not just trusted: its exact logic, run against live crates.io,
  flags `mnemo-embeddings-bench` as the new crate that needs `publish-new`.
- **A hard, un-baselined post-publish gate** wired into `release-crate.yml` (now
  [`scripts/registry_parity.sh`](scripts/registry_parity.sh) `--mode assert`; it landed as the
  separate `assert_release_parity.sh` and was folded into the one parity implementation later in
  this same window): after the walk it asserts every closure crate reached the workspace version
  and fails the release if any lags. The stranding was invisible for multiple releases because
  nothing asserted the publish fully landed; this makes a partial publish loud. Skipped on dry-runs.

### Fixed (2026-08-09) - release-crate.yml flags new crates that need the publish-new token scope (#140)

`#140` blamed `mnemo-mcp-server` being stuck at 0.4.4 on "a rejected
`CARGO_REGISTRY_TOKEN` (403)". That is wrong: the nine 0.5.22 libraries published on 2026-08-08
(via `cargo-publish.yml`), so the token authenticates. The real chain is (1) the `v0.5.22` tag was
never pushed, so the tag-triggered `release-crate.yml` — the only path that publishes
`mnemo-embeddings-bench` + `mnemo-mcp-server` — never ran; and (2) even when it does run,
`mnemo-embeddings-bench` is a **new** crate, and creating a crate needs the token's `publish-new`
scope (not just `publish-update`). The existing `/api/v1/me` preflight proves the token
authenticates but not that it can create a crate.

- Added a **new-crate preflight** to `release-crate.yml`: it GETs each crate in the walk and
  surfaces every NEW (404) crate up front with the `publish-new` requirement, so the operator
  confirms the scope BEFORE the walk burns ~30 minutes and dies creating the first new crate.
- The walk order is now a single job-level `WALK` env used by both the preflight and the real
  publish loop, so they cannot disagree about what the walk contains.
- The mid-walk auth-failure message now distinguishes a missing `publish-new` scope (new crate)
  from an expired/revoked token.
- **Softened the token preflight.** It previously HARD-failed when `/api/v1/me` returned non-200,
  which false-blocks a valid **granular/scoped** crates.io token — such a token has no
  account-read scope and 403s on `/me` yet publishes fine (the 0.5.22 libraries shipped
  2026-08-08 with exactly such a token, via `cargo-publish.yml`, which has no `/me` check). The
  preflight now hard-fails only on a **missing** secret; a `/me` non-200 is a warning and the real
  publish is the authority (a genuinely dead token fails at the first upload with a specific error).
  This is what stranded the `v0.5.22` walk at the preflight even though the token can publish.

### Fixed (2026-08-09) - a crates.io publish can no longer happen without a matching tag

The 0.5.22 library line published to crates.io from the **push-to-main** path (`cargo-publish.yml`)
with **no `v0.5.22` git tag** — so a crate on crates.io could not be bisected back to a commit, and
the tag-triggered `release-crate.yml` (the ONLY path that publishes `mnemo-embeddings-bench` +
`mnemo-mcp-server`) never ran, leaving those two stranded. `cargo-publish.yml` now refuses to
publish a version with no `v<ver>` tag on the remote:

- The gate fires **only when the publish queue is non-empty** — a doc-only push has nothing to
  publish and stays a clean no-op.
- It checks the remote directly with `git ls-remote --tags`, so it works with the default shallow
  checkout (no full-history fetch).
- Verified against the live remote: `v0.5.21` (present) → allowed, `v0.5.22` (absent) → blocked.

The `v0.5.21` and `v0.5.22` tags + GitHub Releases are reconciled as a release step (the 0.5.21
changelog already apologised for this exact untagged-publish defect class; this closes it).

### Docs (2026-08-09) - stated exactly what the Python SDK 0.5.12 is wire-compatible with

The README's Python "Version line" note called the 0.5.12-vs-0.5.22 gap "expected, not skew" but
left the actual compatibility implicit. Made it explicit for both surfaces of the SDK, rather than
cutting a 0.5.13 PyPI release (publishing is an operator action):

- **In-process `MnemoClient` (the PyO3 extension)** embeds the engine, so `mnemo-db` 0.5.12 *is*
  `mnemo-core` 0.5.12 — a month behind the 0.5.22 workspace; it lacks engine changes from
  0.5.13–0.5.22 (for example, the v0.5.17 forged-reasoning recall defense).
- **The MCP adapters** (`agno` / `camel` / `agno-memory`) do not embed a server — they spawn the
  external `mnemo mcp-server` binary and bind to its MCP **tool surface** (the 21 registered tools),
  not a specific `mnemo-core` version. So they are wire-compatible with any 0.5.x `mnemo-mcp-server`
  (build 0.5.22 from source, #140). The rmcp 2.2 → 3.0 transport migration (0.5.22) and the v0.5.20
  tool-catalog attestation are properties of that server binary, not the SDK.

### Fixed (2026-08-09) - the version-drift guard was blind to npm, and the npm publish failed silently on a bad token

`scripts/check_version_drift.sh` watched crates.io but not npm, so
`sdks/typescript/package.json` drifting ahead of the published `@mndfreek/mnemo-sdk` went
unnoticed — it is on **0.4.8** while npm is stuck at **0.4.4**. Separately, `npm-publish.yml`
failed on every push to main since the 0.4.4 release: each run built the tarball, passed tests,
type-checked, and signed a provenance statement to the public transparency log — then the final
`npm publish` PUT died with a bare `404 Not Found` (npm's response for a scoped package when the
token is invalid or lacks write access).

- **Drift guard.** Added an npm block to `check_version_drift.sh` with the same baselined semantics
  as the crates.io check: GREEN on the acknowledged standing gap (`package.json` 0.4.8 vs npm
  0.4.4), RED only on a NEW divergence — a further `package.json` bump without an npm publish. A
  positive-control run confirmed both paths. The baseline (`version-drift-baseline.json`) now
  records the npm state, and its stale crates entries were refreshed 0.5.21 → 0.5.22 to match
  crates.io.
- **Publish preflight.** `npm-publish.yml` now validates `NPM_TOKEN` with `npm whoami` BEFORE
  building or signing anything, failing fast with the operator fix (rotate the token) instead of the
  cryptic post-provenance 404. The token itself is an operator action; no code change publishes the
  stranded 0.4.5–0.4.8 versions.
### Fixed (2026-08-09) - the README contradicted itself on the current release, and the fence could not see it

The release/install block stated crates `resolve 0.5.21` and that `cargo install mnemo-mcp-server`
gives the binary "not 0.5.21", while its own `Current release:` heading — and crates.io — were both
**0.5.22**. The existing `readme_crates_version_matches_workspace.rs` pinned only the single
`Current release:` line, so two stale `0.5.21` literals three sentences away passed unnoticed.

- **Prose.** Both literals corrected to `0.5.22`. The same block blamed the stranded
  `mnemo-mcp-server` on "a rejected registry token"; that is disproven — nine library crates
  published 0.5.22 on 2026-08-08, so the token authenticates. Reworded to the real cause: the server
  depends on `mnemo-embeddings-bench`, a new crate not yet on crates.io, so the publish walk cannot
  finish (#140).
- **Fence.** Extended the test with a band guard that fails on any *bare* `0.5.2x` release literal
  anywhere in the README that is not the workspace version. `v`-prefixed feature history (`wired in
  v0.5.20`, `New in v0.4.0`) is exempt — those are true statements about the past and must not be
  rewritten when the workspace climbs into their patch band. A positive-control test proves the fence
  fires on a stale bare literal and skips the `v`-prefixed form, so it cannot rot into a vacuous pass.

### Fixed (2026-08-10) - the two stranded packages are wired to ship, and npm rides the same release tag

- `mnemo-mcp-server` was stranded at 0.4.4 on crates.io while the workspace moved to 0.5.23.
  It (and its new dependency `mnemo-embeddings-bench`) is wired into the tag-gated release job,
  and the new-crate preflight surfaces the `publish-new` token scope it needs; it ships with the
  rest of the line once `CARGO_REGISTRY_TOKEN` carries `publish-new`. Tracked in #140 (not closed
  here — the server is still 0.4.4 until an operator publishes).
- The TypeScript SDK was four patches ahead of npm (`package.json` 0.4.8 vs npm 0.4.4). No
  breaking change: the 0.4.5-0.4.8 bumps are dev-dependency / tooling (TS 5.9 -> 6.0), the
  `MnemoClient` API and the runtime dep are unchanged. `npm publish` is now gated on the release
  tag, the same `v*.*.*` event that drives the crates.io release, instead of firing on every push.

### Added (2026-08-10) - read-time provenance, documented honestly

- `docs/security/read-time-provenance.md`: what provenance mnemo can and cannot prove at recall
  time, written from the code. States plainly that a recalled tool-return is not provably separable
  from a user entry today - `source_type` is optional, defaults to `Agent`, is outside the hash
  chain, and is not carried in the read receipt.

### Changed (2026-08-10) - a release can no longer skip its changelog section

- `cargo-publish.yml` now refuses to publish a version whose `## [X.Y.Z]` CHANGELOG heading is
  missing - a sibling to the existing matching-tag gate. 0.5.22 shipped to crates.io with no
  changelog section (now cut, below); this makes that impossible to repeat. Bumping the workspace
  to 0.5.23 without cutting `## [0.5.23]` will (correctly) block the publish until the section
  exists.

## [0.5.22] - 2026-08-09

The **rmcp 2.2 to 3.0** MCP-runtime migration (#136) is the substantive change in this cut: the workspace moved 0.5.21 -> 0.5.22 with it. The rest is release-engineering and docs-honesty work (publish-parity guards, the deployed mdBook site, the single benchmark index).

### Development trace

The 0.5.22 cut opened on the **v0.5.21** release. Its base is `main` at
[`f302d98`](https://github.com/sattyamjjain/mnemo/commit/f302d98c8)
(the `release-crate.yml` retry-hardening merge); the entries below accumulated on
top of it and shipped as 0.5.22 (crates.io 2026-08-08, tag `v0.5.22` at `608d913`).

### Fixed (2026-08-01) — `main` has not built since 2026-05-21; the golem WIT cdylib was in the default workspace build

`cargo build --workspace` (the CLI's CI **Build** job) had been red on every
push for **72 days** — last green was run
[26205093362](https://github.com/sattyamjjain/mnemo/actions/runs/26205093362) @
`7370bc0` (2026-05-21); it went red at run
[26213447616](https://github.com/sattyamjjain/mnemo/actions/runs/26213447616) @
`69a8ca6`, the commit that added `crates/mnemo-golem-wit` and
`crates/mnemo-golem-host` to `[workspace] members`.

- **Cause.** `mnemo-golem-wit` is a `cdylib` WASM *component*. A workspace build
  must produce a **native** dynamic library for every member cdylib, and this
  one cannot link natively — its WIT host imports have no native definition:
  `Undefined symbols … _cabi_post_mnemo:golem-vector/vectors@0.1.0`. Clippy
  emits metadata and never links, so Clippy stayed green while Build stayed red —
  the asymmetry that let this hide for 72 days.
- **Fix.** Moved `crates/mnemo-golem-wit` from `[workspace] members` to
  `[workspace] exclude`. Because an excluded crate is outside the workspace (it
  can neither inherit `[workspace.package]` nor be reached by `-p` from the
  root), it was made self-contained — its own empty `[workspace]` table plus
  explicit `version`/`edition`/`license`/`repository` — so it still builds
  standalone with `cargo component build --release --manifest-path
  crates/mnemo-golem-wit/Cargo.toml --target wasm32-wasip2` (verified). It is not
  orphaned. `mnemo-golem-host` stays a member — it is a plain native crate and
  does **not** depend on `mnemo-golem-wit`, so nothing else moves.
- **Fence.** New `crates/mnemo-cli/tests/workspace_builds_clean.rs` fails if any
  `[workspace] members` entry declares a `cdylib` crate-type (allow-listing only
  the maturin-built `mnemo-python`), so this class of regression cannot silently
  return. A maintainer note in `ci.yml` documents the exclusion alongside the
  existing mnemo-python / mnemo-grpc / onnx notes.
- **Two latent failures the green build unmasked** (both never-run since 2026-05-21,
  fixed here so `cargo test --workspace` is actually green, not just compiling):
  `mnemo-golem-host`'s 3 lib tests called `strategy = "semantic"` recall under
  `NoopEmbedding`, which the v0.5.13 fail-loud guard now refuses — swapped the test
  engine to the workspace-standard `DeterministicEmbedding` (the assertions check id
  membership / collection scoping, not scores, so they are unchanged). And
  `readme_crate_claims_are_real.rs` treated only `members` as real crates, so moving
  `mnemo-golem-wit` to `exclude` made the README's by-path reference to it read as a
  phantom crate — the fence now counts `exclude` entries as real too.

### Changed (2026-08-01) — contributor docs reconciled with what CI actually runs

`CLAUDE.md`'s auto-managed build-commands block told contributors (human or agent)
to run commands CI never runs: `cargo build --all` / `cargo test --all` (which do
**not** exclude the maturin-only `mnemo-python`) and `cargo clippy --all-targets`
with the full feature set (which turns on the broken `onnx`/`ort` feature that
ci.yml deliberately omits, issue #125). Replaced the three with the exact strings
CI runs (`--workspace --exclude mnemo-python`), added the feature-set/#125 and
`protoc` caveats under the lint command, corrected the CLI package name in two
commands (`-p mnemo-cli` → `-p mnemo-mcp-server`; the crate is published as
`mnemo-mcp-server`), fixed the stale "CI enforces" bullet in Git Insights, and
deleted the unverifiable "132 tests" parenthetical that nobody could reproduce
(the workspace suite now reports 567). Fixed the `python/pyproject.toml` comment's
stale "currently 0.5.18" → "0.5.21"; the independent PyPI `version = "0.5.12"` is
correct and untouched.

### Fixed (2026-08-02) — the tag-gated publish walk fails fast and loud on an expired token; drift guard now names every stranded crate

**Why the whole 0.5.x line is stranded on crates.io.** `v0.5.17`, `v0.5.18`, and
`v0.5.20` were all tagged; **none reached crates.io**. Every `release-crate.yml`
walk packaged + verify-built `mnemo-core` (~5 min) and then died on the upload with
`403 Forbidden: authentication failed`, after burning five retry attempts. The
cause is a single **expired `CARGO_REGISTRY_TOKEN`** (last rotated 2026-04-25,
~90-day expiry ≈ 2026-07-24 — `v0.5.16` published 2026-07-24 was the last that beat
it). The earlier timeout / 5xx-retry commits treated it as transient; it is not.
Today's registry state is the result: `mnemo-core`/`-mcp`/`-compliance`/
`-attention-state`/`-db` stuck at **0.5.16** (2026-07-24), `mnemo-graph` at **0.4.5**,
and thirteen crates (`-postgres`, `-rest`, `-grpc`, `-admin`, `-pgwire`, `-letta`,
`-mesh`, `-codemode`, `-deal`, `-md-sync`, `-cma`, `-baseline`, `mnemo-mcp-server`)
at **0.4.4** (2026-05-18) — `cargo add mnemo-postgres` still resolves the pre-#99
pgvector version.

- **`release-crate.yml`:** a token **preflight** now validates
  `CARGO_REGISTRY_TOKEN` against `crates.io/api/v1/me` *before building anything*
  and, if it is missing or rejected, fails in seconds with an explicit
  operator-action message and a job-summary block (rotate the token) — instead of
  spending ~10 min to rediscover the 403 mid-upload. The walk (already topological
  + idempotent + resumable via the 404-skip) now prints a **published / already-present
  / NOT-published ledger on every exit**, so a partial publish can never report
  success, and short-circuits a mid-walk auth failure instead of retrying it.
- **`scripts/check_version_drift.sh`:** the failure output is now a side-by-side
  table (crate · repo version · crates.io version · status) plus a grouped tally —
  e.g. "13 crate(s) stranded at 0.4.4" — so an operator reading the red job sees the
  whole picture from the log without cloning. Pass/fail semantics unchanged.
- **`README.md`:** a dated note under the crates.io install block states the actual
  newest published version (0.5.16) while the publish is blocked, so the doc does not
  claim a current line the registry disagrees with. It is removed by the commit that
  lands the publish.
- **Still blocked on the operator:** rotating the token is not a code fix. Once
  `CARGO_REGISTRY_TOKEN` is refreshed, re-running the tag walk ships the whole line
  and this entry's registry lag clears.

### Changed (2026-08-02) — batched the six open Dependabot bumps; fixed the getrandom 0.4 API break

Consolidated six stale Dependabot PRs (all red from the then-broken workspace build
plus the drift guard) into one signed-off change: `getrandom` 0.2→0.4 (its 0.3+ drop
of the free `getrandom()` fn broke `encryption.rs` — updated to `getrandom::fill()`),
`base64` 0.22→0.23, `duckdb` `=1.10504.0`→`=1.10505.0`, `wasmtime`+`wasmtime-wasi`
46→47 as a matched pair, and `actions/setup-python` v6→v7. Green on CI except the
documented drift guard; superseded and closed #117/#119/#120/#121/#122/#123.

### Fixed (2026-08-03) — the `onnx` feature was repaired but CI and the docs still said "broken"

The `onnx` feature of `mnemo-core` was, at some point, quietly migrated to the pinned
`ort 2.0.0-rc.11` / `ndarray 0.17` / `tokenizers 0.23` API — it **builds and its whole
`mnemo-core` test suite passes** under `--features onnx`. But nothing reverted the fallout
of the earlier breakage: CI still excluded it, the README still said the integration was
"broken," `onnx.rs`'s module doc said "currently excluded from CI," and a v0.3.0 checklist
still had "[ ] Repair onnx" open — while `CHANGELOG.md` line ~600 already said it was
migrated. A listed feature whose docs and CI disagree with its own code is the same class
of defect as a feature that silently does not work: someone reads "broken," never enables
it, and the working code rots. **It now builds again and is covered by CI** — a dedicated
`onnx feature` job (`.github/workflows/ci.yml`) runs `cargo build`+`test -p mnemo-core
--features onnx` so it cannot silently rot again — and the five stale doc/CI locations were
reconciled to match. (The feature is kept out of the *workspace-wide* jobs only because
`ort` compiles ONNX Runtime and downloads a native lib, not because it is broken.) The one
open item on [#125](https://github.com/sattyamjjain/mnemo/issues/125) is a model-fetch CI
job to make the ONNX MiniLM **recall number itself** reproducible (end-to-end inference
needs a real model on disk, which the build+test job does not fetch).

### Added (2026-08-03) — the public benchmark page has mnemo's own numbers in it (closes #44)

`docs/benchmarks/2026-04-25-mnemo-v0.3.4.md` shipped in April 2026 with mnemo's rows
`_pending_`, waiting on an authenticated LLM-judged nightly run that never landed (the
OpenAI/Anthropic/gated-HF secrets in #44 were never wired). They are **populated now**,
produced by a **real embedder** (Ollama `nomic-embed-text`, 768-dim) via the credential-free
Rust `semantic_recall_bench` harness — the retrieval-quality metric `baseline.json` has
recorded since 2026-06-29 — on a fresh run on this machine, with the model, dimensionality,
dataset, hardware (Apple M4 / macOS 26.5.2 / rustc 1.97.0), date, and reproduce command on
the page. **Losses are published alongside wins:** the default `auto` RRF fusion loses to
pure `vector_only` on MRR (0.604 vs 0.806), and BM25 loses on every metric. The page is
reframed to state honestly that these are **retrieval** numbers, not the LLM-judged QA of
the reference leaderboard (a different axis). The bench itself gained **recall@10** and a
**run-to-run recall@5 spread** (it previously reported only the mean); `baseline.json` was
refreshed with both. `graph_boosted` and `p99` are left explicitly empty with one-line
reasons rather than estimated. Closes #44.

### Fixed (2026-08-04) - the drift guard was noise, and the onnx feature ran nowhere anyone could see

Three things, one theme: a check or a feature that looks fine but proves nothing, so a real
gap hides behind a green-looking surface.

- **The version-drift guard was red on every push, which taught everyone to ignore it.**
  The guard failed on any published crate more than one patch behind the workspace. That is
  correct, but the registry has been permanently behind since the tag-gated publish broke on
  an expired `CARGO_REGISTRY_TOKEN`, so the job was red on every single push. A check that is
  always red is worse than no check: the one time a NEW crate falls behind, nobody looks.
  Rebuilt it around a committed baseline (`scripts/version-drift-baseline.json`): green when
  the registry matches the workspace or the recorded baseline, red only when a new divergence
  appears (a new stranded crate, or the workspace version advanced past the baseline without
  publishing, which widens the pile). The red message now names each crate with its registry
  version, its workspace version, and the age of the gap in days. Refresh after a real publish
  with `bash scripts/check_version_drift.sh --update-baseline`.
- **The publish is still blocked, and this is why it stayed hidden.** The token preflight
  (added `b0f06f65`) confirms `CARGO_REGISTRY_TOKEN` is still rejected by crates.io (HTTP 403
  from `/api/v1/me`); the secret has not changed since 2026-04-25. Nothing published:
  `mnemo-core` stays 0.5.16, `mnemo-postgres` stays 0.4.4 (77 days behind). Rotating the token
  is an operator action, not a code change. The always-red drift guard above is part of why
  the 77-day gap read as normal for so long.
- **The `onnx` feature built and its tests passed, but nothing exercised the inference path.**
  The `--features onnx` tests only asserted that construction errors without a model on disk,
  so a build could ship where tokenize, ort inference, mean-pool, and normalise all silently
  never ran. That is the same latent-stub class as the Postgres semantic-recall stub fixed in
  v0.5.7. Added an end-to-end test (gated on `MNEMO_ONNX_MODEL_PATH`, skips green when unset)
  that embeds real text through `all-MiniLM-L6-v2` and asserts a 384-dim, finite, non-zero,
  L2-normalised vector where distinct sentences differ; the `onnx feature` CI job now downloads
  the model and runs it. Verified locally against the real model. Advances #125.
- **Recorded the capability-leased-reads design (#126) as `docs/adr/0001` instead of leaving
  it as folklore.** The dead `lease.rs` was removed earlier; the design is now written down as
  a dated ADR (the threat it defends, why a per-read lease rather than a session token, what it
  costs on the recall path, and the honest reason it is not built on a single-caller stdio
  transport). Not implemented, and labelled so, because an unbuilt feature in a dated ADR is
  credibility and an unbuilt feature on a capability list is the opposite.

### Added (2026-08-05) - a root SECURITY.md and a comparison against the crates people actually search

- A root `SECURITY.md`. A crate that positions on DPDP and HIPAA should say how to report a
  problem privately, and this one did not. It states supported versions, private reporting
  through GitHub security advisories, a first-response time that can be kept (10 working days,
  not 48 hours), which storage backends are in scope, and what is out of scope.
- `docs/comparisons/mem0-zep-letta.md`, the three names a 2026 search for agent memory returns
  first, with mnemo's own recall@1 of 0.739 from `bench/locomo` cited by path and an honest note
  on where it is behind: it loses the QA-accuracy row and the temporal-knowledge-graph row, and
  wins on-prem plus offline-verifiable tamper-evident audit.

### Fixed (2026-08-05) - the publish walk, validated end to end so it stops failing in silence

crates.io was serving `mnemo-postgres` 0.4.4 from May while the workspace was at 0.5.21, so
`cargo add mnemo-postgres` shipped code without the pgvector ANN backend. The token expired on
2026-04-25 and nothing failed loudly for months. This is the close of that hole, not a version
bump.

- The whole nine-crate walk was dry-run end to end (`cargo publish --dry-run` across
  core, graph, attention-state, compliance, mcp, postgres, rest, grpc, db in one coordinated
  invocation): every crate packages and verify-builds at 0.5.21, with no failure for any reason
  other than the upload itself. The ordered publish command list is recorded for the operator.
- The silent part is already fixed: the tag-gated `release-crate.yml` preflight (added
  2026-08-02) validates `CARGO_REGISTRY_TOKEN` against crates.io before building anything and
  aborts loudly on a bad token, and the drift guard (2026-08-04) is now signal rather than the
  permanent red it had become.
- What remains is not a code change: the registry token has to be a live, unrevoked crates.io
  token in the repo secret. As of this entry the secret is still being rejected (HTTP 403 from
  `/api/v1/me`), so nothing is published yet. Once the token is live, pushing the `v0.5.21` tag
  runs the validated walk and closes the loop.

### Fixed (2026-08-06) - the installable binary trails its own libraries, and the README understated the release

mnemo-core and mnemo-compliance are on crates.io at 0.5.21, but `cargo install mnemo-mcp-server`
still gives the 0.4.4 binary from May, so a stranger installs a `mnemo` that is behind the very
libraries it embeds. Root cause: the CLI crate depends on `mnemo-embeddings-bench` for the
`bench embeddings` subcommand, and that crate was `publish = false` with a path-only dependency,
which `cargo publish` refuses, so mnemo-mcp-server could never enter the publish walk. Fixed at
the source, with a guard so the drift is a failing check rather than a silent regression.

- `bench/embeddings` is now a *publishable* crate (no longer `publish = false`; version-pinned in
  the workspace) and, with mnemo-mcp-server, is in the `release-crate.yml` publish walk (topological
  order: embeddings-bench after core, mcp-server last). `cargo publish --dry-run` verify-builds both
  at 0.5.21. It is **not yet on crates.io** — like mnemo-mcp-server, its actual publish is blocked on
  the rejected token below. (This entry originally read "is now a published crate", which was wrong:
  the crate was made publishable, not published.)
- `scripts/check_version_drift.sh` gained a hard, non-baselined parity check that fails when
  published mnemo-mcp-server trails published mnemo-core by more than one patch. It is red now, on
  purpose (the binary really is behind), and goes green the moment the server publishes.
- Publishing itself is still blocked on the same operator step as the 2026-08-05 entry: the
  `CARGO_REGISTRY_TOKEN` secret is rejected by crates.io (403 from `/api/v1/me`). No code change
  fixes that; the parity guard is the honest stand-in until the token is live.
- The README carried a stale "newest on crates.io is 0.5.16" heads-up while crates.io was already
  at 0.5.21, understating the project. It is replaced with a single-sourced `Current release:
  0.5.21` line, fenced by `crates/mnemo-cli/tests/readme_crates_version_matches_workspace.rs`,
  which fails if the README number drifts from `[workspace.package].version`.

### Added (2026-08-06) - one benchmark index so every number has a command and a caveat

- `docs/benchmarks/index.md` is the single entry point for every published number: one table with
  the benchmark, its headline figure, the exact reproduction command, the raw-results file it was
  read from, and a "what this does not show" column. Numbers are transcribed from the existing
  result files, not re-run. Linked from the README benchmark section and the docs Performance page.
- Recorded, not reconciled, one cross-file delta found while building it: the README quotes
  semantic-recall `vector_only` MRR 0.805 from the 2026-06-22 run, while the 2026-08-03 refresh
  reports 0.806; recall@1 (0.739) is identical across both. The 0.001 gap is inside the bench's
  own sub-0.05 tie threshold and is noted in the index rather than papered over.

### Fixed (2026-08-08) - the README claimed a lockstep publish that is not happening

mnemo-mcp-server has been on 0.4.4 since 18 May while the libraries moved 17 patch releases ahead,
and the README said the line publishes in lockstep. It does not, and no code change makes it: the
publish walk is blocked on a registry token crates.io rejects (403 from `/api/v1/me`), the same
blocker recorded since the 2026-08-05 entry. The token cannot be rotated from the codebase, so
instead of leaving the README asserting something false for another day, the claim now states the
truth.

- The README lockstep sentence now says mnemo-mcp-server is behind at 0.4.4, `cargo install`
  gives the May binary, and you should build from source (`cargo build --release -p
  mnemo-mcp-server`) until the token is rotated. Tracking issue: #140.
- The 2026-08-06 entry's "`bench/embeddings` is now a published crate" was itself wrong — the
  crate was made *publishable* (not `publish = false`), not published; it is not on crates.io
  either, blocked on the same token. Corrected in place.
- The parity guard in `scripts/check_version_drift.sh` still fails on the mnemo-mcp-server-vs-core
  gap; it stays red until the server actually publishes, which is the point.

### Added (2026-08-08) - the mdBook docs are deployed instead of clone-only

- `docs/` was 73 files of mdBook readable only by cloning the repo — the compliance docs a
  regulated buyer needs were unreachable. `.github/workflows/docs.yml` now builds the book and
  deploys it to GitHub Pages on every push to `main`: <https://sattyamjjain.github.io/mnemo/>.
  Pages is enabled; the URL is in the README, `Cargo.toml` `documentation`, and the repo About.
- A dead internal link now fails the build before deploy (`scripts/check_docs_links.py`). It
  immediately caught two (`concepts/conflict-resolution.md`, `dpdpa-mannsetu.md`), now fixed.
  `book.toml` also pointed `git-repository-url` at a non-existent `mnemo-ai/mnemo` org (fixed to
  `sattyamjjain/mnemo`), and its mdBook-0.5-incompatible `multilingual` key was removed so the
  book actually builds.

### Changed (2026-08-08) - dependency bumps

- Batched the three patch Dependabot bumps: #131 `@modelcontextprotocol/sdk`, #133 `@swc/core`,
  #134 `@types/node` (all `sdks/typescript`).
- Bumped `agent-audit-kit` straight to the current `v0.3.68` in `security.yml`, past the stale
  Dependabot #132 (0.3.60 -> 0.3.62), which was closed.
- Bumped `serial_test` 3 -> 4 (#135, closed in favour of this branch). The 4.0 break is
  internal/MSRV; the `#[serial_test::serial]` usage in `mnemo-admin` tests is unchanged.
- `rmcp` 2.2 -> 3.0 (#136) is a transport-crate major (removed deprecated v3 APIs, DiscoverResult
  and OAuth-error changes) affecting ~150 call sites in `mnemo-mcp`; it is being migrated on its
  own branch and is intentionally **not** part of this change. See the rmcp entry below for the
  migration itself.

### Changed (2026-08-08) - rmcp 2.2 -> 3.0 transport migration (own branch)

The MCP transport crate (`rmcp`) was bumped 2.2 -> 3.0 (resolves 3.1.2). Despite ~150 `rmcp`
references in `mnemo-mcp`, the actual break surface is three `ServerHandler` methods in
`server.rs` — the other references are internal `CallToolResult` / content-block uses the tool
router still accepts. No behaviour change: every mnemo tool call and resource read is synchronous.

- `call_tool` now returns the new `CallToolResponse` enum (`Complete` / `InputRequired` / `Task`).
  The tool router already yields it, so the result passes through unchanged and resolves to
  `Complete` for mnemo's synchronous tools.
- `read_resource` now returns `ReadResourceResponse`; the read is wrapped in
  `ReadResourceResponse::Complete(..)`.
- `ListToolsResult` gained caching fields (`cache_scope` / `result_type` / `ttl_ms`); they are left
  at `Default` so tool listing keeps its pre-3.0 uncached semantics.
- Docs' `rmcp 2.2` version claims updated to `rmcp 3.0` (fenced by
  `docs_rmcp_version_matches_workspace`). Full workspace + all targets build; the `mnemo-mcp` test
  suite (32 tests), clippy `-D warnings`, and `cargo fmt` are green. Landing this bumped the
  workspace to **0.5.22** (a real code change, unlike the docs/publish fixes above) — workspace
  version and the internal dep pins moved 0.5.21 -> 0.5.22, and the README `Current release:` line
  with it. (crates.io itself is unchanged: publishing is still blocked on the token, #140.)

## [0.5.21] — 2026-07-31

A **release-reconciliation + honesty** pass: no public API, wire, or storage
change. It corrects the standing claim that the stranded backend crates had
republished (they had not), records the eight published-but-untracked adapter
crates, reconciles the honesty registry, and pins down how the Python SDK
versions. **The crates.io publish is still blocked on an operator action** —
see the first entry.

### Fixed (2026-07-31) — honest correction: the 0.5.19/0.5.20 crates.io republish never landed; root cause is an expired token

The 0.5.20 entry below (folding 0.5.19) states the
`mnemo-postgres` / `mnemo-rest` / `mnemo-grpc` / `mnemo-graph` crates were
**"unblocked (→ 0.5.19)"**. That is not what happened on crates.io, and this
repo's value is that its docs are true — so, plainly:

- **As of 0.5.21, nothing has published since v0.5.16 (2026-07-24).** The core
  five are still `0.5.16`, the four backends still `0.4.4`/`0.4.5`. `cargo add
  mnemo-postgres` still resolves 0.4.4.
- **The workflow-code fixes were real but not sufficient.** #127 correctly fixed
  `cargo-publish.yml` (golem exclusion, topological order, missing deps) and
  extended `release-crate.yml` to the full library line, and three follow-ups
  hardened the tag path further (below). None of that could publish, because —
- **the proximate blocker is an expired `CARGO_REGISTRY_TOKEN`.** The v0.5.20 tag
  ran the release workflow four times; the run that reached the real upload got
  `403 Forbidden: authentication failed` on all five retry attempts. The repo
  secret was last set 2026-04-25; crates.io tokens of that era carry a ~90-day
  expiry (≈ 2026-07-24), which is exactly why **v0.5.16 published on 2026-07-24
  and every tag from v0.5.17 onward failed.** Rotating the crates.io token and
  updating the repo secret is the one remaining step; the tag path then publishes
  the whole closure at 0.5.21.
- **v0.5.19 was never a git tag** (only a `Cargo.toml` bump, folded into 0.5.20),
  so there is no v0.5.19 crates.io release and none is fabricated.

### Changed (2026-07-30) — `release-crate.yml` hardened for the 9-crate closure

Three plumbing bugs surfaced (and were fixed) while diagnosing the above; all are
in the current `v0.5.20` tag and carried forward here:

- **`protoc` installed** in both gate and publish jobs — `mnemo-grpc` (added to the
  closure in #127) has a `prost` build script that shells out to it
  (`4fa0b0e`).
- **Publish-job timeout 45 → 180 min** — the closure grew from five crates to nine,
  each verify-built twice (coordinated dry-run + real publish) with 60s
  index-propagation waits; 45 min timed out mid-dry-run (`0188535`).
- **Transient-5xx retry** — a crates.io `503 Application Error` killed one upload
  mid-run; each `cargo publish` now retries with backoff and re-checks presence so
  an upload that landed despite a failed HTTP response is not re-uploaded
  (`c14a933`).

### Docs (2026-07-31) — eight published-but-untracked crates recorded; planned registry reconciled

- `docs/roadmap/planned-crates.md` gains a **"Published-but-not-version-tracked"**
  section recording `mnemo-admin` / `-baseline` / `-cma` / `-codemode` / `-deal` /
  `-md-sync` / `-mesh` / `-letta` (all on crates.io at **0.4.4**, none in the
  publish closure). **Decision: keep them out of the closure** — they are advanced
  integration adapters referenced only by repo path in the README feature table,
  with no documented `cargo add` path and no in-workspace consumer (seven depended
  on by nobody; `mnemo-admin` a dep of the unpublishable `mnemo-cli` only). Not
  yanked. `mnemo-amp` / `mnemo-golem-host` / `mnemo-golem-wit` noted as unpublished
  by design.
- The seven **Planned — not built** entries were re-verified against `ls crates/`
  (none exists in the tree; list unchanged) and the reconciliation stamp moved
  `2026-07-03 (v0.5.5)` → `2026-07-31 (v0.5.21)`.

### Docs (2026-07-31) — Python SDK versions independently; GitHub Release reconciliation

- **Python SDK (`mnemo-db` on PyPI) versions independently of the Rust workspace**
  — by design: `.github/workflows/pypi-publish.yml` reads `python/pyproject.toml`'s
  version (not the workspace `Cargo.toml`) and publishes via OIDC trusted-publisher.
  Its current version is **0.5.12**; the Rust workspace is 0.5.21. README now states
  this explicitly so the two version lines are not read as skew.
- A **GitHub Release** was created for the existing `v0.5.20` tag (its body notes
  the crates.io publish is token-blocked), moving `/releases/latest` off the stale
  `v0.5.18`. No `v0.5.19` release exists because no such tag exists.

## [0.5.20] — 2026-07-30

> **0.5.19** was bumped in `Cargo.toml` (2026-07-29, PR #127) but never tagged or
> published (crates.io stayed at mnemo-core 0.5.16 / mnemo-postgres 0.4.4). Per this
> repo's fold-forward convention, its content ships here in **0.5.20** — the first
> tag since the release-workflow repair.

### Security (2026-07-30) — serve-time MCP tool-catalog attestation wired at hardened boot

**`feat`: the last "claimed-but-not-wired" control — MCP tool-catalog attestation
(arXiv 2604.20994) — is now enforced.** Previously `mnemo mcp-server` loaded the
manifest's `[tool_catalog_pin]`, logged "serve-time attestation is NOT enforced in
this build", and served anyway (same class as the role_filter bug fixed 2026-07-28
and the capability lease removed 2026-07-29; `attest/mod.rs` even opened with
`#![allow(dead_code)]`).

- New `MnemoServer::advertised_tool_catalog()` returns the `(name, description,
  input_schema_json)` triples the server will advertise, filtered through any
  `[role_filter]` (attest against what callers see, not the pre-filter superset).
- `run_mcp_server` fingerprints them and attests against the pin **before**
  `serve(stdio())`: it refuses to serve (non-zero exit) on any added/mutated tool or
  `Reject`, and on removed-only drift unless `allow_removed_drift`. Every verdict is
  recorded as an `mcp_tool_catalog_drift` audit event (the module doc's promise).
- New `mnemo mcp-server --print-catalog-pin` emits a ready-to-paste, round-tripping
  pin for the exact binary — the control was previously unusable (no pin generator).
- Scope stated honestly (`attest/mod.rs`, README enforcement table, example
  manifest): a boot-time check is complete for the **static stdio catalog** (defends
  a substituted binary / a hostile tool-injecting dependency / post-upgrade drift);
  it is NOT per-request and only as strong as the manifest file's permissions.
- Tests: new `hardened_mode_attests_tool_catalog` drives the REAL binary over stdio
  (correct pin serves; a mutated schema byte and an extra pin row both refuse to
  serve; the latter accepted with `allow_removed_drift`) — verified red against the
  pre-fix binary. Plus `advertised_tool_catalog` + pin round-trip unit tests.

### Added (2026-07-30) — implicit-association (indirect-query) retrieval probe + orientation-cache arm

- New `bench/locomo` bin `implicit_association`: measures whether mnemo surfaces a
  decisive stored fact for an **indirect** query that shares no wording with it, and
  whether the opt-in constant-token **orientation cache** closes that gap. Three arms
  (`direct` control, `indirect`, `indirect+orientation`), real embedder
  (`nomic-embed-text` 768-dim), Wilson-95, N=5, refuses to score under NoopEmbedding;
  sub-counts A (top-k memories) and B (orientation map) reported separately.
  Representative run: `direct` recall@5 ≈ 1.00, `indirect` ≈ 0.87 (the blind spot),
  orientation-map surfaces the target ≈ 0.93, combined 1.00 — recovering the full
  ≈ +0.13 gap **via the map, not by re-ranking**. Committed 30-row, 12-domain,
  source-cited corpus `bench/locomo/data/implicit_association.jsonl` + structural
  test; write-up `docs/benchmarks/implicit-association.md`. Framing: InMind
  (arXiv:2607.24368) — **NOT** a reproduction and **NOT** comparable to its
  84.0% / 14.4% (that scores an LLM's answers; this scores retrieval surfacing).
- The Ollama embedder was extracted verbatim from `semantic_recall_bench` into
  `bench/locomo/src/ollama.rs` (shared across the real-embedder benches; the
  published 0.739 / 0.805 headline is unchanged).

### Fixed (2026-07-28) — crates.io publish drift: Postgres/REST/gRPC/graph crates unblocked (→ 0.5.19)

> **Correction (0.5.21, 2026-07-31):** the "unblocked" in this heading describes the
> *workflow code*, which was fixed — but the crates **did not actually republish**.
> They are still at 0.4.4/0.4.5 on crates.io. The real blocker was an **expired
> `CARGO_REGISTRY_TOKEN`** (`403 authentication failed`), not the plumbing this
> section fixed. See the 0.5.21 "honest correction" entry above.

**`mnemo-postgres` / `mnemo-rest` / `mnemo-grpc` had sat at 0.4.4 (and `mnemo-graph`
at 0.4.5) for two months** — so `cargo add mnemo-postgres` resolved a version with
none of the v0.5.7 (#99) real pgvector ANN or v0.5.18 async `VectorIndex` work, even
though the README's Postgres story documents both.

- **Root cause:** `cargo-publish.yml`'s `publish` job gated on
  `cargo build --workspace`, which the pre-existing `mnemo-golem-wit` WASM cdylib
  link failure reddens — so push-to-`main` never published. And its crate list was
  in the wrong order (mcp before compliance/attention-state) and missing
  `mnemo-attention-state` (a hard dep of `mnemo-mcp`) and `mnemo-db`.
- **Fix:** exclude the golem crates from that verify build (they are never
  published; `cargo publish -p <crate>` builds only each crate's own closure);
  rewrite the plan list into verified topological order and add the missing deps;
  drop `mnemo-mcp-server` from auto-publish (its path-only dep on the unpublished
  `mnemo-embeddings-bench` blocks `cargo publish` — a separate follow-up). Also
  extended the tag-gated `release-crate.yml` from the compliance line to the full
  library line (adds graph/postgres/rest/grpc in dep order).
- **Close the hole:** `scripts/check_version_drift.sh` now checks **every** published
  workspace member (not just `mnemo-core`) against the workspace version and its
  crates.io max_version, failing CI on drift. The `version-drift` CI job already
  calls it.
- **npm:** the `@mndfreek/mnemo-sdk` SDK (npm 0.4.4) gained a README compatibility
  note stating it versions independently and targets a 0.5.x `mnemo-mcp-server` tool
  surface (no npm publish this pass — no token in the release env).

### Removed (2026-07-28) — capability-lease dead code (never wired)

- Deleted `crates/mnemo-cli/src/lease.rs`, its `LeaseStore` allocation + purge task
  in `main.rs`, and the `Manifest.lease_ttl_seconds` field/validation. The store was
  allocated but **no operation was ever gated on a lease**, while its docstring
  described the defence in the present tense — the same claimed-but-not-wired class
  of bug #124 fixed for `role_filter`. Wiring it would be a breaking MCP change
  (`recall` returning a token, `forget_subject` requiring one), and on stdio a
  single-operator lease is ceremony, not isolation. The design is captured in
  [#126](https://github.com/sattyamjjain/mnemo/issues/126) for a future
  authenticated, multi-caller transport. README enforcement table + `docs/` claims
  corrected to match.

### Docs (2026-07-28) — benchmark headline is the CI-reproducible number; ort repair tracked

- The README headline retrieval number is now the **`nomic-embed-text` (768-dim,
  n=23) recall@1 0.739 / MRR 0.805** result, which needs only Ollama and no build
  feature. The **ONNX `all-MiniLM-L6-v2` (384-dim, n=45) recall@1 0.689** number was
  demoted to a clearly-labelled "not currently CI-reproducible" note — it needs
  `--features onnx`, which CI deliberately excludes (the `ort 2.0.0-rc.11` /
  `ndarray 0.17` integration is broken; tracked in
  [#125](https://github.com/sattyamjjain/mnemo/issues/125)). Each figure is now
  labelled inline with embedder + dimension + n so the two cannot be conflated. The
  `onnx.rs` module doc block was corrected from tokenizers 0.21 / ndarray 0.16 to the
  real pins.

### Docs (2026-07-28) — README CLI Options lists all five commands + drift guard

- Added `bench` (subcommand `embeddings --slo-ms`) and `compliance` (subcommand
  `retention --profile`) to the README `## CLI Options` block (it listed only
  `baseline` / `mcp-server` / `eval` while the binary shipped five). New test
  `readme_cli_commands_documented` parses the built binary's `--help` command tree
  and fails when a top-level command is missing from that block.

### Changed (2026-07-28) — CHANGELOG released versions split out of [Unreleased]

- Split 0.5.5–0.5.18 (which had all accumulated under `[Unreleased]`) into their real
  dated `## [x.y.z]` sections, using git tags/dates as the source of truth; untagged
  intermediate bumps fold forward into the next tagged release. `[Unreleased]` now
  holds only genuinely-unreleased work. Extended the exactly-one-`[Unreleased]` guard
  with `changelog_unreleased_has_no_released_version`, which fails when a
  released/tagged version still has a `### ` sub-section under `[Unreleased]`.

### Chore (2026-07-28) — workspace version 0.5.18 → 0.5.19

- Bumped `[workspace.package].version` and the matching `[workspace.dependencies]`
  internal pins so the drift fix ships as a single coherent release.

### Security (2026-07-28) — manifest [role_filter] is now attached in hardened mode

**`fix`: the manifest `[role_filter]` was built but never passed to the server on the
hardened CLI path — so `mnemo mcp-server --manifest` enforced nothing.**

- The filter was constructed from the `[role_filter]` block in
  `crates/mnemo-cli/src/main.rs` (L696), but the hardened server was built as a bare
  `MnemoServer::new(engine)` (L802) with no `.with_role_filter(...)` — so the binding
  was read once for a logging branch and then discarded. The library dispatch
  (`list_tools` filtering + `call_tool` `-32601`) landed in the previous commit and was
  correct and tested; only the binary dropped the filter, so a manifest `deny` list was
  parsed, logged, and thrown away. Now attached: `server.with_role_filter(filter)`.
- The startup log was a security-posture misreport ("per-tool dispatch enforcement is
  NOT active … tool calls are NOT filtered by role yet") — the kind of line an operator
  reacts to by adding a redundant control or disabling the filter. Replaced with an
  `info!` that reports the enforcing configuration (`default_policy`, `caller_role_count`,
  `allow_entries`, `deny_entries`, `is_noop`), plus a genuine `warn!` for the real
  footgun: a `[role_filter]` block that is a no-op (present but denies nothing).
- **Caller identity (unchanged limitation, stated honestly):** the stdio transport
  carries no per-call caller identity, so dispatch builds a `CallerContext` from the
  engine's default agent id with no roles. A `deny` list therefore acts as a
  **server-wide tool denylist on stdio, not per-caller RBAC**.
- **Test:** new `crates/mnemo-cli/tests/hardened_mode_attaches_role_filter.rs` drives the
  real `mnemo` binary over stdio and asserts a `deny`d tool is absent from `tools/list`
  and rejected by `tools/call` with `-32601`. It **fails against the pre-fix binary**;
  the library-level `role_filter_*.rs` tests did not catch this because the library was
  never broken — only the CLI dropped the filter.

### Docs (2026-07-28) — rmcp version claim fenced; README enforcement table corrected

- **README security-enforcement table:** the **MCP role-filter** row moved from
  ❌ "parsed + validated only" to a conditional ✅ that names both the condition and the
  transport limitation — enforced when a `[role_filter]` block is present and not a
  no-op (a denied tool is hidden from `tools/list` and rejected by `tools/call` with
  `-32601`); on stdio it is a server-wide tool denylist, not per-caller RBAC; with no
  block, every advertised tool stays reachable (unchanged). The matching feature bullet
  was retitled from "parsed + validated; not yet dispatched" to "enforced when
  configured". The tool-catalog attestation, consent-token, and lease rows stay ❌ —
  those are still library-only and untouched.
- **rmcp version fence:** the MCP runtime version was claimed three different wrong ways
  ("rmcp 1.3", `rmcp = "1.3"`, "rmcp 0.14") while the root `Cargo.toml` pins
  `rmcp = "2.2"`. New test `crates/mnemo-cli/tests/docs_rmcp_version_matches_workspace.rs`
  parses the real version from `[workspace.dependencies]` and fails on any `rmcp <ver>`
  claim in `README.md` / `docs/src/**` whose `major.minor` disagrees, reporting each by
  file and line. Fixed the **5 live surfaces** it flagged (README ×2, `architecture.md`,
  `introduction.md`, `integrations/mcp-server.md`) to 2.2 — keeping the "SEPs land in
  `rmcp` first; mnemo upgrades when stable" spec-follower argument intact.

### Build (2026-07-28) — exactly-one-[Unreleased] guard; agent-audit-kit pin 0.3.52 → 0.3.60

- **CHANGELOG hygiene:** the file had **two** `## [Unreleased]` headings — the live one
  and a stale duplicate wedged between `[0.4.0-rc3]` and `[0.4.0-rc1]`. Both the existing
  `changelog_has_unreleased_section` and the ordering guard use `find`/`contains`, which
  only inspect the first heading, so the duplicate silently made the ordering guard
  vacuous. Retitled the stale one to the release its content belongs to
  (`[0.4.0-rc2] - 2026-04-25`, the publication-name-change notes) rather than deleting it,
  and added `changelog_has_exactly_one_unreleased_section` asserting exactly one heading.
- **agent-audit-kit pin 0.3.52 → 0.3.60** (271 rules, latest 2026-07-27) across
  `.github/workflows/security.yml`, `.pre-commit-config.yaml`, and the `.agent-audit-kit.yml`
  header. The suppression baseline was **re-derived** against the new rule set, not copied
  forward: running v0.3.60 on the repo yields 0 critical / 4 high / 31 medium / 1 low, so
  `fail-on: critical` still passes with a single exclusion. Notably `AAK-AGENT-001`
  (critical) now fires **0×** — the rule was tightened upstream and no longer flags
  CLAUDE.md's documented build commands (it fired 60× at v0.3.52); kept as a defensive
  exclude with a corrected rationale. The 9 new rules (e.g. `AAK-AGENT-005` hidden-content
  14× on CLAUDE.md, `AAK-SUPPLY-005`, `AAK-LANGGRAPH-TOOLNODE-LIST-REGRESSION-001`) surface
  only medium/high/low findings and are documented as visible-but-non-blocking; the new
  high `AAK-TRUST-004/006` fire only on the gitignored local `.claude/settings.local.json`,
  which is absent on a clean CI checkout. Supersedes Dependabot PR #118 (which bumps only
  to 0.3.58).
- **README:** dropped the drift-prone "376 tests at v0.4.5" count from the Development
  section (a 0.4.5-stamped number in a 0.5.x repo), keeping the test-surface list.

### Security (2026-07-27) — role-aware MCP tool filter is now wired end-to-end

**`feat`: the `RoleFilter` in `crates/mnemo-mcp/src/role_filter.rs` is no longer
dead code — it is dispatched by the server.**

- New builder `MnemoServer::with_role_filter(Arc<dyn RoleFilter>)` stores an
  optional filter. When set, the server overrides `ServerHandler::list_tools` so
  a caller only sees the tools its role is allowed to call, **and** checks the
  filter at the top of `call_tool` so a denied tool cannot be invoked by name —
  a denied call returns a structured `-32601` (method-not-found) MCP error with
  the deny reason, never a silent empty result. Without a filter, every tool is
  visible and callable (unchanged behavior).
- **Caller identity:** stdio transport carries no per-call caller identity (the
  binary's operator *is* the caller), so the dispatch builds a `CallerContext`
  from the engine's default agent id with no roles. Per-request identity
  plumbing (HTTP/authenticated transports) is a documented follow-up — the
  wiring, filter, and tests are all in place for it.
- **Tests:** `role_filter_hides_and_blocks_denied_tools` asserts a denied tool is
  both hidden from `tools/list` **and** rejected by `tools/call` (the assertion
  that matters), plus the no-filter baseline.

### Docs (2026-07-27) — all 21 registered MCP tools are documented + drift-tested

- `docs/src/tools/README.md` rewritten from "10 tools" to the full **21**
  registered tools, grouped (core memory ops; checkpoint/branch/merge/replay;
  delegation & verification; attention state; agent-controlled `mem_*`; plan
  memory) with each tool's purpose, arguments, and return shape taken from the
  actual tool definitions in `server.rs`.
- New test `docs_document_exactly_the_registered_tools` regenerates the tool set
  from the live `tool_router` and asserts the docs document **exactly** that set
  (no undocumented tool, no phantom tool) — this is what caught the drift.
- Clarified that `mnemo.export_audit_log` is **not** a registered MCP tool: the
  capability exists today as the library API `mnemo_compliance::export_audit_log`
  and is a *planned* lease-gated tool. Fixed stale "10/15 MCP tools" counts in
  `README.md`, `docs/src/introduction.md`, and `docs/src/compliance/README.md`
  (the root README tool table gained the 6 missing rows: `forget_subject`,
  `trajectory_audit`, and the four `mem_*`).

### Build (2026-07-27) — crates.io version-drift guard + SDK version-policy notes

- New CI job **version-drift** (`scripts/check_version_drift.sh`) fails when the
  workspace version is more than one patch ahead of the newest `mnemo-core`
  release on crates.io — the forcing function against tagged-but-unpublished
  releases. **Expected red until 0.5.17 / 0.5.18 are published** (crates.io is at
  0.5.16; publishing is gated on the maintainer's registry token).
- `python/pyproject.toml` gained a comment documenting that the PyPI `mnemo-db`
  package versions **independently** of the Rust workspace (tracks PyPI 0.5.12,
  not workspace 0.5.18).

## [0.5.18] — 2026-07-26

### Fixed (2026-07-26) — Postgres pgvector semantic recall works from the async runtime (v0.5.17 → v0.5.18, #99)

**`fix`: make the `VectorIndex` ANN path truly async so Postgres semantic recall
can no longer panic or deadlock inside the server/CLI `#[tokio::main]` runtime.**

- **Root cause:** `PgVectorIndex::search` / `filtered_search` bridged the async
  `sqlx` pgvector query through `block_in_place` + `Handle::block_on`. That
  requires the multi-threaded runtime and re-enters the current one — on a
  `current_thread` runtime it **panics** ("can call blocking only when running on
  the multi-threaded runtime"), and it risks deadlock. So Postgres semantic /
  hybrid / graph / domain-scoped recall, while no longer silent-empty, could
  **panic at runtime**.
- **Fix (option a — the real one):** `mnemo_core::index::VectorIndex::search` and
  `filtered_search` are now **async** (`#[async_trait]`); the pgvector backend
  `.await`s its query directly on the ambient runtime — no `block_on` bridge, no
  runtime re-entry, works on **any** runtime flavor. The USearch backend does its
  synchronous CPU work inside the async method (no runtime assumed). `recall`
  (`crates/mnemo-core/src/query/recall.rs`) and `conflict` now `.await` the index;
  the `filtered_search` filter is `&(dyn Fn(Uuid) -> bool + Send + Sync)` so the
  future stays `Send`. **Breaking only for external `VectorIndex` implementors**
  (none published).
- **Fail-loud preserved:** a pool-less index / genuinely-absent pgvector extension
  still returns the typed `Error::BackendUnsupported` — never a silent success
  (unit tests retained).
- **Proof:** the `MNEMO_TEST_POSTGRES_URL`-gated integration test
  [`crates/mnemo-postgres/tests/pgvector_ann.rs`](crates/mnemo-postgres/tests/pgvector_ann.rs)
  runs `remember()` + semantic/auto `recall()` against a **live pgvector Postgres**
  and asserts real, non-empty, rank-ordered hits with the permission filter
  respected — under a **multi-threaded** runtime **and** a new
  **`current_thread`** regression test that the old bridge would have panicked on.
  Both pass end-to-end (verified locally against PostgreSQL 17 + pgvector 0.8.5).
- README storage-backend footnote updated: Postgres vector recall now works from
  any Tokio runtime flavor with no `block_on` bridge; #99 resolved.

## [0.5.17] — 2026-07-25

### Added (2026-07-25) — forged-reasoning defense + real-embedder resistance bench (v0.5.16 → v0.5.17)

**`feat`: defend against forged-reasoning memory injection — an attacker plants a
fabricated chain-of-thought so retrieval treats a lie as "already-reasoned
truth" — and prove it with a real-embedder benchmark (ASR OFF/ON + Wilson-95 +
benign false-quarantine).**

- **Defense (shipped, wired):** `retrieval::ReasoningAuthorship` /
  `ReasoningProvenance` (carried in `MemoryRecord.metadata["reasoning_provenance"]`,
  **fail-closed** to `unverified`) + `ReasoningTrustPolicy`, enforced via a new
  **opt-in** `RecallRequest.reasoning_trust` field in recall's shared
  `passes_filters`. Untrusted (injected/unverified) reasoning authorship is
  excluded (`Quarantine`) or demoted (`DownWeight` via `rerank`). Extends the
  existing `RecallRequest`/`retrieval` surface (same pattern as `DomainScope`);
  default `None` keeps the read path unchanged; composes with any strategy.
- **Benchmark [`bench/forged_reasoning`](bench/forged_reasoning)** (bin
  `forged_reasoning`, mirrors the 07-22 real-embedder pattern: ONNX default +
  refuse-to-score-on-noop). Seeds clean model-authored + forged injected-reasoning
  entries into a real engine and measures attack-success-rate with the trust
  filter **OFF vs ON**, plus a benign false-quarantine control.
- **Result (Ollama `nomic-embed-text`, 768-dim, 3 seeds):** forged-reasoning ASR
  **100.0% [95% 96.9, 100.0] → 0.0% [95% 0.0, 3.1]** (n=120, −100.0 pts) at
  **0/180 = 0.0%** benign false-quarantine [95% 0.0, 2.1]. Never a bare ASR —
  Wilson-95 + FPR always shown. Raw JSON (sorted keys, no wall-clock):
  [`bench/results/forged_reasoning.json`](bench/results/forged_reasoning.json);
  threat model + method: [`bench/forged_reasoning/README.md`](bench/forged_reasoning/README.md).
- **Distinct** from the 07-24 ASI06 content-poisoning bench: this targets forged
  *reasoning provenance*, not poisoned *content* retrieval.
- **README** security table gains a "Forged-reasoning defense" row. Version bump
  **0.5.16 → 0.5.17**.

## [0.5.16] — 2026-07-24

### Added (2026-07-24) — ASI06 auditable memory-poisoning-resistance benchmark (v0.5.15 → v0.5.16)

**`bench(security)`: prove the auditable/provenance thesis with a number — the
share of poisoning cover-up/forgery attempts mnemo's cryptographic audit layer
rejects, with a 95% CI and a benign false-positive control.**

- **New crate [`bench/asi06_poisoning`](bench/asi06_poisoning)** (bin
  `asi06_poisoning`). Drives the shipped auditable primitives
  `hash::verify_chain` + `provenance::verify_read_provenance` (never
  re-implements crypto) against three OWASP **ASI06** attack families —
  contradictory-fact silent overwrite, authority-spoofed origin + provenance
  forgery, and belief-drift trail splice/back-date — each with the adversary's
  **cover-up** step.
- **Metric:** auditable **resistance rate** = share of cover-ups the verifier
  **rejects**, per family, with a **Wilson 95%** interval, plus a **benign
  false-positive control** (honest supersession / key rotation / consolidation)
  and a **naive-baseline** contrast (0% — no primitive can detect a cover-up).
- **Result (deterministic, offline; 500 attempts/family, 300 benign):**
  **overall resistance 100% (1500/1500), Wilson 95% [99.7%, 100.0%]**, at
  **0% benign false-positive** (0/300, [0.0%, 1.3%]); naive baseline 0%.
- **Honesty:** this is **tamper-evidence + attribution** (poisoning cannot be
  hidden), **not** write-time prevention — that is the separate quarantine layer
  ([`docs/BENCH_POISONING.md`](docs/BENCH_POISONING.md)). Documented prominently.
- Raw JSON (sorted keys, no wall-clock): [`bench/results/asi06_poisoning.json`](bench/results/asi06_poisoning.json);
  method + attack families + positioning + LoCoMo ground-truth caveat:
  [`docs/benchmarks/asi06-poisoning.md`](docs/benchmarks/asi06-poisoning.md).
  Anchors: OWASP ASI06, arXiv:2606.24322 / 2606.30566, Mem0 93.4% LongMemEval.
- **README** security wedge links the benchmark. Version bump **0.5.15 → 0.5.16**.

### Added (2026-07-22) — memory-poisoning defense benchmark on a real embedder (v0.5.15)

**`bench(security)`: memory-poisoning (MINJA/consolidation) defense benchmark on
a real embedder — ASR + 95% CI + benign-FPR; refuse-to-score-on-noop.**

- **New bin [`poisoning_real_bench`](bench/poisoning/src/bin/poisoning_real_bench.rs)**
  + harness [`real_embedder_bench`](bench/poisoning/src/real_embedder_bench.rs) in
  the existing `mnemo-poisoning-bench` crate. Exercises the **shipped** detector
  (`check_for_anomaly` → `quarantine_memory` on the `remember` write path +
  `recall`'s quarantined-skip, incl. the `PoisoningPolicy` embedding z-score lane)
  through a **real semantic embedder** (default local ONNX MiniLM, no API key;
  `--embedder openai|ollama` also wired). Per attack: detector **ASR** (poison
  survives to recall) + **Wilson 95%**, plus the **benign false-positive rate**,
  over 3 seeds. **Refuses to score under a no-op embedder** (`run_real_bench`
  guards; unit test `refuses_noop_embedder`; CI covers the harness on the offline
  `DeterministicEmbedding`).
- **Attack patterns** (roadmap #37): MINJA canonical + evasive, and
  consolidation redirects (off-distribution-trigger + in-distribution).
- **Honest headline (ONNX `all-MiniLM-L6-v2`, n=90/attack):** the lexical /
  self-referential lane drops **canonical MINJA 100% → 0%** at **0/300 benign
  false-quarantine**; the embedding **z-score lane does not generalise to a dense
  embedder** — poison sits ~1.5σ from benign (below the 3σ gate, 0% flagged), so
  marker-stripped + consolidation redirects survive (ASR 100%). A z-score
  diagnostic (poison vs benign z, gate `baseline_n`) proves the gate is engaged.
  This **corrects** the hash-embedder sibling bench's rosier z-score reading.
- Raw JSON (sorted keys, no wall-clock): [`bench/results/poisoning_real.json`](bench/results/poisoning_real.json);
  methodology + honest limitations: [`docs/BENCH_POISONING.md`](docs/BENCH_POISONING.md).
- **README** security/integrity wedge now points at the real-embedder measurement.
- No version change — rides the same **unreleased 0.5.15** (not yet on crates.io).

### Added (2026-07-21) — first real-embedder LoCoMo retrieval benchmark (v0.5.14 → v0.5.15)

**`bench(locomo)`: mnemo's first retrieval numbers produced by a real semantic
embedder, with a 95% confidence interval and a hard anti-no-op guard.**

- **New bench binary [`locomo_v1_bench`](bench/locomo/src/bin/locomo_v1_bench.rs).**
  Runs the bundled 45-record LongMemEval_M slice through the real recall path
  (in-memory DuckDB + USearch HNSW + Tantivy BM25, RRF fusion) under a **real
  semantic embedder** and reports gold-document **recall@{1,5,10}** with a **Wilson
  95%** interval, **MRR**, **p50/p95** query latency, and **index build time**, per
  strategy (`lexical` / `semantic` / `auto`), averaged over 3 seeds.
  - **Default embedder is local ONNX** (`all-MiniLM-L6-v2`, 384-dim) — reproducible
    by anyone with **no API key**; `--embedder openai` (`OPENAI_API_KEY`) and
    `--embedder ollama` are also wired. The bench is **never** gated behind a paid
    embedder.
  - **Hard guard** [`guard_real_embedder`](bench/locomo/src/real_embedder.rs): the
    runner **refuses to emit any score** if the resolved embedder is not
    semantic-capable (i.e. the zero-vector no-op), naming the embedder it found. A
    silently-noop benchmark is worse than no benchmark. Unit test `refuses_noop_embedder`
    pins it.
- **Headline (ONNX `all-MiniLM-L6-v2`, n=45, mean of 3 seeds — _preliminary_):**
  `semantic` **recall@1 0.689 [0.543, 0.805]**, recall@10 0.911, MRR 0.770; `auto`
  0.615 / 0.889; `lexical` 0.422 / 0.689. Raw deterministic JSON (sorted keys, no
  wall-clock stamp) at [`bench/results/locomo_v1.json`](bench/results/locomo_v1.json);
  full writeup + limitations at [`docs/benchmarks/locomo-v1.md`](docs/benchmarks/locomo-v1.md).
- **`crates/mnemo-core/src/embedding/onnx.rs`:** migrated the ONNX embedder to
  `ort` 2.0.0-rc.11 (session behind `Arc<Mutex<_>>` for `&mut run`, `Tensor::from_array`
  inputs, `try_extract_array`) so the `--features onnx` default path builds and
  produces verified-sane (L2-normed, semantically separated) embeddings.
- **Honest scope:** retrieval quality only (not LLM-judged QA); **no** head-to-head
  vs Mem0/Letta/Zep (not run here); **DuckDB backend only** (Postgres/pgvector
  semantic path not exercised); n=45 → wide, overlapping CIs, labelled `preliminary`.
- **README:** the intro's LoCoMo claim now carries the real-embedder number + a link,
  distinguishing it from the byte-reproducible hash-embedder floor.
- Version bump **0.5.14 → 0.5.15**.

### Security (2026-07-20) — AgentAuditKit MCP static scan in CI + pre-commit (no version bump)

CI / dev-tooling only; **no version bump** (no engine/protocol/crate change).

- **chore(security): dogfood [AgentAuditKit](https://github.com/sattyamjjain/agent-audit-kit)
  (deterministic, offline MCP/agent-config scanner, 262 rules) on the mnemo repo.**
  New `agent-audit-kit` job in [`.github/workflows/security.yml`](.github/workflows/security.yml)
  (pinned `@v0.3.52`) scans mnemo's MCP-server surface, agent configs, and
  supply-chain manifests for secrets / tool-poisoning / auth-bypass /
  path-traversal / supply-chain CVEs — complementing `cargo-audit` + `cargo-deny`
  (Rust crate advisories) on the *MCP / agent* attack surface `mnemo-mcp`
  exposes. Uploads SARIF to the Security tab + posts a PR comment; also wired as
  an opt-in [`.pre-commit-config.yaml`](.pre-commit-config.yaml) hook.
  - **Observe-first gate:** `fail-on: critical` via
    [`.agent-audit-kit.yml`](.agent-audit-kit.yml); highs/mediums are reported
    but non-blocking, to be triaged in the Security tab before tightening to
    `high`.
  - **Baseline established by running it first (v0.3.52):** the one noisy rule
    `AAK-AGENT-001` (60 false-positive criticals on `CLAUDE.md`, which legitimately
    documents build/test commands) is excluded with a written rationale; the
    remaining 51 findings (9 high / 41 medium / 1 low) stay visible — including
    the legit `AAK-GHA-IMMUTABLE-001` (pin Actions to SHAs). The two products
    stay **separate repos**: mnemo (runtime tamper-evident audit) + AgentAuditKit
    (static pre-deploy scan) are complementary, not merged.

### Added (2026-07-20) — STATE-Bench entry harness (number pending model access; no version bump)

Bench harness + docs only; **no version bump** (no engine/protocol/crate change,
no benchmark number yet). This lands the *integration*, not a result.

- **bench(state-bench): mnemo's entry on Microsoft STATE-Bench (Agent Learning
  Track).** New [`bench/state_bench/`](bench/state_bench/) — a **Python-native
  driver** (not a Rust crate: STATE-Bench is Python/`uv` + a `StateBenchAgent`
  subclass, so a Rust crate would reimplement the whole harness). mnemo plugs into
  the read-only `retrieve_learnings(query, top_k) -> list[str]` hook via the
  **public Python SDK** (`MnemoClient.recall`), backed by an embedded DuckDB store
  built from the train trajectories (`build_learnings`). **No `mnemo-core` change.**
  - **Resolved, pinned, cited:** [`microsoft/STATE-Bench`](https://github.com/microsoft/STATE-Bench)
    @ `4efcbf2d4fe60df04878859b692d9391f3d5b33a` (v0.8.1, MIT); baseline
    GPT-5.1-no-memory ~50–60% pass@1 ([leaderboard](https://microsoft.github.io/STATE-Bench/leaderboard/)).
  - **Number is PENDING hosted-model access, not faked.** STATE-Bench is an
    *agentic* enterprise-task benchmark (task completion, not retrieval): it
    hard-locks its user simulator + judge to **GPT-5.4** and needs an agent model
    (gpt-5.1-class). Those are unreachable from the build environment
    (no OpenAI/Azure keys; only a local embedder). Per the honest-benchmark rule we
    publish **no partial or fabricated number** — the harness is built and the
    mnemo half smoke-tested offline; a real run is turnkey via
    [`run_state_bench.sh`](bench/state_bench/run_state_bench.sh) once models exist.
  - **Honest framing:** the score is dominated by the agent model; mnemo is one
    read-only memory hook. So it is an *agent+memory-hook delta* on an agentic
    benchmark — the **on-prem / embedded / auditable** entry nobody has posted, and
    evidence *for* the regulated-AI wedge (the same store carries the hash-chained
    audit log), **not** a retrieval score and **not** a "state of the art" claim.
    README benchmark section gains the entry; the regression gate
    (`check_bench_regression.py`, dataset-scoped `recall@10` for locomo/longmemeval)
    is out of scope by construction and unchanged.

## [0.5.14] — 2026-07-19

### Added (2026-07-19) — v0.5.14, DPDP Rules processing-log retention-conformance profile

Workspace `0.5.13 → 0.5.14` (patch bump — an additive `mnemo-compliance` surface
+ a `StorageBackend` capability method + a CLI command + a bench; no breaking
API change).

- **feat(compliance): processing-log retention-conformance profiles.** New
  [`mnemo_compliance::RetentionProfile`](crates/mnemo-compliance/src/retention.rs)
  expresses a per-obligation retention **floor** (configurable via
  `with_floor_days`) and *verifies* — over before/after `AgentEvent` snapshots —
  that no deletion / compaction / cold-tier path dropped or rewrote a log row
  inside the floor, and that **traffic/processing metadata** (DPDP names "personal
  data, traffic data and logs" separately) was retained. Defaults:
  **DPDP Rules 2025 → 365 days**, **EU AI Act Art.19/26(6) → 180 days**,
  **HIPAA §164.312(b)/§164.316(b)(2) → six years**. Matches the pure-function-over-
  `&[AgentEvent]` shape of the existing Art.12 `export_audit_log` surface.
  - **Fail loud, never silent.** `RetentionProfile::assert_backend_can_retain`
    returns the new typed
    [`ComplianceError::RetentionFloorUnsupported { backend, floor_days }`](crates/mnemo-compliance/src/error.rs)
    (naming the backend) when the active backend cannot guarantee an append-only
    log — the same posture as `mnemo_core::error::Error::EmbedderNotConfigured`
    (v0.5.13). Backed by a new default `StorageBackend::events_are_append_only()`
    capability (`true` for DuckDB — no `DELETE`/`UPDATE` on `agent_events` — and
    PostgreSQL — plus a `prevent_event_modification` trigger).
  - **The enumeration is real.** Every mnemo-core path that could plausibly drop
    an event — `forget` (SoftDelete/HardDelete/Redact/Archive incl. cold-tier),
    `run_ttl_sweep`, `run_decay_pass`, `run_consolidation` — edits *memory content*
    and **appends** an audit event; none removes one. The `agent_events` log is
    append-only by construction; this is the DPDP *personal data* (erasable) vs
    *traffic data and logs* (retained) split.
  - **CLI.** `mnemo compliance retention --profile <dpdp|eu-ai-act-art19|hipaa>
    [--floor-days N]` prints the profile and gates it against the active backend's
    append-only guarantee (fails loud on a backend that cannot honour the floor).
  - **Bench.** New `publish = false`
    [`bench/retention_conformance`](bench/retention_conformance) drives every
    deletion path end-to-end and emits a byte-stable machine-readable artifact
    (profile, floor, one row per path, pass/fail) —
    [`results/retention_conformance.md`](bench/retention_conformance/results/retention_conformance.md)
    (+ `.json`). Sibling of `bench/audit_conformance` (tamper-evidence). Contract
    pinned by `bench/retention_conformance/tests/conformance.rs`.
  - **Docs.** README gains a **Compliance profiles** table (profile → obligation →
    floor → commencement → primary-source URL), using conformance-check language
    only — no certification or compliance claim. DPDP commencement is **2027-05-13**
    (Gazette G.S.R. 846(E), 2025-11-13; 18-month transition); EU AI Act high-risk
    dates are **2027-12-02** (stand-alone Annex III) / **2028-08-02** (Annex I
    embedded) per the Digital Omnibus (Council final green light 2026-06-29).

### Fixed (2026-07-18) — v0.5.13, semantic recall fails loud instead of silent-empty

Workspace `0.5.12 → 0.5.13` (patch bump — a **correctness/safety** fix to the
recall path; no dependency change).

- **fix(recall): semantic recall hard-errors when no real embedder is configured,
  instead of silently returning empty.** With the no-op embedder every query
  embeds to an all-zero vector, so `strategy` ∈ {`semantic`, `hybrid`, `auto`,
  `graph`, `domain_scoped`} (and the typed `RetrievalMode` equivalents) would feed
  a degenerate vector to the index and silently return an empty or meaningless
  result set. These paths now return a typed
  [`Error::EmbedderNotConfigured { requested, backend }`](crates/mnemo-core/src/error.rs)
  — *"semantic recall requires a configured embedder (OpenAI HTTP or local ONNX);
  the noop embedder returns no vectors — refusing to silently return empty."*
  - **The non-semantic path is untouched.** `strategy="lexical"` (BM25) and
    `strategy="exact"` (filter) need no embedder and keep working; `remember` /
    CRUD / ACL / audit are unaffected.
  - **Mechanism.** A new default trait method `EmbeddingProvider::is_semantic_capable()`
    (`true` for OpenAI/ONNX/any real provider; overridden to `false` on
    `NoopEmbedding`) drives a guard in
    [`recall::execute`](crates/mnemo-core/src/query/recall.rs) that fires before the
    query is embedded. `StorageBackend::backend_name()` (`"duckdb"` / `"postgres"`)
    names the backend in the error. This complements the existing Postgres
    `Error::BackendUnsupported` fail-loud (an *unwired index*); this fix covers an
    *absent embedder* on either backend.
  - **New public `DeterministicEmbedding`** — a deterministic, offline
    bag-of-words hashing embedder (real, non-zero vectors; `is_semantic_capable()`)
    for tests, examples, and demos that need the vector path without an API key or
    model. **Not** a production-quality semantic model.
  - **Docs.** README gains a **supported-embedder matrix** (which embedders
    actually produce semantic results) naming DuckDB (or PostgreSQL) **+ a real
    embedder** (OpenAI or on-prem ONNX) as the supported semantic path, and the
    no-op default as a hard-error.
  - **Tests.** New [`crates/mnemo-core/tests/semantic_recall_hard_error.rs`](crates/mnemo-core/tests/semantic_recall_hard_error.rs)
    proves (a) semantic recall under the no-op embedder → typed error, (b) lexical
    recall under the no-op embedder → still returns results, (c) semantic recall
    with a real embedder → returns results. Existing Noop-based suites that
    exercised recall were migrated to `DeterministicEmbedding` (or, for the
    evidence-scorer retrieval-fallback suites, to `strategy="lexical"`); the
    conflict-detection tests that intentionally use degenerate identical vectors
    keep a no-op engine.

### Added (2026-07-16) — Art.12 audit-log tamper-evidence benchmark + `mnemo-db` defensive crate

- **feat(compliance): adversarial audit-log tamper-evidence benchmark.** New
  `publish = false` bench [`bench/audit_tamper`](bench/audit_tamper) builds a
  **real** `agent_events` hash chain through the shipped `remember()` path,
  exports it, and applies four post-hoc attacks — **delete** (mid-chain),
  **reorder** (swap two events), **forge** (integrity field `content_hash`), and
  **truncate** (tail) — scoring each with mnemo's shipped `verify_event_chain`
  (the verifier `verify_event_integrity` runs). Reports a **detection rate** with
  a **Wilson 95%** interval per attack, plus a **benign control**. Result
  (deterministic, offline, byte-stable): delete / reorder / forge-`content_hash`
  each **200/200 (100.0%)** [Wilson 95% 98.1%–100.0%]; **0/72** benign
  false-positives; and **honest 0/200** on payload-only forge + tail truncation —
  two disclosed gaps whose shipped mitigations (memory-record content is
  hash-bound; Postgres `prevent_event_modification` trigger; signed checkpoints)
  are named, not oversold. Contract pinned by `bench/audit_tamper/tests/tamper.rs`.
  - Repro: `cargo run --release -p mnemo-audit-tamper-bench` →
    [`bench/audit_tamper/results/audit_tamper.md`](bench/audit_tamper/results/audit_tamper.md).
  - Narrative:
    [`docs/benchmarks/audit-log-tamper-evidence.md`](docs/benchmarks/audit-log-tamper-evidence.md)
    (cites EU AI Act Art.12 record-keeping, Art.19(1)/Art.26(6) ≥6-month
    retention, and the Art.99(4) **€15M / 3%-of-turnover** penalty tier).
  - Wired into [`docs/POSITIONING.md`](docs/POSITIONING.md) as the Art.12
    tamper-evidence proof point (new thesis-table row + repro command + penalty
    citation).
- **chore(trust): reserve the `mnemo-db` crate name on crates.io.** New
  dependency-free, `publish = true` pointer crate
  [`crates/mnemo-db`](crates/mnemo-db) whose docs redirect Rust users to
  `mnemo-core` + `mnemo-mcp` (the unqualified `mnemo` name on crates.io is an
  unrelated project). It is explicitly **distinct from the PyPI `mnemo-db`
  package**, which is the real Python SDK. `mnemo-db` is removed from the
  README-guard `KNOWN_NON_CRATE` allowlist (it is now a real workspace member),
  and [`release-crate.yml`](.github/workflows/release-crate.yml) gains it as a
  leaf in the gate + coordinated dry-run + idempotent publish loop — so on an
  unchanged workspace version the four compliance-line crates 404-gate and only
  `mnemo-db` is newly published. **No version bump** (docs + bench + a new
  never-before-published crate published at the current `0.5.12`).

## [0.5.12] — 2026-07-14

### Docs (2026-07-13) — contributor IP + regulated-AI README wedge

Docs/governance only; **no version bump** (no engine, protocol, crate, or
compliance-module change; workspace stays at `0.5.12`).

- **docs(governance): add DCO+CLA; narrow README positioning to regulated-AI
  memory.**
  - **Contributor IP hygiene.** [`CONTRIBUTING.md`](CONTRIBUTING.md) now requires
    a per-commit **Developer Certificate of Origin** sign-off (`git commit -s`,
    full DCO 1.1 text inline); a new self-contained
    [`.github/workflows/dco.yml`](.github/workflows/dco.yml) enforces it on every
    PR (matching `Signed-off-by` ↔ author, no third-party action); a new
    [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md) carries
    the sign-off checkbox + Summary/Test-Plan sections. New [`CLA.md`](CLA.md)
    adds the standard Apache-style **ICLA + CCLA** for substantial contributions.
    The project stays **Apache-2.0**; `LICENSE` is unchanged; neither the DCO nor
    the CLA transfers copyright.
  - **README wedge.** The top-of-README tagline moves from the generic
    "MCP-native memory database for AI agents" to the survivor position —
    **on-prem, MCP-native, cryptographically-auditable memory for regulated AI
    (EU AI Act Art.12 · India DPDP · HIPAA)** — while keeping the
    REMEMBER/RECALL/FORGET/SHARE primitives and the DuckDB-embedded (+ your
    PostgreSQL) wedge intact, and calling out the hash-chained `agent_events`
    audit log + `mnemo-compliance` as the differentiator. Links the already-
    shipped [`docs/POSITIONING.md`](docs/POSITIONING.md) (unchanged).

### Distribution (2026-07-13) — v0.5.12, crates.io compliance line

Workspace `0.5.11 → 0.5.12` (patch bump — a **distribution-only** change: no
engine, protocol, or bench code change; the bump exists so the compliance line
gets a clean, previously-unpublished crates.io version and a fresh release tag).

- **distribution: 0.5.x compliance line published to crates.io.** The minimal
  importable set to run on-prem, hash-chain-audited memory —
  [`mnemo-core`](https://crates.io/crates/mnemo-core),
  [`mnemo-attention-state`](https://crates.io/crates/mnemo-attention-state),
  [`mnemo-compliance`](https://crates.io/crates/mnemo-compliance), and
  [`mnemo-mcp`](https://crates.io/crates/mnemo-mcp) — now carries full
  `[package]` metadata (description, `license = "Apache-2.0"`, repository,
  homepage, keywords, categories, and a per-crate README) and publishes via a new
  tag-triggered [`release-crate.yml`](.github/workflows/release-crate.yml).
  - `mnemo-attention-state` is in the set because `mnemo-mcp` hard-depends on it
    (introduced after the last 0.4.4 crates.io publish); publish order is
    `core → attention-state → compliance → mcp`.
  - The workflow gates on **only the publish closure** (fmt + clippy + tests for
    those four crates), so the pre-existing golem-wit WASM link failure that
    reddens the workspace-wide build cannot block a release; it version-gates
    each crate (404 check) for idempotent re-runs, and dry-runs before uploading.
  - Internal path deps in the root `Cargo.toml` carry `version` alongside `path`
    (bumped `0.5.0 → 0.5.12` in lockstep) so `cargo publish` accepts them.
  - No crate name collision: `mnemo-core`/`mnemo-mcp`/`mnemo-compliance` are
    already owned by this project on crates.io (last at 0.4.x); no rename needed.
  - README gains an **Install from crates.io** section leading with the offline
    hash-chain verify API, pointing at [`docs/POSITIONING.md`](docs/POSITIONING.md).

## [0.5.11] — 2026-07-08

### Docs (2026-07-08) — compliance-axis positioning one-pager

Docs-only; **no version bump** (no code, bench, or protocol change).

- **docs: POSITIONING.md — compliance-audit-axis comparison vs Mem0/Letta/native
  memory, wired to shipped bench numbers.** New
  [`docs/POSITIONING.md`](docs/POSITIONING.md) converts the already-published
  benchmarks into a single credibility one-pager: a capability table across
  on-prem/self-host, MCP-native primitives, cryptographic hash-chain audit log,
  memory-poisoning defense (shipped delta), and regulatory mapping (EU AI Act
  Art.12 2026-08-02 · India DPDP 2027-05-13 · OWASP ASI06). Every mnemo cell
  cites a reproducible bench already in the repo — LongMemEval semantic recall@1
  0.739, audit-conformance 100%/256-trial tamper detection (Wilson-95), poisoning
  defense delta MINJA +100 / AgentPoison +96.5 pts with 0/200 benign FPR, and the
  byte-stable LoCoMo reproduction — and the page states plainly where mnemo does
  **not** lead (recall/QA quality vs Mem0's funded team). No invented numbers; no
  gating; Apache-2.0. README gains a top-of-file link to it.

### Added (2026-07-07) — v0.5.11, memory-poisoning defense-delta benchmark

Workspace `0.5.10 → 0.5.11` (patch bump — a new bench crate + tests + docs; no
engine/protocol API change). Offline, Apache-2.0, **no managed-cloud dependency
added to core**.

- **bench(security): memory-poisoning defense delta.** New crate
  [`bench/poisoning`](bench/poisoning/) (`cargo run --release -p
  mnemo-poisoning-bench`) measures the **Attack Success Rate (ASR)** of two
  named, published attacks with mnemo's shipped poisoning-quarantine defense
  **OFF vs ON** — the **delta** is the headline. It toggles the *real* defense
  (`check_for_anomaly` → `quarantine_memory` on the `remember` write path +
  recall's `quarantined` skip, plus the opt-in `PoisoningPolicy` z-score gate) —
  **not** provenance HMAC (which is per-read receipts, not a retrieval filter),
  stated honestly in the crate docs.
  - **Attacks:** MINJA-style memory injection
    ([arXiv:2503.03704](https://arxiv.org/abs/2503.03704), indirect-ingest poison
    with self-referential bridging phrasing) and an AgentPoison-style low-rate
    trigger (single novel-token poison among 1001 benign, **0.0998% < 0.1%** of
    the store).
  - **Observed (seed `0x901504202607`, 200 trials/attack, top-5):** MINJA
    canonical ASR 100%→0% (**delta +100 pts**, lexical lane); AgentPoison
    100%→3.5% (**delta +96.5 pts**, z-score gate); **benign control 0/200
    false-quarantine**. Evasive MINJA (markers stripped) stays 100%→100% — a
    disclosed lexical-lane blind spot, not hidden.
  - **Deterministic + byte-stable:** fixed corpus, deterministic hashed embedder,
    exact brute-force vector index (the reference mnemo's approximate HNSW
    tracks), neutralised recency lane; every ASR carries a Wilson 95% interval.
    Tests in [`bench/poisoning/tests/`](bench/poisoning/tests/) gate the delta,
    the 0% benign control, and byte-stability. Observed numbers only, never a
    claimed one; no "best"/"first" claim.
  - Reuses the shared `mnemo_locomo_bench::stats::wilson_95` helper.
- **chore(python): close the PyPI publish gap.** The `mnemo-db` PyPI SDK version
  was stale at `0.4.9` while the workspace moved to `0.5.x` — a stale published
  artifact undercuts the benchmark-credibility story. Bumped
  `python/pyproject.toml` + `python/mnemo/__init__.py` to `0.5.11` (PyO3 wheel
  verified to build against the current core with `maturin build --release`); the
  push-to-`main` `pypi-publish` workflow publishes `mnemo-db 0.5.11`.

### Added (2026-07-06) — v0.5.10, claimed-vs-observed LoCoMo reproduction

Workspace `0.5.9 → 0.5.10` (patch bump — a new bench bin + a shared loader + a
byte-stable test + docs; no engine/protocol API change). Offline, Apache-2.0,
**no managed-cloud dependency added to core**.

- **bench(locomo): claimed-vs-observed reproduction.** New
  [`reproduction_bench.rs`](bench/locomo/src/bin/reproduction_bench.rs)
  (`cargo run --release -p mnemo-locomo-bench --bin reproduction_bench`) re-runs a
  LoCoMo single-hop split under mnemo's disclosed **offline hybrid-recall harness**
  (`strategy="auto"`: BM25 + graph + RRF fusion; fixed seed; Wilson-95) and tables
  mnemo's **observed** number against vendors' **published, cited, not-re-run**
  claims (Mem0 92.5; Zep 84→58.44 corrected; MemPalace 100→60.3 R@10 corrected;
  Supermemory ~99 self-reported PoC) — riding the 2026 memory-benchmark
  reproducibility crisis. Only mnemo's row is reproducible here; the report is
  explicit that the claimed figures are **not a ranking** (retrieval vs
  end-to-end QA, different scale/judge). No "best"/"first" claim.
  - **Reproducible by disclosure.** The report
    ([`bench/locomo/results/reproduction_2026-07-06.md`](bench/locomo/results/reproduction_2026-07-06.md))
    is **byte-stable** — two runs `diff` identically — via two *disclosed*
    methodological choices: an **exact brute-force cosine** vector index (the
    deterministic reference mnemo's approximate USearch HNSW tracks) and a
    **neutralised recency lane** (a batch-seeded corpus has no recency signal, so
    the wall-clock lane is pinned to a constant). A new
    [`bench/locomo/tests/reproduction_byte_stable.rs`](bench/locomo/tests/reproduction_byte_stable.rs)
    gates the byte-stability. Observed (offline hashed embedder, n=45 single-hop):
    recall@1 **24.4%** [Wilson 95% 14.2%, 38.7%], recall@3 37.8%, recall@5 46.7%
    (2/45 queries errored in the BM25 lane on natural-language punctuation and are
    disclosed + counted as misses).
  - **Real-embedder path** gated behind `--ollama-model` (fail-loud, never a
    silent number), matching the sibling benches.
  - **Refactor:** extracted the shared LoCoMo fixture loader into
    `mnemo_locomo_bench::dataset` (`LongMemRecord` + `load_dataset` +
    `default_dataset_path` + `dataset_sha`), reused by `reproduction_bench`.
- **docs:** `bench/RESULTS.md` gains a claimed-vs-observed section; `README.md`
  adds a "reproducible-by-disclosure" line to the regulated-AI block.

### Added (2026-07-05) — v0.5.9, regulated-memory audit-conformance artifact

Workspace `0.5.8 → 0.5.9` (patch bump — a new offline bench crate + compliance
docs + positioning; no engine/protocol API change). Apache-2.0, offline-
verifiable, **no managed-cloud dependency added to core**.


Workspace `0.5.8 → 0.5.9` (patch bump — a new offline bench crate + compliance
docs + positioning; no engine/protocol API change). Apache-2.0, offline-
verifiable, **no managed-cloud dependency added to core**.

- **bench: offline, deterministic audit-conformance proof.** New crate
  [`bench/audit_conformance/`](bench/audit_conformance/)
  (`cargo run --release -p mnemo-audit-conformance-bench`) proves — with no
  network and no LLM — that mnemo's memory-write log is tamper-evident and
  externally verifiable **without trusting the store**. It is a driver+reporter
  built **entirely on shipped `mnemo-core` primitives** (`hash::verify_chain`,
  `hash::verify_event_chain`, `MnemoEngine::verify_integrity`,
  `verify_event_integrity`) — it never re-implements cryptography.
  - **Six properties, all `PASS`:** the write chain verifies through the real
    `remember()` path; the append-only `agent_events` log verifies; a single-byte
    content mutation is caught **100% over 256 trials (Wilson 95% ≥ 98.5%)** and
    the first broken record is named; `forget` **appends** a signed
    `MemoryDelete` event and **retains** the original write row (append-only
    retention, not erasure); plus a fixed, **recomputable SHA-256 crypto vector**
    anyone can reproduce offline (`printf … | shasum -a 256`).
  - **Byte-stable report.** No timestamps or run-varying hashes in the body — two
    runs `diff` identically; the run prints the report's own SHA-256. Report at
    [`bench/audit_conformance/results/conformance.md`](bench/audit_conformance/results/conformance.md).
  - Reuses the shared `mnemo_locomo_bench::stats::wilson_95` helper (no per-bin
    copy). Registered in `[workspace] members`.
- **docs(compliance): regulatory mappings (honest, hedged, not legal advice).**
  [`docs/compliance/eu-ai-act-art12.md`](docs/compliance/eu-ai-act-art12.md)
  maps the append-only log + retention to EU AI Act Art.12 record-keeping and
  Art.26(6) six-month deployer log retention, with the hedge that the May-2026
  Digital Omnibus proposal may move high-risk dates toward Dec-2027.
  [`docs/compliance/dpdp-2027.md`](docs/compliance/dpdp-2027.md) maps to the
  India DPDP Rules 2025 obligations (full-compliance working date 2027-05-13),
  and states the DPDPA-erasure vs AI-Act-retention tension explicitly (mnemo
  ships both `HardDelete` and `Redact` and logs which was used).
- **docs(results): auditability comparison.** `bench/RESULTS.md` gains an
  auditability table (mnemo offline hash-chain verify vs Mem0 vs Zep
  cloud/managed audit), sourced from each vendor's docs +
  developersdigest.tech, with a dated hedge and no "best" claim.
- **docs(README): regulated-AI positioning block** — "on-prem, MCP-native,
  cryptographically-auditable memory for regulated AI (EU AI Act / DPDPA /
  HIPAA)", linking the bench and the two compliance docs.

### Added (2026-07-04) — v0.5.8, reproducible BEAM-style multi-hop/open-domain retrieval bench

Workspace `0.5.7 → 0.5.8` (patch bump — a new bench bin + a shared stats helper
+ docs; no engine/protocol API change).

- **bench(locomo): reproducible BEAM-style multi-hop/open-domain number over
  hybrid recall.** New bin
  [`bench/locomo/src/bin/beam_bench.rs`](bench/locomo/src/bin/beam_bench.rs)
  (`cargo run --release -p mnemo-locomo-bench --bin beam_bench`) runs two
  BEAM-style subtasks — multi-hop (answer reachable only via a `related_to`
  graph edge) and open-domain (gold among same-schema distractors) — over
  mnemo's default hybrid `auto`/RRF recall (semantic + BM25 + graph + recency),
  reporting per-subtask accuracy with a **Wilson 95%** interval.
  - **Deterministic + offline by default:** a hashed bag-of-tokens embedder (no
    network, no LLM), fixed seed `0xbea320262026`, 100 queries × 5 pooled
    repeats/subtask (repeats pooled to average the USearch HNSW approximate-NN
    noise floor into the CI — the same treatment `semantic_recall_bench`
    documents). A real-embedder path is gated behind `--ollama-model` and fails
    loud if Ollama is unreachable — never a silent/unreproducible number.
  - **Result (this run):** `multi_hop` **0.6%** (3/500, [0.2%, 1.7%]),
    `open_domain` **68.6%** (343/500, [64.4%, 72.5%]). The low multi-hop figure
    is reported as-is — default `auto` RRF barely surfaces a graph-only answer
    against lexically-equivalent distractors; `graph`/`reconstruct` are the
    multi-hop tools.
  - **Honest framing (no over-claim):** `bench/RESULTS.md` + README add a BEAM
    row with an explicit **reproduced (this fixture) vs. self-reported
    (upstream)** column and a note that self-reported memory scores (e.g.
    Hindsight BEAM 64.1% @ 10M tokens) are a vendor-run **upper bound**, not
    independently reproduced, and **not comparable** to a small synthetic
    fixture. No "first"/"best" claim.
  - **Refactor:** extracted the shared `wilson_95` CI helper into
    `mnemo_locomo_bench::stats` (reused by `beam_bench` and `asi06_resistance`
    instead of a per-bin copy). Also corrected `bench/RESULTS.md`'s stale
    backend note (pgvector semantic recall is implemented as of v0.5.7, #99).

### Fixed (2026-07-04) — v0.5.7, real pgvector ANN search on the Postgres backend ([#99])

Workspace `0.5.6 → 0.5.7` (patch bump — implements a previously-stubbed backend
capability; no public API change to the `VectorIndex` trait).

- **fix(postgres): implement pgvector ANN search — semantic/hybrid/graph recall
  now returns results on the Postgres backend ([#99]).** `PgVectorIndex::search`
  / `filtered_search` previously returned a typed `BackendUnsupported` error
  (they had been `Ok(vec![])` before 2026-06-23). They now run a real
  cosine-distance ANN query (`embedding <=> $1 … ORDER BY … LIMIT k`) against the
  `idx_memories_embedding_hnsw` HNSW index (`vector_cosine_ops`), returning the
  stored memory ids + distances in the same `(id, distance)` shape as USearch —
  so recall's `score = 1.0 - distance` conversion is identical across backends.
  - **Permission-safe.** `filtered_search` mirrors the USearch backend's
    iterative oversample-then-filter (3× → double until `limit` accessible hits
    or the table is exhausted), so scoped/filtered recall never under-returns.
  - **Wiring.** `PgVectorIndex::with_pool(pool, dims)` shares `PgStorage`'s
    `sqlx::PgPool` (new `PgStorage::pool()` / `dimensions()` accessors); the CLI
    now constructs the index with the pool. The synchronous `VectorIndex` trait
    is bridged to async `sqlx` via `block_in_place` + `Handle::block_on`, which
    requires the multi-threaded Tokio runtime (the CLI/server is `#[tokio::main]`).
  - **Still fails loud.** A pool-less index, or a genuinely-absent pgvector
    extension / `<=>` operator at runtime, returns the typed
    `Error::BackendUnsupported` — never a silent empty result.
  - **Test.** New `MNEMO_TEST_POSTGRES_URL`-gated integration test
    `crates/mnemo-postgres/tests/pgvector_ann.rs` (skips cleanly when unset):
    inserts 3 known-embedding memories, asserts `semantic` + `auto` return the
    nearest in rank order, and that a nearer *private* record owned by another
    agent is excluded (permission filter + oversample). README backend
    capability matrix updated: Postgres vector recall flips from ❌ to ✅.

[#99]: https://github.com/sattyamjjain/mnemo/issues/99

### Added (2026-07-02) — v0.5.6, first memory-poisoning resistance micro-bench + OWASP ASI06 mapping

Workspace `0.5.5 → 0.5.6` (patch bump — a new bench bin + a security doc + one
README row; no new detector, no engine/protocol API change).

- **bench(security): publish mnemo's first memory-poisoning *resistance* number
  (OWASP ASI06).** New bin
  [`bench/locomo/src/bin/asi06_resistance.rs`](bench/locomo/src/bin/asi06_resistance.rs)
  (`cargo run --release -p mnemo-locomo-bench --bin asi06_resistance`) quantifies
  how well the **existing** poisoning defense (`check_for_anomaly` → `quarantine`
  → recall skips quarantined) resists a query-only MINJA-style attack
  ([arXiv:2503.03704](https://arxiv.org/abs/2503.03704)). This adds **no**
  detector — it measures the one already shipped. DEFENDED vs UNDEFENDED isolates
  exactly one variable (the `quarantined` flag on a byte-identical record).
  - **Result (200 deterministic trials/class, top-5, seed `0xa510062026`):**
    canonical MINJA (bridging markers) → **100.0% resistance, Wilson 95%
    [98.1%, 100.0%]**, 200/200 quarantined; the same poison is recalled 100% of
    the time in an undefended store.
  - **Honest limitation, published alongside:** a marker-free *evasive*
    paraphrase → **0.0%** resistance [0.0%, 1.9%] — the always-on lexical layer
    keys on bridging phrasing; the opt-in embedding z-score baseline gate
    (`PoisoningPolicy::with_outlier_threshold`) is the intended defense there and
    is **not** exercised in this single-embedder run.
- **docs: [`docs/security/ASI06.md`](docs/security/ASI06.md)** maps mnemo's
  REMEMBER anomaly scan / quarantine / RECALL quarantine-filter / hash-chain to
  OWASP **ASI06 (Memory & Context Poisoning)**, states the number + full
  methodology + limitations (query-only MINJA variant, not a full adversarial
  suite, single embedder). README enforcement-table poisoning row updated to
  cite the published number.

### Fixed (2026-07-03) — v0.5.5, workspace-member drift ([#74])

Workspace `0.5.4 → 0.5.5` (patch bump — docs + a CI fence + version stamps; no
dependency, engine, or protocol API change).

- **chore(workspace,docs): close the phantom-crate drift ([#74]).** Seven
  `mnemo-*` crate names asserted by the daily-prompt ledger
  (`mnemo-envelope`, `mnemo-aas01`, `mnemo-mgt`, `mnemo-bench-cf`,
  `mnemo-langgraph`, `mnemo-purview`, `mnemo-toolhive`) have **no source tree
  and are not `[workspace] members`**. None were stubbed — each is an
  external-system adapter with no consumer, and an empty shell is exactly the
  drift the repo already *retired* `mnemo-langgraph` for. Instead every
  reference is now truthful:
  - **New single source of truth**
    [`docs/roadmap/planned-crates.md`](docs/roadmap/planned-crates.md) — all
    seven listed as **Planned / not built** (or **Retired**, for the
    `mnemo-langgraph` Rust shell superseded by the Python `MnemoCheckpointer`).
  - **Residual shipment-assertions corrected.** `docs/src/integrations/mcp-server.md`
    no longer says the `mnemo-envelope` exporter "lands in v0.4.3";
    `docs/comparisons/cloudflare-agent-memory.md` no longer says bench numbers
    "ship in v0.4.3 as the `mnemo-bench-cf` crate" — both now say **not built**
    and link the roadmap. The already-honest "Parked"/"Retired"/"has not been
    built" notes elsewhere are unchanged.
  - **CI fence against recurrence.** New
    `crates/mnemo-cli/tests/readme_crate_claims_are_real.rs` fails the build if
    a `mnemo-*` name in `README.md` is neither a real workspace member (matched
    live against member dir basenames + declared package names) nor on an
    explicit allowlist of non-crate references (PyPI/npm dist names, JSON
    filenames, labelled sketches, prose hypotheticals). Mirrors the AAK
    rule-count fence + the existing `readme_no_marketing_phrases` lint.
- Version stamps bumped to `0.5.5`: `Cargo.toml`, `version_metadata` test,
  `docs/compat/version-skew-matrix.md`.

[#74]: https://github.com/sattyamjjain/mnemo/issues/74

### Landing trace (2026-07-07)

This `[Unreleased]` accumulator sits on `main` at
[`d764de6`](https://github.com/sattyamjjain/mnemo/commit/d764de6) (the v0.5.10
claimed-vs-observed LoCoMo cut). It now also carries the **v0.5.11**
memory-poisoning defense-delta benchmark above, landing via branch
`feat/poisoning-defense-bench` (push-to-`main`, tagged `0.5.11`; the workspace
version bump `0.5.10 → 0.5.11` triggers the crates.io publish of changed crates
— the bench crate is `publish = false`). It also carries the **2026-07-08
compliance-axis positioning one-pager** (`docs/POSITIONING.md`) above — a
docs-only change landing via branch `docs/positioning-compliance-axis`
(push-to-`main`, **no version bump**, no crate republish). And it carries the
**v0.5.12** crates.io compliance-line distribution change above — landing via
branch `chore/crates-io-0.5.x`, workspace bump `0.5.11 → 0.5.12`, tagged
`v0.5.12` to fire the new tag-triggered `release-crate.yml` (publishing
`mnemo-core` → `mnemo-attention-state` → `mnemo-compliance` → `mnemo-mcp`). The
prior `v0.5.11` tag already points at the poisoning cut (`3d21e63`) and predates
`release-crate.yml`, so a fresh `v0.5.12` tag is what carries the workflow.
It also carries the **2026-07-13 contributor-IP + regulated-AI README wedge**
governance change above (DCO + CLA + PR template + README tagline) — a
docs/governance-only change landing via branch `docs/cla-and-positioning`
(push-to-`main`, **no version bump**, no crate republish). It also carries the
**2026-07-16 Art.12 audit-log tamper-evidence benchmark + `mnemo-db` defensive
crate** change above — landing via branch
`feat/audit-log-tamper-evidence-bench` (push-to-`main`, **no version bump**); the
new `mnemo-db` pointer crate is published at the current `0.5.12` via
`release-crate.yml`'s idempotent loop (the four compliance-line crates 404-gate
as already-present). It also carries the **2026-07-18 semantic-recall
fail-loud correctness fix** above — landing via branch
`fix/semantic-recall-hard-error` (push-to-`main`, workspace bump
`0.5.12 → 0.5.13`); this is an engine (`mnemo-core`) change, so a `v0.5.13` tag
republishes the compliance line via `release-crate.yml`. Finally it carries the
**2026-07-19 DPDP retention-conformance profile** above — landing via branch
`feat/dpdp-retention-conformance` (push-to-`main`, workspace bump
`0.5.13 → 0.5.14`); a `v0.5.14` tag republishes the compliance line
(`mnemo-core` + `mnemo-compliance` changed). Finally it carries the
**2026-07-20 STATE-Bench entry harness** above — landing via branch
`bench/state-bench` (push-to-`main`, **no version bump**, no crate change; a
Python-native bench driver whose *number* is pending hosted-model access). Earlier
cuts `v0.5.4` (`04a1145`) through `v0.5.10` remain documented in the sections
below.

## [0.5.4] — 2026-06-29

First GitHub Release cut since `v0.4.15`. The `v0.5.0 → v0.5.4` tags were pushed
(and auto-published to crates.io) but never got a GitHub Release; this section
consolidates that 0.5.x work under the current `v0.5.4` version, plus the
benchmark + release-drift work below. No new version bump — the workspace was
already at `0.5.4`; this cuts the release rather than bumping past it.

### Added (2026-06-29) — first authenticated benchmark baseline + two-axis parity table

- **bench: publish the first authenticated retrieval baseline (real local embedder,
  not `NoopEmbedding`).** Ran `mnemo-locomo-bench :: semantic_recall_bench` against a
  real `nomic-embed-text` (768-dim, via Ollama) over the bundled LongMemEval_M slice
  and recorded the scored result — recall@k / MRR per mode, embedder config, swept
  hybrid weights, commit SHA, and date — into
  [`docs/benchmarks/baseline.json`](docs/benchmarks/baseline.json) so the nightly
  regression gate has real numbers to compare against. Headline: `vector_only`
  **recall@1 = 0.739 (MRR 0.805)**, measured 2026-06-29 @ `640b7b1`. Reproduce:
  `ollama pull nomic-embed-text && cargo run --release -p mnemo-locomo-bench --bin semantic_recall_bench`.
- **docs: two-axis parity table in the README Benchmarks section.** Places mnemo's
  **measured retrieval** row (recall@1, the axis we actually ran) next to the
  **reported end-to-end QA-accuracy** numbers for Mem0 (93.4% LongMemEval) and Letta
  (~83% LoCoMo), with a bold caveat that these are **not directly comparable** —
  different metrics, different datasets — and that mnemo has **not** run the
  end-to-end QA-accuracy pipeline (no generative LLM in this harness). No win is
  claimed that was not measured; only the real retrieval row is published.
- **docs(drift, [#74]): removed the phantom `mnemo-bench-cf` references.** The
  Cloudflare-vs-mnemo bench crate was scoped but never built; the README now says so
  explicitly (not a workspace member, numbers not run) instead of implying it ships.

[#74]: https://github.com/sattyamjjain/mnemo/issues/74

### Security (2026-06-27) — v0.5.4 cut, bearer-token auth + truth-in-advertising

Workspace `0.5.3 → 0.5.4` (patch bump — adds a network auth floor, no breaking API change).

- **security: bearer-token auth on REST/gRPC; align README security claims with
  wired behavior; typed errors for unsupported features.**
  - **Network auth floor.** REST (`mnemo-rest`) and gRPC (`mnemo-grpc`) now read
    `MNEMO_AUTH_TOKEN`; when set, every REST request (except `GET /v1/health` +
    CORS preflight) must send `Authorization: Bearer <token>` → `401`, and every
    gRPC RPC must send matching `authorization` metadata → `UNAUTHENTICATED`.
    Constant-time compare via the new `mnemo_core::auth::bearer_token_matches`.
    When unset, both servers run open **and log a warning on startup** — never
    silently unauthenticated. New `router_with_auth(engine, Option<String>)` on
    both crates; `router()` reads the env var.
  - **Truth-in-advertising.** Audited every README security claim against the
    live code path. Corrected three over-claims to match reality and added a
    "Security: what is and isn't enforced today" table:
    - **MCP role-filter** — manifest block is *parsed + validated*, but **not
      invoked at tool dispatch**; the README no longer implies tool calls are
      filtered, and the CLI now logs a **warning** (was an info "loaded" line).
    - **MCP tool-catalog attestation** — pin is *parsed + validated*, but
      serve-time attestation is **not enforced**; CLI logs a warning.
    - **DPDPA consent-token-per-write** — `ConsentTokenGuard` is a **library**;
      the core `engine.remember` performs **no** consent check. README no longer
      claims it "refuses every remember."
  - **Typed errors for unsupported features** carry over the structured
    `Error::BackendUnsupported` variant (v0.5.3) — no security feature silently
    no-ops; loaded-but-unenforced features warn loudly.
  - Tests: `mnemo_core::auth` unit tests + REST auth integration tests
    (reject-missing / reject-wrong / accept-correct / health-exempt / open-mode).

### Fixed (2026-06-23) — v0.5.3 cut, typed `BackendUnsupported` for Postgres semantic recall

Workspace `0.5.2 → 0.5.3` (patch bump — error-type hardening + docs, no API break).

- **fix: Postgres `semantic_recall` now returns a typed `BackendUnsupported`
  error instead of silently returning empty; document DuckDB as the supported
  semantic backend.** The pgvector ANN path (`semantic` / `auto` / `graph` /
  `domain_scoped` / `reconstruct`) already failed loud, but with a generic
  `Error::Index(String)`. It now returns the structured
  `Error::BackendUnsupported { backend: "postgres", capability:
  "semantic_recall", detail }` so callers can match on `backend` / `capability`
  programmatically instead of string-sniffing the message; `detail` keeps the
  actionable guidance + tracking link ([#99]).
  - **New typed variant** `Error::BackendUnsupported` in
    `crates/mnemo-core/src/error.rs` (additive; the gRPC/REST error mappers
    fall through their existing wildcard arms → 500/internal).
  - **README backend capability matrix**: an explicit per-capability does/does-NOT
    table (DuckDB ✅ vs Postgres ❌-on-vector); crate-level doc note on
    `mnemo-postgres`.
  - **Test** `ann_search_fails_loud_not_silent_empty` upgraded to assert the
    structured variant (`backend`/`capability`), not just `is_err()`.

[#99]: https://github.com/sattyamjjain/mnemo/issues/99

### Added (2026-06-22) — v0.5.2 cut, real-embedder memory-quality result + Postgres semantic stub hard-errors

Workspace `0.5.1 → 0.5.2` (patch bump — bench + docs + a credibility-bug confirmation, no API change).

- **feat(bench): published real-embedder memory-quality result ([`bench/RESULTS.md`](bench/RESULTS.md)).**
  One honest, reproducible number from `semantic_recall_bench` run with a
  **real** local semantic embedder (`nomic-embed-text`, 768-dim, via Ollama —
  never `NoopEmbedding`) over the bundled LongMemEval_M slice: held-out
  semantic **recall@1 = 0.739 (MRR 0.805)**, with the default `auto` RRF
  fusion reported as-is (0.435 recall@1 — not cherry-picked).
  - **Engram-style token efficiency** (arXiv:2606.09900, lean-slice-vs-full-history,
    cited as a reference point not a parity claim): a lean top-5 retrieved
    slice costs **~89% fewer context tokens** than the full history. Added as
    a deterministic, no-LLM section + JSON field to the bench
    (`bench/locomo/src/bin/semantic_recall_bench.rs`).
  - **Honest caveats baked in:** single-run (5 in-process seeds, not
    restart-averaged); HNSW + RRF-weight selection sit near a noise floor (FID
    Lottery) so the swept "best" hybrid config flips run-to-run — `vector_only`
    is the one stable strong mode. This is retrieval quality + token
    efficiency, NOT end-to-end QA accuracy (which needs a generative LLM, not
    run here), and LongMemEval_M (45 q) not _S.
- **fix(postgres): semantic-recall stub hard-errors, confirmed + documented.**
  The pgvector ANN path returns a clear `Err` ("pgvector ANN search is not
  implemented…") instead of silently returning empty results
  (`crates/mnemo-postgres/src/pgvector_index.rs`, test
  `ann_search_fails_loud_not_silent_empty`). README documents **DuckDB +
  USearch as the supported semantic backend**; an unwired path must never
  return empty.
- **docs:** README Benchmarks section links `bench/RESULTS.md` (one number, one
  caveat, one source).

### Added (2026-06-21) — v0.5.1 cut, active-reconstruction recall strategy (MRAgent, arXiv:2606.06036)

Workspace `0.5.0 → 0.5.1` (patch bump — additive recall option, no breaking change).

- **feat(core): active-reconstruction recall strategy (MRAgent, arXiv:2606.06036).**
  Adds an opt-in `reconstruct` recall strategy
  (`RetrievalMode::Reconstruct` / `strategy = "reconstruct"`). Instead of
  returning only the top-k snippets, it retrieves candidates via the default
  hybrid RRF, walks the existing memory-graph `related_to` edges to gather
  linked/causal context, and synthesises a deterministic **belief-state
  node** returned ALONGSIDE the raw hits — MRAgent's cue → linked-context →
  reconstruct pattern.
  - **Additive, no pivot.** REMEMBER/RECALL/FORGET/SHARE are untouched; the
    `memories` top-k is exactly what `auto` returns, and the belief node is
    a new optional `RecallResponse.reconstruction` field. The default read
    path is unchanged.
  - **Deterministic** (rule-based synthesis, no LLM): same inputs → same
    belief node. `ReconstructedBelief { cue, summary, source_ids,
    linked_context_ids, confidence }`.
  - **Surfaced as a strategy parameter across all four protocols** (no new
    tool): MCP `strategy: "reconstruct"`, REST `?strategy=reconstruct`,
    gRPC `RecallRequest.strategy` (+ new `Reconstruction` message on
    `RecallResponse`), and pgwire `SELECT ... /*+ reconstruct */`.
  - **A/B bench** (`bench/locomo/src/bin/reconstruct_ab.rs`): measures
    gold-coverage@k of `reconstruct` vs. default RRF on an adversarially
    multi-hop fixture so the MRAgent "up-to-23%" claim can be checked on
    mnemo itself (fixture result: coverage@5 0.083 → 0.208). Framed honestly
    as a mechanism check, not an absolute-number claim.
  - **Tests** (`crates/mnemo-core/tests/reconstruct.rs`): belief node carries
    graph-linked context disjoint from sources; typed mode parity; default
    path unchanged; empty-corpus belief.

### Added (2026-06-21) — v0.5.0 cut, topic-document consolidation (Infini-Memory, arXiv:2606.10677)

Workspace `0.4.15 → 0.5.0` (minor bump — new public primitive).

- **feat(core,mcp): topic-document consolidation primitive (Infini-Memory,
  arXiv:2606.10677).** Adds `MnemoEngine::consolidate` and the MCP tool
  `mnemo.consolidate` — a caller-driven primitive that groups a chosen set of
  member memories into one revisable **topic document** ("each topic document
  serves as a semantic unit for collecting related evidence, preserving
  metadata, and revising facts over time"). It is the interactive, by-id
  sibling of the offline `run_consolidation` tag-cluster pass.
  - **Deterministic + protocol-agnostic.** New module
    [`crates/mnemo-core/src/query/consolidate.rs`](crates/mnemo-core/src/query/consolidate.rs)
    with `ConsolidateRequest { memory_ids, topic_name, agent_id?, summary?,
    supersede?, thread_id?, metadata? }` and `ConsolidateResponse`. Absent a
    caller `summary`, the body is a stable join of member contents ordered by
    `(created_at, id)`. Additive engine wrapper — no existing primitive
    changes.
  - **Evidence + provenance preserved.** The topic document records
    `consolidated_from` plus each member's source / timestamp / confidence in
    metadata, and writes `topic_document --consolidated_from--> member`
    relations so the set is retrievable as a unit. Members are permission-gated
    (`check_permission(Read)`) and decrypted on read; a missing/denied/deleted
    member aborts the whole op (nothing partial written).
  - **Fact revision keeps history.** `supersede` makes the new document
    version *N+1* (`prev_version_id → old`); the old document is **retained**
    (not soft-deleted — that would orphan the memory hash chain) and marked
    `Consolidated` with a `superseded_by` pointer. Reuse the same `topic_name`
    so the current-fact resolver (keyed on `topic`) collapses to the current
    view.
  - **Auditability — no dropped provenance.** Two new hash-chained
    `EventType` variants: `MemoryConsolidated` (every consolidation) and
    `MemoryRevised` (on supersede). `verify_integrity` and
    `verify_event_integrity` both stay valid after consolidation + revision.
  - **Surfaces.** MCP `mnemo.consolidate`
    ([`crates/mnemo-mcp/src/tools/consolidate.rs`](crates/mnemo-mcp/src/tools/consolidate.rs)),
    REST `POST /v1/consolidate`, gRPC `rpc Consolidate` (12 RPCs total).
    **pgwire is not extended** — it stays SQL-only (`SELECT`/`INSERT`/`DELETE`
    → recall/remember/forget); consolidation is a primitive-RPC operation, not
    a SQL statement.
  - **Tests** ([`crates/mnemo-core/tests/consolidate.rs`](crates/mnemo-core/tests/consolidate.rs)):
    consolidate-as-unit + relations, provenance metadata, revision-keeps-history,
    hash-chain integrity after consolidation/revision, permission gating,
    empty/missing rejection, and `EventType` serde round-trip.

### Fixed (2026-06-14) — Postgres semantic recall fails loud instead of silent-empty

- **fix(postgres): `PgVectorIndex` ANN search now errors instead of silently
  returning empty.** On the PostgreSQL backend, `semantic` / `auto` (hybrid) /
  `graph` / `domain_scoped` recall previously returned `Ok(vec![])` because
  `PgVectorIndex::search` / `filtered_search` were no-op stubs — making recall
  look like it legitimately found nothing, the most dangerous failure mode for
  a memory database. Both now return a clear `Error::Index` naming the
  limitation, the DuckDB alternative, and the tracking issue. Embeddings are
  still persisted to the pgvector column; only ANN *search* is unimplemented.
  The README now documents DuckDB as the supported vector backend, and real
  pgvector ANN is tracked in
  [#99](https://github.com/sattyamjjain/mnemo/issues/99). Adds a unit test
  asserting the fail-loud behaviour. **No change to the DuckDB path or any
  public API.**

### Added (2026-06-13) — real-embedder retrieval benchmark (`semantic_recall_bench`, bench-only)

- **bench(locomo): real-embedder retrieval-quality bench.** New
  [`semantic_recall_bench`](bench/locomo/src/bin/semantic_recall_bench.rs)
  bin measures mnemo's recall path with a **real local semantic embedder**
  (`nomic-embed-text`, 768-dim, via Ollama) instead of the degenerate
  `NoopEmbedding` the sibling scaffolds use. Metric = gold-document
  recall@1/@3/@5 + MRR, with a deterministic held-out tune/eval query
  split, an auditable `hybrid_weights` / `rrf_k` sweep on the tune split,
  and 5-seed averaging for stable numbers. Report + JSON at
  `bench/locomo/results/semantic_recall_2026-06-13.md`.
  - Held-out eval (mean of 5 seeds): `vector_only` recall@1 **0.739** / MRR
    **0.805** clearly leads; mnemo's default `auto` (RRF) fusion
    *underperforms* on recall@1 (0.452); a vector-dominant weight config
    (`[6,1,0,0]` k=30) recovers most of the gap (0.696) — a real,
    actionable finding that the default `auto` weights are worth revisiting
    for paraphrase-heavy single-fact recall.
  - **Bench-only**: no engine API, access protocol, or retrieval default is
    changed; not the official LLM-judged LongMemEval / LoCoMo QA score
    (gated; #44).

## [0.4.15] — 2026-06-13

The v0.4.10 → v0.4.15 accumulator (tags + GitHub Releases through `v0.4.15`
already exist). Sectioned here on the 2026-06-29 cut so the `[0.5.4]` section
above stays scoped to the 0.5.x line.

### Added (2026-06-13) — v0.4.15 cut, domain-scoped recall (MASDR-RAG, arXiv:2606.11350)

Workspace `0.4.14 → 0.4.15`. Pinned `cargo_pkg_version_matches_v0_4_15`
test and `docs/compat/version-skew-matrix.md` updated.

- **feat(recall): domain-scoped recall mode (anti vector-search-dilution,
  MASDR-RAG 2606.11350).** Adds `RetrievalMode::DomainScoped`, a recall
  mode that restricts the candidate set to a **metadata-defined
  sub-corpus before the dense similarity step**, then runs a single
  vector pass — so off-domain-but-semantically-similar records cannot
  dilute the top-k as the corpus scales.
  - **Diff-compatible:** additive enum variant (→ new `"domain_scoped"`
    strategy) plus an optional `RecallRequest.domain_scope` kwarg
    (`DomainScope { org_id, namespace, doc_class, tags }`,
    `#[serde(default)]`). No existing caller breaks; legacy `strategy`
    and typed `mode` paths are unchanged. A non-empty `domain_scope`
    selects the mode automatically even when `mode` is unset.
  - **Backend-agnostic + RBAC-gated:** the predicate resolves the
    sub-corpus id-set through the existing storage layer (DuckDB +
    PostgreSQL) and is composed with the permission filter, so the ANN
    sees only `(accessible ∩ in-domain)` ids.
  - **MCP surface:** `mnemo.recall` gains a `domain_scope` object
    (`crates/mnemo-mcp/src/tools/recall.rs`); named `domain_scope` (not
    `scope`) because `scope` already filters visibility.
  - **Dilution eval** (`crates/mnemo-core/tests/domain_scoped_dilution.rs`):
    on a corpus growing 50 → 1,000 docs, flat semantic P@10 collapses
    0.100 → 0.000 while domain-scoped holds at 1.000 — asserts the gap at
    the largest size is ≥ 0.05 (it is ~1.0). Plus `DomainScope::matches`
    + serde unit tests in `retrieval.rs`.

### Added (2026-06-11) — v0.4.14 cut, experience-memory tier (DocTrace, arXiv:2606.10921)

Workspace `0.4.13 → 0.4.14`. Pinned `cargo_pkg_version_matches_v0_4_14`
test and `docs/compat/version-skew-matrix.md` updated.

- **Experience-memory tier — cached plan replay (`REMEMBER_PLAN` /
  `RECALL_PLAN`).** DocTrace's two-tier idea (arXiv:2606.10921) as a
  mnemo **mode, not a new store**: tier 1 is the raw memory store; tier 2
  caches a *successful* retrieval/reasoning plan and replays it when a
  structurally-similar query recurs.
  - **New ops on the existing engine surface** (`MnemoEngine::remember_plan`
    / `recall_plan`, module `crates/mnemo-core/src/query/experience.rs`) —
    diff-compatible additions, no change to existing signatures and no new
    `MemoryType` variant.
  - `REMEMBER_PLAN` persists `{query-signature, steps, chunk ids, outcome
    score}` **only** when the outcome clears the success threshold (0.5) —
    failures are never cached. `RECALL_PLAN` returns the best stored plan
    whose signature Jaccard-matches above a threshold (default 0.7), else
    a miss.
  - **Backend-agnostic + RBAC/consent-gated for free:** plans are ordinary
    `MemoryRecord`s (reserved `__experience_plan__` tag + payload in
    `metadata`), so both the DuckDB and PostgreSQL backends work unchanged
    and scope/ACL visibility is enforced exactly like any record. Plan
    records are excluded from ordinary `recall`.
  - **Gated, default-off:** `MnemoEngine::with_experience_memory()` (or the
    `MNEMO_EXPERIENCE_MEMORY=1` env on the CLI/server). With the mode off,
    `remember_plan` errors and `recall_plan` misses, so default behaviour
    is unchanged.
  - **MCP surface:** `mnemo.remember_plan` + `mnemo.recall_plan`
    (`crates/mnemo-mcp/src/tools/experience.rs`).
  - **Tests** (`crates/mnemo-core/tests/experience_memory.rs`):
    store-on-success, replay-on-similar, no-replay-on-dissimilar,
    failures-not-cached, RBAC (private invisible / public visible), and
    mode-off inertness; plus signature/Jaccard unit tests.

### Added (2026-06-09) — agent-controlled memory mode (AutoMEM, arXiv:2606.04315)

- **Agent-controlled memory mode over the MCP tool surface.** Four new
  MCP tools let the agent manage a flat store it curates, so the *agent*
  (not an ingestion heuristic) decides what persists. Anchored on
  [arXiv:2606.04315](https://arxiv.org/abs/2606.04315) (*AutoMEM*).
  - `mnemo.mem_write` / `mnemo.mem_read` / `mnemo.mem_revise` /
    `mnemo.mem_forget` — **thin compositions over the verified
    `remember` / `recall` / `forget` primitives** plus a reserved
    `agent-managed` tag (`crates/mnemo-mcp/src/tools/agent_managed.rs`).
    No new engine enum or method. `mem_revise` = soft-`forget` old +
    `remember` corrected (newest wins); `mem_read` is `recall` scoped to
    the reserved tag.
  - **The default `mnemo.recall` pipeline is unchanged** and remains the
    fallback for single-shot queries; the agent-managed path is additive
    and for long-horizon write-control.
  - **Crossover eval** at
    `crates/mnemo-core/tests/agent_managed_crossover.rs` reproduces the
    paper's single-shot-vs-long-horizon framing on a multi-session
    fixture (12 tracked facts × 3 revisions + 12 incidental details),
    holding retrieval to BM25 to isolate write-control:
    - **fixed-pipeline wins single-shot** incidental recall (1.000 vs
      0.000 — it ingested everything the agent skipped);
    - **agent-managed wins long-horizon** current-fact F1 (1.000 vs
      0.500 — it revised in place, so no stale versions dilute
      precision).
  - MCP contract test (`crates/mnemo-mcp/tests/mcp_test.rs`) verifies the
    tag-scoping invariant (mem_read sees only agent-managed entries; the
    default pipeline still sees everything) and revise supersession.
  - Workspace version unchanged.

### Added (2026-06-08) — budgeted evidence retention (EMBER, arXiv:2606.05894)

- **`RecallRequest.retained_token_budget: Option<usize>` — opt-in
  budgeted evidence retention.** Extends the existing recall surface
  (no new enum); anchored on
  [arXiv:2606.05894](https://arxiv.org/abs/2606.05894) (*EMBER —
  Efficient Memory By Evidence Retention*).
  - When `Some(budget)`, the engine packs the recalled hits into at most
    `budget` retained tokens as verbatim **evidence capsules** (a short
    verbatim excerpt + a **retrieval key** that recovers the full
    record), ranked by a v0 **recoverability heuristic**
    (`recency × retrieval-hit-rate`) — a stand-in for EMBER's learned
    writer. New module `crates/mnemo-core/src/query/retained.rs`.
  - **Purely additive:** the `memories` list is unchanged; capsules ride
    in the new `RecallResponse.retained_evidence`
    (`RetentionReport { capsules, retained_tokens, candidates_examined,
    dropped, … }`). The default read path (no budget) is unaffected.
  - **Eval harness** at
    `crates/mnemo-core/tests/budgeted_evidence_retention.rs` reports
    recall@budget (and F1) on a LongMemEval-style fixture (60 gold facts)
    at a fixed 8192-token budget for budgeted-capsules vs
    naive-truncation: **1.000 vs 0.750** recall, budgeted using ~4.4K of
    the 8192 tokens — so the knob's value is measurable.
  - No protocol surface (MCP / REST / gRPC / pgwire) and no core
    retrieval default is changed; the field is `#[serde(default)]` so
    existing wire payloads deserialize unchanged. Workspace version
    unchanged.

### Added (2026-06-07) — bench-only, no version bump

- **bench/locomo: phase-aware cost attribution (construction/retrieval/generation)
  + 2606.06448 recommendations scorecard.** New `phase_cost` bin + reusable
  `mnemo_locomo_bench::phase_cost` module, anchored on
  [arXiv:2606.06448](https://arxiv.org/abs/2606.06448) (*Agent Memory:
  Characterization and System Implications of Stateful Long-Horizon
  Workloads*).
  - **Phase attribution:** splits every benchmark scenario's cost into the
    paper's three logical phases — **construction** (remember-path:
    embedding calls, prefill tokens, write latency), **retrieval**
    (recall-path: ANN + BM25 + graph + RRF latency, query tokens), and
    **generation** (downstream, *estimated* — mnemo does not generate). Emits
    a per-phase Markdown table (tokens, wall-ms, $-estimate at configurable
    per-1K rates) per scenario via `render_phase_table`.
  - **Scorecard:** `--scorecard-2606-06448` renders mnemo's PASS / PARTIAL /
    FAIL position against the paper's 10 §5 recommendations (quoted verbatim
    in `RECOMMENDATIONS`) as a 10-row table — currently **5 PASS · 5 PARTIAL
    · 0 FAIL**.
  - **Bench-only guardrail:** wired through the existing `mnemo-locomo-bench`
    bench entry point only; no access protocol (MCP / REST / gRPC / pgwire)
    and no retrieval default is touched. Token counts are `ceil(chars/4)`
    estimates and the generation phase is never an LLM call.
  - Workspace version unchanged (bench crate is `publish = false`); README
    bench section updated with a sample per-phase table.

### Added (2026-06-04) — v0.4.13 cut, AMP / memorywire interop adapter

Workspace `0.4.12 → 0.4.13`. Pinned `cargo_pkg_version_matches_v0_4_13`
test and `docs/compat/version-skew-matrix.md` updated.

> Note: the request that drove this cut referenced a "v0.4.4 cycle",
> but the canonical workspace manifest was already at 0.4.12. Per the
> "bump per the canonical Cargo manifest" instruction, this lands as
> 0.4.13 rather than downgrading.

- **New `mnemo-amp` crate — AMP / memorywire wire-format interop
  adapter.** Implements the AMP surface (5 operations × 4 memory types)
  as a `MemoryStore`-conformant layer over a real `MnemoEngine`, so any
  AMP-speaking client can drive mnemo's embedded DuckDB backend
  unchanged. Added to the workspace members + dep aliases.
  - **Wire format (`wire.rs`):** `AmpOp` (`remember` / `recall` /
    `forget` / `merge` / `expire`), `AmpMemoryType` (`episodic` /
    `semantic` / `procedural` / `working`), `AmpEnvelope` request,
    `AmpResult` response, and `schema()` returning a **JSON-Schema
    2020-12** document that pins the 5-op × 4-type surface with
    per-op conditional `required` lists.
  - **Store (`store.rs`):** `MemoryStore` async trait + `MnemoAmpStore`
    impl. `remember` → `engine.remember`; `recall` → `engine.recall`
    (top-k, default 5); `forget` → `engine.forget`. **`merge` and
    `expire` are thin compositions over existing primitives** — not
    assumed engine methods. `merge` folds N records into one
    consolidated record (`remember` + `SourceType::Consolidation`) and
    retires the originals (`forget` + `Consolidate`); it is explicitly
    *not* `engine.merge`, which is a branch-timeline merge. `expire`
    sets `expires_at` + runs `run_ttl_sweep` (there is no
    `engine.expire`).
  - **Router (`router.rs`):** `AmpRouter` single- and fan-out-backend
    entry; writes broadcast to every backend, recall fuses with RRF.
    Ships `rrf_fuse` (Reciprocal Rank Fusion) and `max_fuse` (max-score)
    combiners.
  - **HITL (`approval.rs`):** `ApprovalHook` trait + `AutoApprove` /
    `ClosureApprove` impls. Long-term (`semantic` / `procedural`)
    writes are diffed (`WriteDiff`) and gated before commit; on
    approval a `Decision` event is emitted through mnemo's existing
    hash-chained audit log, so the approve trail is tamper-evident.
    Short-term tiers bypass the gate.
- **Conformance suite (deterministic).** Mirrors the paper's
  cross-adapter checks: **recall@5** on a small labelled corpus driven
  end-to-end through the AMP surface over the embedded DuckDB backend,
  and **RRF-holds-under-rank-0-injection vs max-fusion** (RRF keeps the
  genuinely-relevant item on top; max-fusion is fooled by an
  adversarial rank-0 injection). 14 tests total (9 unit across
  wire/approval/router + 5 integration in
  `crates/mnemo-amp/tests/conformance.rs`) plus an `amp_conformance`
  smoke binary (`cargo run --release --bin amp_conformance -p
  mnemo-amp`) that runs all 5 ops + the fusion check and exits non-zero
  on any failure.
- **Docs:** README gains an AMP row in both the Access-Protocols table
  and the integrations list; `docs/src/integrations/mcp-server.md`
  gains an "AMP / memorywire conformance" section.

No managed-cloud dependency added; the `REMEMBER` / `RECALL` /
`FORGET` / `SHARE` primitive names are untouched; the embedded DuckDB
default is intact.

### Added (2026-06-02) — v0.4.12 cut, cost-aware answer-impact-scored recall

Workspace `0.4.11 → 0.4.12`. Pinned `cargo_pkg_version_matches_v0_4_12`
test and `docs/compat/version-skew-matrix.md` updated.

- **New `mnemo_core::query::evidence` module — cost-aware evidence
  budget.** An opt-in per-query budget that runs over the
  already-ranked recall candidate set and returns the smallest prefix
  that clears a configurable sufficiency bar, capped by an optional
  `max_evidence`. Purely subtractive: it only ever returns a prefix of
  the ranked order, so it cannot reorder or silently lower the
  retrieval's top-k ordering (enforced by an in-module property test).
  - `EvidenceBudget { max_evidence: Option<usize>, stop_when_sufficient:
    bool, sufficiency_threshold: f32, scorer: ScorerKind }` —
    serializable config, attached via the new additive
    `RecallRequest.evidence_budget: Option<EvidenceBudget>` field.
  - `stop_when_sufficient` returns early once the running per-chunk
    score sum clears `sufficiency_threshold`, so callers fetch the
    smallest set that clears the bar instead of front-loading.
- **New `EvidenceScorer` trait — pluggable answer-impact relevance
  signal.**
  - `CosineScorer` (default) — cosine of candidate vs query embedding,
    falling back to the fused retrieval score when embeddings are
    absent or degenerate (e.g. `NoopEmbedding`).
  - `DeltaScorer` — answer-impact scorer that rates a chunk by whether
    adding it to the already-selected evidence would change a
    downstream answer. The judgement is an **injectable closure**
    (`DeltaScorer::new(|ctx| …)`) so the core stays model-agnostic; the
    real LLM callback is supplied by the caller.
    `DeltaScorer::stub()` is a deterministic marginal-novelty heuristic
    for tests / offline use.
  - Attach a custom scorer via the new
    `MnemoEngine::with_evidence_scorer(Arc<dyn EvidenceScorer>)`
    builder. When a budget selects `ScorerKind::Delta` but no scorer is
    attached, the path falls back to cosine rather than erroring.
- **`RecallResponse.evidence_selection: Option<EvidenceSelectionReport>`
  diagnostics** (scorer name, examined vs returned counts, cumulative
  score, `stopped_early` / `capped` flags). Present iff the caller set
  `evidence_budget`. The budget is applied BEFORE `touch_memory`, so
  trimmed-away evidence is not mark-accessed (cost-aware on the write
  side too).
- **Tests:** 7 unit tests in `evidence.rs` (cap respected; early-stop
  at threshold ×2; scorer-trait swappable; injectable closure honoured;
  no-budget passthrough; property: larger budget is a prefix-superset)
  + 6 end-to-end integration tests in
  `crates/mnemo-core/tests/cost_aware_recall.rs` (cap, early-stop,
  no-budget passthrough, delta-scorer-attached, delta-without-scorer
  cosine fallback, prefix-invariant through the engine). The
  integration suite doubles as the public-API smoke test: it imports
  `EvidenceScorer` / `CosineScorer` / `DeltaScorer` from the built
  crate and exercises both scorers through `engine.recall`.

The default read path is unchanged — no `evidence_budget` means the
legacy front-loaded top-`limit`. No managed-cloud dependency added; the
`REMEMBER` / `RECALL` / `FORGET` / `SHARE` primitive names are
untouched; the embedded DuckDB default is intact.

### Added (2026-06-02) — v0.4.11 cut, MemFail per-operation fault-isolation harness

Workspace `0.4.10 → 0.4.11`. Pinned `cargo_pkg_version_matches_v0_4_11`
test and `docs/compat/version-skew-matrix.md` updated.

- **New `mnemo_core::eval::memfail` module** that decomposes each
  end-to-end recall into the three operation seams mnemo exposes —
  `remember` (store), `run_consolidation` (summarize), `recall`
  (retrieve) — and ships three adversarial probe sets engineered so a
  failed assertion is attributable to exactly one stage. Prior-art
  anchor: MemFail's per-operation eval decomposition; mnemo's harness
  targets the real MCP-native primitives, not invented seams.
  - **Store probes** check storage directly (no recall ranking, no
    consolidation): content round-trip + hash, batch atomicity,
    tag round-trip.
  - **Summarize probes** inspect post-consolidation state via direct
    storage reads: cluster emitted, needle string preserved verbatim
    in the `[Consolidated from N memories] …` bundle, originals
    flipped to `ConsolidationState::Consolidated`.
  - **Retrieve probes** assume store has passed in the same run, so
    any failure points at recall: direct-hit by needle string,
    tag-filter scoping.
- **`run_stale_context_fixture` (canonical MemFail "isolate the
  operation" case).** Writes the same fact twice (older write at high
  importance, newer write at low importance), asks the default hybrid
  ranker, and confirms it returns the older / stale record on top.
  Store + summarize probes pass — both records are present in storage
  with intact content hashes and no consolidation has run — so the
  harness attributes the failure to **retrieve**, not summarize. The
  v0.4.7 current-fact-resolver (`fact_key` post-processor on
  `RecallRequest`) is the documented opt-in mitigation; this fixture
  asserts the *attribution shape*, not the retriever's quality.
- **Integration test `crates/mnemo-core/tests/memfail_isolation.rs`**
  exercises the harness end-to-end against an in-memory engine and
  asserts (a) every stage probe passes on a well-formed engine and
  (b) the stale-context fixture lands on
  `Stage::Retrieve`, not `Stage::Summarize`.
- **Module-level unit tests** in `eval/memfail.rs` independently
  exercise each per-stage probe runner against a fresh engine.

5 new test functions (3 module-level unit tests + 2 integration
tests) added under the workspace `cargo test` surface. No new public
trait, no protocol surface change, no managed-cloud dep, no change
to the `REMEMBER` / `RECALL` / `FORGET` / `SHARE` primitive names or
the embedded DuckDB default.

### Added (2026-05-30) — GEM trajectory-correctness audit

- **New `mnemo_compliance::trajectory_audit` function** that replays
  the hash-chained event log for an `(agent_id, thread_id?)` scope and
  computes four GEM-aligned health signals over the state
  trajectory (anchor: [arXiv:2605.26252](https://arxiv.org/abs/2605.26252)):
  - **(a) unregulated-growth** — active-bank size over time vs a
    configured ceiling, with the full per-event timeline returned.
  - **(b) missing-semantic-revision** — facts written under the same
    `fact_key` where older writes were never deleted or redacted,
    listed by `(fact_id, stale_memory_ids)`.
  - **(c) capacity-driven-forgetting** — `MemoryDelete` events whose
    `strategy` payload is missing or outside the five named
    strategies (`soft_delete` / `hard_delete` / `decay` /
    `consolidate` / `archive`).
  - **(d) read-only-retrieval** — scopes that only ever emit
    `MemoryRead` / `RetrievalQuery` / `RetrievalResult` and never a
    write-shaped event.
- **Surfaced through the three protocols that already expose
  `mnemo.verify`:**
  - `mnemo.trajectory_audit` MCP tool (mirrors `mnemo.verify`'s
    `(agent_id, thread_id)` shape; adds `active_bank_ceiling`,
    `fact_key`, `named_forget_strategies` knobs).
  - `POST /v1/compliance/trajectory_audit` REST handler.
  - `TrajectoryAudit` gRPC RPC (new RPC on the existing
    `mnemo.v1.MnemoService`; new `TrajectoryAuditRequest` /
    `TrajectoryAuditResponse` / `TrajectoryFinding` messages — same
    proto file, no new package).
- **Wiring change:** `mnemo-compliance` is now a workspace dep of
  `mnemo-mcp`, `mnemo-rest`, and `mnemo-grpc`. The crate was already
  in the workspace; this just adds the dep edge so the protocol
  crates can call into it. No version bump (mnemo is on a doc-only
  v0.4.4 cycle window; this lands under `[Unreleased]` only).
- **9 unit tests** in `crates/mnemo-compliance/src/trajectory.rs`
  exercise each signal independently (happy-path, breach, fail,
  supersession-then-revision, mixed strategies, agent filter,
  empty-window error). The compliance crate's existing
  `export_audit_log` / `verify_ndjson_signed` tests remain
  untouched.

### Landing trace (2026-05-26)

v0.4.9 cut today (workspace 0.4.8 → 0.4.9). Next cycle's accumulator
opens here. The Auto-Dreamer offline-consolidation bench landed as
commit
[`c34039c`](https://github.com/sattyamjjain/mnemo/commit/c34039c83d5fd313201c62fa10f24187786466f0)
(2026-05-26 admin-merge of PR #96); the embedding-backend selection
bench + SLA-aware recommender is the headline feature of this cut.

### Added (v0.4.10 work-in-progress, 2026-05-29)

- **Feedback-driven consolidation trigger metric.** New
  [`crate::query::maturity`](crates/mnemo-core/src/query/maturity.rs)
  module ships a per-cluster scalar maturity score combining four
  components — access-recency, retrieval-hit success, edge degree in
  the graph store, and pairwise embedding redundancy — with tunable
  weights and saturations. The new
  `ConsolidationPolicy::MaturityDriven(MaturityPolicy)` engine config
  gates `run_consolidation` on the score crossing a configurable
  threshold; the default `ConsolidationPolicy::FixedSize` preserves the
  v0.4.x unconditional-on-size behaviour byte-for-byte. The policy is
  inherited by the existing `forget` and `checkpoint` paths
  (opportunistic, best-effort, never propagates errors), so all four
  access protocols (MCP / REST / gRPC / pgwire) pick it up without a
  new MCP tool. Internal prior-art anchor only:
  [arXiv:2605.28773](https://arxiv.org/abs/2605.28773) (FluxMem) —
  mnemo's policy is a structural cousin, not a reproduction.
- **New `bench/locomo/src/bin/maturity_consolidation.rs` scenario.**
  LoCoMo-style synthetic trace mixing "mature" (backdated, hit,
  edge-rich) and "fresh" (zero-access, no-edge) clusters; runs both
  `FixedSize` and `MaturityDriven` arms and reports `active_bank_ratio`,
  `recall_post`, `clusters_consolidated`, `overreach` (fresh clusters
  consolidated), and a Pareto verdict on the user-specified
  (recall_retained × store_reduction) axes. Markdown + JSON summaries
  written to `bench/locomo/results/maturity_<date>.{md,json}`.
- **2026-05-29 bench result on the synthetic trace.** Maturity arm
  achieves equal recall (`1.0` vs `1.0`), zero overreach (`0` vs `3`
  median), and ~7.7× faster consolidation pass (`~17ms` vs `~133ms`),
  but consolidates fewer clusters, so `active_bank_ratio` is `0.625`
  vs the fixed arm's `0.25`. No Pareto win on the (recall × reduction)
  plane; selectivity win on overreach. **No release tag** until a
  scenario demonstrates a true Pareto improvement.

## [0.4.9] - 2026-05-26

Embedding-backend selection bench + SLA-aware recommender +
Auto-Dreamer-shaped offline consolidation bench. **Measurement and
recommendation only** — no retrieval-default change, no RRF-weights
change, no managed-cloud default. The embedded-first wedge stays.

### Added

- **New `bench/embeddings` crate (criterion + lib +
  `mnemo bench embeddings --slo-ms <N>` CLI subcommand).** Anchored
  on [arXiv:2605.23618](https://arxiv.org/abs/2605.23618) (GE2 vs
  local encoders — quality + latency). For every *configured*
  backend (Noop + a bench-local hashing baseline always;
  `OpenAiEmbedding` if `OPENAI_API_KEY` is set; `OnnxEmbedding` if
  `MNEMO_ONNX_MODEL_PATH` is set AND `mnemo-core` is built with the
  `onnx` feature), the bench measures nDCG@10, recall@10, p50/p95
  single-vector embed latency, and throughput at batch sizes
  1 / 8 / 32 on a 50-document / 10-query labeled fixture checked
  into `bench/embeddings/data/`. The recommender picks the
  **highest-nDCG backend whose p95 ≤ the SLO** and reports the
  explicit nDCG gap vs the absolute best-quality backend. No new
  production embedding backend was added — the bench-local
  `hashing-baseline` is a lexical floor that lives in
  `bench/embeddings/src/lib.rs`, not in `mnemo-core`. See
  [`bench/embeddings/README.md`](bench/embeddings/README.md) for
  the full "what this bench is NOT" block.

- **New `Command::Bench(BenchCommand)` clap variant on the `mnemo`
  binary.** Dispatches `mnemo bench embeddings --slo-ms <N>` to
  `mnemo_embeddings_bench::run_all` + `recommend` + `render_table`.
  No other CLI shape changes; existing subcommands
  (`baseline`, `mcp-server`, `eval`) are untouched.

- **Auto-Dreamer-shaped offline consolidation bench**
  ([`bench/locomo/src/bin/auto_dreamer_consolidation.rs`](bench/locomo/src/bin/auto_dreamer_consolidation.rs)).
  Exercises the engine's existing
  [`mnemo_core::query::lifecycle::run_decay_pass`](crates/mnemo-core/src/query/lifecycle.rs)
  + [`run_consolidation`](crates/mnemo-core/src/query/lifecycle.rs)
  path end-to-end on a synthetic multi-session trajectory (8 sessions ×
  25 facts × 5 trials by default) and reports
  `active_bank_ratio = post / pre` (expects `< 1.0`) and held-out
  `recall_post >= recall_pre`. Emits a Markdown report
  (`bench/locomo/results/auto_dreamer_<YYYY-MM-DD>.md`) plus a JSON
  summary (`auto_dreamer_<YYYY-MM-DD>.json`) carrying
  `active_bank_ratio`, `recall_pre`, `recall_post`, and the
  offline-pass elapsed time. No new public API surface.

### Landing trace (2026-05-23)

v0.4.8 cut today (workspace 0.4.7 → 0.4.8). Next cycle's accumulator
opens here. The v0.4.7 land was commit
[`df84482`](https://github.com/sattyamjjain/mnemo/commit/df84482)
(2026-05-22 admin-merge of PR #88 — MINTEval interference scenario
+ current-fact resolver).

## [0.4.8] - 2026-05-23

PEEK-anchored orientation cache (constant-token "context map")
recall mode. Adds an opt-in post-processor over the standard recall
result set that maintains a per-namespace, fixed-token-budget map of
entities + `UPPER_SNAKE` constants + fenced schema fragments
distilled from each hit, and returns a bounded rendering alongside
`top-k`. Default read path is unchanged.

### Added

- **New `mnemo_core::query::orientation_cache` module.** Carries
  `OrientationCacheConfig { namespace, token_budget,
  include_in_response, distill }` + `OrientationCacheStore`
  (in-process `Arc<RwLock<HashMap<namespace, ContextMap>>>`) +
  `RenderedContextMap { namespace, entities, constants, schemas,
  token_estimate, budget, hit_count }` + a heuristic `distill()`
  + a priority-evictor `evict_to_budget()` + a one-shot
  `update_and_render()` driver. The Distiller extracts
  capitalized noun phrases (entities), `UPPER_SNAKE = value`
  / `UPPER_SNAKE: value` pairs (constants), and fenced ```` ``` ````
  blocks + `CREATE TABLE` / `interface` / `type` / `struct`
  declarations (schemas). The Evictor scores entries by
  `freq × recency × (1 - token_share)` and drops the lowest
  until under budget. **8 unit tests** cover entity / constant /
  schema extraction, namespace derivation + override, bounded
  rendering, eviction at small budget, namespace isolation,
  read-only config, and budget invariance across many updates.

- **New `RecallRequest.orientation_cache: Option<OrientationCacheConfig>`
  field.** Backwards-compatible additive field. When `Some` AND
  the engine has an `OrientationCacheStore` attached via
  `MnemoEngine::with_orientation_cache_store()`, the engine
  updates the per-namespace map from each hit and (when
  `include_in_response = true`) returns the bounded rendering in
  the response.

- **New `RecallResponse.orientation_cache: Option<RenderedContextMap>`
  field.** Surfaces the bounded map when the cache is enabled
  AND the config did not set `include_in_response = false`.

- **New `MnemoEngine.orientation_cache_store` +
  `with_orientation_cache_store()` builder.** Per-engine attach
  point for the in-process namespace-keyed store. Mirrors the
  existing `with_cache` / `with_encryption` pattern.

- **MCP `recall` tool param `orientation_cache`.** New
  `RecallOrientationCacheInput { namespace, token_budget,
  include_in_response, distill }` in
  [`crates/mnemo-mcp/src/tools/recall.rs`](crates/mnemo-mcp/src/tools/recall.rs)
  threaded through the MCP tool dispatch. The MCP response JSON
  carries a top-level `orientation_cache` field when the rendered
  map is present.

- **REST recall query params** `orientation_cache`,
  `orientation_namespace`, `orientation_token_budget`,
  `orientation_include_in_response`, `orientation_distill` on
  `GET /v1/memories`. Wires through to the config without
  changing the default query semantics.

- **gRPC `OrientationCacheRequest` + `OrientationCacheResponse` +
  `OrientationEntry`** added to `mnemo.proto`. New optional
  `RecallRequest.orientation_cache` field (proto field 14) +
  new optional `RecallResponse.orientation_cache` field (proto
  field 3). Wired in `crates/mnemo-grpc/src/lib.rs` recall
  handler.

- **pgwire `/*+ orientation_cache */` SQL hint comment.** The
  parser sets `SelectQuery.orientation_cache = true` when the
  query contains this directive; the server then attaches a
  default `OrientationCacheConfig::new()` to the underlying
  `RecallRequest`. Minimal first-cut surface (no namespace /
  budget overrides through SQL yet — REST / MCP / gRPC carry the
  full config knobs).

- **New `bench/locomo/src/bin/orientation.rs`** — PEEK-shaped
  repeated-context scenario. For each `K ∈ {3, 6, 10, 15}`, seeds
  30 shared-context facts referencing a fixed cast + issues `K`
  related recall calls per trial, comparing hybrid-only vs
  orientation-cache arms. Reports p50 payload tokens per call +
  p50 latency + top-1 hit parity. Writes
  `bench/locomo/results/orientation_<YYYY-MM-DD>.md`. Anchored
  on [arXiv:2605.19932](https://arxiv.org/abs/2605.19932) in
  the module doc-comment.

- **README "Repeated-context recall — orientation cache (v0.4.8)"
  subsection** under Access Protocols with primary-source link +
  pointer to the module + pointer to the bench scenario +
  explicit "not a learned summariser / not a context-window
  extender / not persisted" disclaimers.

- **`bench/locomo/README.md`** gains a row for the new
  `orientation` bin alongside the existing three.

### Changed

- **Workspace version 0.4.7 → 0.4.8.** Cargo.toml workspace +
  internal-crate dep pins; python/pyproject.toml;
  sdks/typescript/package.json; sdks/go/mnemo.go (`Version`
  const); python/mnemo/__init__.py `__version__`. Regression
  tests bumped: `cargo_pkg_version_matches_v0_4_8` (renamed from
  `_v0_4_7`) + `test_v0_4_8_pinned` (renamed from
  `_v0_4_7_pinned`).

- **~30 RecallRequest construction sites** across mnemo-core
  (engine + benches + integration tests), mnemo-grpc,
  mnemo-pgwire, mnemo-rest, mnemo-letta, mnemo-mcp tests,
  mnemo-cli, python/src/lib.rs, and bench/locomo bins updated
  to set `orientation_cache: None` (matches the additive-field
  pattern from v0.4.4's `mode` addition and v0.4.7's
  `current_fact_resolver` addition).

### Honest scope — what's NOT in v0.4.8

- **NOT a write-side memory consolidator.** The cache only
  summarises hits as they pass through recall; it does not
  rewrite, compact, or persist any memory record on disk.
- **NOT a learned summariser.** The Distiller is heuristic by
  choice — regex-free pure-Rust extraction of capitalized
  entities, `UPPER_SNAKE` constants, and fenced/declared schemas.
  An LLM-backed Distiller variant is parked for v0.5.x; treat the
  extracted entries as pointers, not paraphrases.
- **NOT a context-window extender.** The rendered map fits inside
  the recall response payload and is bounded by the caller's
  `token_budget` (default 512). The cache does not bypass any
  model context limit.
- **NOT a faithful PEEK reproduction.** PEEK uses a learned
  prefix encoder and a write-side update path. This module
  adopts the *orientation map + constant-token budget* shape
  only. The bench measures the *shape* of the savings, not the
  absolute number PEEK reports.
- **NOT persisted.** The store is in-process
  (`Arc<RwLock<HashMap<..>>>`). Restart drops it. Persistence
  to DuckDB / Postgres is a v0.5.x knob.
- **Token estimate is `(len / 4)`-heuristic, not `tiktoken-rs`.**
  Calibration against a real tokenizer is a follow-up for
  production sizing decisions.
- **pgwire surface is minimal.** Only the boolean hint
  `/*+ orientation_cache */` is parsed; namespace + budget
  overrides through SQL are deferred. Full-config knobs travel
  through MCP / REST / gRPC today.

## [0.4.7] - 2026-05-22

Interference bench scenario + current-fact resolver recall mode
(MINTEval-anchored). Adds an opt-in post-processor over the standard
recall result set that groups candidates by a caller-chosen
`fact_key` (typical: `"fact_id"`) and keeps the most-recent write
per group, with the older versions optionally returned as a
supersession chain. Default read path is unchanged.

### Added

- **New `mnemo_core::query::current_fact_resolver` module.** Carries
  `CurrentFactResolverConfig { fact_key, include_supersession_chain }`
  + `resolve()` + `ResolverOutput { kept, superseded }`. The resolver
  groups by JSON metadata pointer, picks the record with the latest
  `updated_at` (ties → higher score → higher UUID v7), and returns
  the older entries as a `SupersededRecord` chain. Records missing
  the `fact_key` pass through untouched. **6 unit tests**: most-recent
  wins, supersession chain when enabled, records-without-fact-key
  pass through, multi-group resolution, empty-candidate-set,
  integer fact-id support.

- **New `RecallRequest.current_fact_resolver: Option<CurrentFactResolverConfig>`
  field.** Backwards-compatible additive field on the existing
  request struct. When `Some`, the engine dispatches the resolver
  AFTER the normal hybrid recall completes. **The default read path
  is unchanged.**

- **New `RecallResponse.superseded: Option<Vec<SupersededRecord>>`
  field.** Surfaces the supersession chain when the resolver was
  enabled with `include_supersession_chain = true` AND any
  candidates were actually dropped. `SupersededRecord` carries
  `{id, fact_id, superseded_by, superseded_at, prior_updated_at}`
  so an auditor can reconstruct the timeline.

- **MCP `recall` tool param `current_fact_resolver`.** New
  `RecallCurrentFactResolverInput { fact_key, include_supersession_chain }`
  in [`crates/mnemo-mcp/src/tools/recall.rs`](crates/mnemo-mcp/src/tools/recall.rs)
  threaded through the MCP tool dispatch. The MCP response JSON
  carries a top-level `superseded` field when the chain is present.

- **REST recall query params** `current_fact_key` +
  `current_fact_include_chain` on `GET /v1/memories`. Wires
  through to the resolver config without changing the default
  query semantics.

- **New `bench/locomo/src/bin/interference.rs`** — MINTEval-shaped
  scenario. For each `K ∈ {1, 3, 5, 10}`, seeds 50 distractor
  facts + revises a target fact `K + 1` times under the same
  `fact_id`, then queries via both the default read path and the
  resolver arm. Reports current-fact-accuracy@K + supersession-chain
  length per K, p50 latency for each arm. Writes
  `bench/locomo/results/interference_<YYYY-MM-DD>.md`. Anchored
  on [arXiv:2605.18565](https://arxiv.org/abs/2605.18565) in the
  module doc-comment.

- **README "Memory under interference — current-fact resolver
  (v0.4.7)" subsection** under Access Protocols with primary-source
  link + pointer to the resolver module + pointer to the bench
  scenario + explicit "not a contradiction detector / not a
  write-side guard" disclaimers.

- **`bench/locomo/README.md`** gains a row for the new `interference`
  bin alongside the existing `mnemo-locomo` + `grep_vs_vector_replay`
  rows.

- **`tests/readme_no_marketing_phrases.rs` banlist extended** with
  four MINTEval overclaim phrasings: `MINTEval-compliant`,
  `interference-proof`, `supersession-perfect`, `MINTEval-resistant`.

### Changed

- **Workspace version 0.4.6 → 0.4.7.** Cargo.toml workspace +
  internal-crate dep pins; python/pyproject.toml;
  sdks/typescript/package.json; sdks/go/mnemo.go (`Version` const
  + package doc-comment); python/mnemo/__init__.py `__version__`.
  Regression tests bumped: `cargo_pkg_version_matches_v0_4_7`
  (renamed from `_v0_4_6`) + `test_v0_4_7_pinned` (renamed from
  `_v0_4_6_pinned`).

- **~20 RecallRequest construction sites** across mnemo-core,
  mnemo-grpc, mnemo-pgwire, mnemo-rest, mnemo-letta, mnemo-mcp
  tests, integration tests, benches, bench/locomo bins, and
  mnemo-cli updated to set `current_fact_resolver: None` (matches
  the additive-field pattern from v0.4.4's `mode` addition).

### Honest scope — what's NOT in v0.4.7

- **NOT a contradiction detector.** Two records with the same
  `fact_key` value are treated as versions of one fact; the
  resolver does NOT inspect content semantics. The operator picks
  `fact_key` to mean what they want.
- **NOT a write-side guard.** The resolver only re-ranks reads;
  contradictory writes are accepted by the existing engine path
  unchanged. Operators wanting write-side conflict prevention use
  the existing `crate::query::conflict` module.
- **NOT a gRPC proto extension.** The new field is wired through
  Rust + MCP + REST today. The gRPC proto and pgwire SQL surface
  carry `current_fact_resolver: None` as a padding default; the
  full grpc proto bump is deferred to v0.5.x.
- **NOT a faithful MINTEval reproduction.** The bench bin uses a
  synthetic distractor corpus + deterministic exact-content
  scoring. The official MINTEval metric (GPT-judge over a curated
  benchmark corpus) is gated behind the same secrets as
  [#44](https://github.com/sattyamjjain/mnemo/issues/44).
- **NOT a re-ranker for the underlying retrieval.** The resolver
  runs over whatever candidates the underlying `RetrievalMode`
  produced. It does not re-issue a query.

## [0.4.6] - 2026-05-21

Substrate-anchor release. Net-new v0.4.6 surface: a vertical-slice
implementation of the [`golem:vector@1.0.0`](https://github.com/golemcloud/golem-ai/issues/21)
WIT contract, two-crate host-runner architecture, with mnemo-core
on the host side because the engine's C++ deps (DuckDB + USearch)
cannot compile to `wasm32-wasip2`.

### Added

- **New `crates/mnemo-golem-wit` workspace crate.** WIT-bindings
  crate built with `cargo-component v0.21.1`. Implements 3 of 30
  upstream functions — `upsert-vector`, `search-vectors`,
  `delete-vectors` — each delegating to a host import. Compiles
  cleanly to `wasm32-wasip2`; the release artifact is ~73K at
  `target/wasm32-wasip1/release/mnemo_golem_wit.wasm`. WIT
  package is `mnemo:golem-vector@0.1.0` (namespaced under
  `mnemo:` to signal the subset, not the full upstream contract).

- **New `crates/mnemo-golem-host` workspace crate.** Rust host
  crate that owns an `Arc<MnemoEngine>` and supplies the WIT host
  imports. Ships:
  - `trait MnemoGolemProvider` — async Rust shape of the three
    host imports.
  - `struct MnemoGolemHost { engine }` — backs the trait with
    mnemo's `remember` / `recall` (semantic top-K) / `forget`
    (HardDelete) operations; maps the WIT `collection` parameter
    to mnemo's `agent_id` namespace.
  - **5 integration tests**: put → search round-trip,
    collection-scoping isolates writes, delete-removes-only-targeted-ids,
    upsert-rejects-empty-vector, search-rejects-empty-query.
  - **End-to-end example** at
    `examples/golem_agent_round_trip.rs` showing REMEMBER →
    RECALL → DELETE through a real `MnemoEngine` (3 upserts + 1
    search + 1 delete + 1 post-delete search).

- **New research-anchor doc**
  [`docs/research/golem-vector-wit-provider.md`](docs/research/golem-vector-wit-provider.md)
  documenting the architectural reality (DuckDB / USearch ↛ WASM),
  the two-crate host-runner split, the WIT subset shipped today,
  the wasmtime-component-loader wiring step explicitly deferred
  to v0.5.x, the per-function gap list (6 Collections + 8
  Vectors-extended + 5 Search-Extended + 3 Analytics + 5
  Namespaces + 4 Connection = **27 deferred**, **3 shipped** = 30
  upstream contract), and the explicit non-overclaim disclaimers
  (NOT a Golem-durability claim, NOT a multi-provider abstraction,
  NOT a real embedder integration, NOT a bounty-claimable
  submission for the full contract).

- **README "mnemo as a golem:vector provider (v0.4.6)" subsection**
  under Access Protocols with primary-source link to
  golemcloud/golem-ai#21 + pointer to both new crates + pointer to
  the research anchor + explicit honest framing of the deferred
  wasmtime wiring.

- **`tests/readme_no_marketing_phrases.rs` banlist extended** with
  five golem:vector overclaim phrasings: `Golem-durable by
  construction`, `golem:vector-compliant`, `Qdrant killer`,
  `Pinecone killer`, `WIT-component-perfect`.

### Changed

- **Workspace version 0.4.5 → 0.4.6.** `Cargo.toml` workspace +
  internal-crate dep pins; python/pyproject.toml; sdks/typescript
  package.json; sdks/go mnemo.go (`Version` const + package
  doc-comment); python/mnemo/__init__.py `__version__`. Regression
  tests bumped: `cargo_pkg_version_matches_v0_4_6` (renamed from
  `_v0_4_5`) + `test_v0_4_6_pinned` (renamed from `_v0_4_5_pinned`).

- **Workspace member list extended** with two new entries:
  `crates/mnemo-golem-wit` and `crates/mnemo-golem-host`.

### Honest scope — what's NOT in v0.4.6

- **NOT the full golem:vector contract.** 3 of 30 functions
  shipped; 27 deferred to v0.5.x with the per-interface rationale
  in the research doc.
- **NOT the wasmtime-component-loader wiring.** The Rust trait +
  mnemo-core integration ship today; the
  `wasmtime::component::Linker` + bindgen host bindings + async
  trampoline step is documented as a v0.5.x row.
- **NOT a Golem-durability claim.** Component runs on Golem the
  same way any guest does; mnemo does not introspect Golem's
  checkpoint protocol.
- **NOT a multi-provider abstraction.** mnemo is one provider;
  routing across Qdrant / Pinecone / Milvus / pgvector is out of
  scope.
- **NOT a real embedder integration.** Vectors arrive
  pre-computed via the WIT; today's slice uses `NoopEmbedding` for
  the test setup and demonstrates the wiring, not the embedder.
- **NOT a bounty-claimable submission for the full upstream
  contract.** The vertical slice + host-runner scaffold is the
  bounty's first deliverable shape; a v0.5.x follow-up closes the
  remaining 27 functions to be bounty-claimable.

## [0.4.5] - 2026-05-20

Substrate-anchor release. Net-new v0.4.5 surface: an attention-state
memory store, anchored on the
[arXiv 2605.18226 Context Memorization](https://arxiv.org/abs/2605.18226)
result (Okoshi et al., Institute of Science Tokyo + Imperial College
London, surfaced 2026-05-19). The paper names "a lightweight,
lookup-based memory of precomputed attention states" as the
substrate prefix-augmented inference reaches for; v0.4.5 ships that
substrate without claiming to implement the full Context
Memorization mechanism (the producer + consumer are external — see
the honest scope below).

### Added

- **New `crates/mnemo-attention-state` workspace crate.** Typed
  `AttentionStateStore` trait + `InMemoryAttentionStateStore`
  reference implementation + serializable `AttentionStateRecord`
  envelope (id / agent_id / prefix_hash / model / state_blob /
  blob_sha256_hex / ttl_seconds / created_at). Six unit tests cover
  put → get round-trip, get-miss, put-overwrites-existing,
  SHA-256-matches-input, agent-scoping-isolates-writes,
  delete_for_agent-removes-only-that-agents-records.

- **2 new MCP tools** on `MnemoServer`: `mnemo.attention_state.put`
  and `mnemo.attention_state.get`. Tools dispatch into the store
  when `MnemoServer::with_attention_state(...)` is configured at
  startup; **unconfigured calls return a spec-shaped error result,
  not a panic.** Blobs travel hex-encoded on the JSON-RPC wire to
  keep transport string-safe. Three integration tests in
  `crates/mnemo-mcp/tests/attention_state_tools.rs` exercise the
  store contract through the same `AttentionStateStore` trait the
  tools dispatch into.

- **New research-anchor doc**
  [`docs/research/context-memorization-2605.18226.md`](docs/research/context-memorization-2605.18226.md)
  documenting what the paper measures, where mnemo fits (store
  only), what this anchor is explicitly NOT (not a Context
  Memorization implementation, not an inference-runtime
  integration, not a RECALL fast-path, not a stability claim on
  blob format, not encrypted-at-rest at the storage trait, not a
  benchmark), the operator recipe for putting the substrate to
  work today, and the v0.4.4 vs v0.4.5 layering (RetrievalMode
  HarnessAware vs the new orthogonal attention-state store).

- **README "Attention-state-memory substrate (v0.4.5)" subsection**
  under Access Protocols with primary-source link to arXiv 2605.18226
  + pointer to the new crate + pointer to the new MCP tools +
  explicit honest framing of producer / consumer scope.

- **`tests/readme_no_marketing_phrases.rs` banlist extended** with
  four Context-Memorization overclaim phrasings:
  `Context-Memorization-compliant`, `attention-state-compatible`,
  `KV-cache-portable`, `prefix-cache by construction`.

### Changed

- **Workspace version 0.4.4 → 0.4.5.** Cargo.toml workspace +
  internal-crate dep pins; python/pyproject.toml; sdks/typescript
  package.json; sdks/go mnemo.go (`Version` const + package
  doc-comment); python/mnemo/__init__.py `__version__`. Regression
  tests bumped: `cargo_pkg_version_matches_v0_4_5` (renamed from
  `_v0_4_4`) + `test_v0_4_5_pinned` (renamed from `_v0_4_4_pinned`).

- **`mnemo-mcp` adds `hex = { workspace = true }` dependency.** The
  new MCP tool methods hex-encode / hex-decode the state blob at
  the JSON-RPC wire boundary.

### Honest scope — what's NOT in v0.4.5

- **NOT a Context Memorization implementation.** mnemo does not
  extract prefix attention states from any inference runtime. The
  producer is out of scope.
- **NOT an inference-runtime integration.** mnemo does not wire to
  vLLM, TGI, Triton, or any specific runtime. The mechanism is
  transport-agnostic.
- **NOT a RECALL fast-path.** Existing semantic + BM25 + graph +
  recency hybrid retrieval does NOT consult the attention-state
  store. Substrates sit orthogonal. Future v0.5.x row may explore
  the composition.
- **NOT a stability claim on the blob format.** The
  `AttentionStateRecord` schema is starter; pin v0.4.5 minor if
  relying on byte-level layout.
- **NOT encrypted-at-rest at the storage trait.** The in-memory
  reference store holds bytes as `Vec<u8>`. Encryption is the
  operator's responsibility at the tool / engine layer using the
  existing `mnemo-core::encryption::ContentEncryption` helper.
- **NOT a persistent backend.** v0.4.5 ships only
  `InMemoryAttentionStateStore`. A DuckDB / PostgreSQL backend is
  a future minor.
- **NOT a benchmark.** No bench harness compares attention-state
  lookup cost vs prefix recomputation.

### Also-landed in this cycle

- **(2026-05-18) — LangGraph 1.x checkpoint adapter wrap-up**
  shipped 2026-05-18 in commit
  [`0cf6f39`](https://github.com/sattyamjjain/mnemo/commit/0cf6f3939c92cbe494eb8b1118faf9595b74f427)
  before today's substrate row. `python/mnemo/checkpointer.py` adds
  **`MnemoCheckpointer`** as the canonical class name; the legacy
  `ASMDCheckpointer` is preserved as a back-compat alias so existing
  `from mnemo.checkpointer import ASMDCheckpointer` imports continue
  to work. The module docstring now documents the LangGraph 1.x
  ``BaseCheckpointSaver`` surface coverage explicitly: primaries
  (`get_tuple`, `put`, `delete_thread`) are implemented; `list` +
  `put_writes` are stubs with the contract recorded in the
  docstring. New tests in
  [`python/tests/test_langgraph_checkpointer.py`](python/tests/test_langgraph_checkpointer.py)
  cover put→get_tuple round-trip, thread isolation, branch
  round-trip, delete_thread, stub-method contracts, and the
  back-compat-alias identity (`ASMDCheckpointer is MnemoCheckpointer`).
  Tests use a `_FakeMnemoClient` shim so the suite does NOT spawn
  the mnemo binary. New
  [`examples/langgraph_checkpointer.py`](examples/langgraph_checkpointer.py)
  shows a 5-line `StateGraph` + `MnemoCheckpointer` integration.
  [`python/README.md`](python/README.md) integrations table swaps
  `ASMDCheckpointer` → `MnemoCheckpointer` with the back-compat
  alias annotated inline. `mnemo.availability` registers both names
  so the soft-import probe surfaces either.

  **Honest scope:** the wrap-up closed the parked
  `mnemo-langgraph` v0.4.4-backlog item via the existing Python
  adapter; **no new Rust crate shipped** because LangGraph is
  Python-only and a Rust `crates/mnemo-langgraph/` shell would
  have no downstream consumer. The Python adapter's `list` +
  `put_writes` stubs are unchanged — the v0.4.4-backlog inventory
  was moved from "ship the crate" to "implement `list` + per-thread
  `put_writes` enumeration" as a v0.5.x follow-up.

## [0.4.4] - 2026-05-17

Substrate-anchor release. Twelve days of `[Unreleased]` accumulator
(2026-05-05 → 2026-05-17) shipping the four substrate-composition
anchors of the cycle (Dreams curator, ARGUS read-side audit,
DELEGATE-52 outcome-diff, MCP 2026 Roadmap Enterprise-Readiness)
plus today's two-PR ship:

- **PR-A (bench scaffold)** — new `[[bin]] grep_vs_vector_replay` in
  `bench/locomo` routing a LongMemEval-shaped slice through
  `mnemo.recall` in three modes (`vector_only` / `bm25_only` /
  `rrf_hybrid`) and emitting a Markdown table per run. Reproduces
  the Sen et al. arXiv:2605.15184 experiment design against mnemo's
  own substrate. Operator-runnable today against the bundled
  45-record `longmemeval_m.jsonl`; the gated 116-question slice +
  GPT-judge-scored official metric require the same secrets as
  [#44](https://github.com/sattyamjjain/mnemo/issues/44).
- **PR-B (RetrievalMode typed enum)** — new `mnemo_core::retrieval`
  module landing `RetrievalMode` typed enum (`VectorOnly` / `Bm25Only`
  / `HybridRrf` / `Graph` / `HarnessAware { harness, format }`) + 5
  starter `HarnessAware` adapters (`ClaudeCodeEnvelope`,
  `CodexEnvelope`, `GeminiCliEnvelope`, `ChronosEnvelope`,
  `GenericEnvelope`). `RecallRequest.mode: Option<RetrievalMode>` is
  added as an **additive** field — the legacy
  `RecallRequest.strategy: Option<String>` stays in place and SDKs
  (Python / TypeScript / Go) continue to work unchanged. New
  research-anchor doc at
  [`docs/research/grep-vs-vector-2605.15184.md`](docs/research/grep-vs-vector-2605.15184.md).
  README "Why mnemo" gains a paragraph framing the
  `HarnessAware` lever against the paper's envelope-format finding.

### What this release is NOT

- Not a breaking change for SDK callers — `strategy: Option<String>`
  is preserved; new `mode` field is additive.
- Not a stability claim on the 5 `HarnessAware` adapter envelope
  contents — each adapter is a starter implementation; pin the
  v0.4.4 minor version if relying on a specific shape.
- Not an implementation of any external paper's retrieval / audit /
  curation model. The four research anchors that accumulated in
  `[Unreleased]` since 2026-05-05 (Dreams, ARGUS, DELEGATE-52,
  arXiv:2605.15184) all carry explicit composition-anchor
  disclaimers in their respective doc files.
- Not a GPT-judge-scored bench result. The `grep_vs_vector_replay`
  bin produces a deterministic exact-substring smoke metric today;
  the official LongMemEval metric stays gated behind #44.

### Added (cycle highlights)

- `mnemo_core::retrieval::RetrievalMode` typed enum + 5
  `HarnessAware` adapters.
- `bench/locomo/src/bin/grep_vs_vector_replay.rs` runnable scaffold
  bin (PR-A; landed in cycle commit `cde9f68`).
- `docs/research/grep-vs-vector-2605.15184.md` composition anchor.

### Landing trace (2026-05-06)

Recorded one day after PR #76 merged so a future operator reading
`[Unreleased]` can verify the rows below are not in a local-only
state.

- All three rows below (A1 Project Think anchor, U1 Sierra evidence
  + corrected v0.4.3 verification trace + spec-drift footer, U2 v0.4.3
  publish-status doc) shipped on `main` in commit
  [`2802616`](https://github.com/sattyamjjain/mnemo/commit/280261639837d9cf84e387347b2732c162c93bec)
  at 2026-05-05T07:40:03Z via [PR #76](https://github.com/sattyamjjain/mnemo/pull/76).
- v0.4.4 cycle now contains 4 rows (the three above + today's two —
  see Added/Changed below for U1 MCP 2026 Roadmap and U2 landing-trace
  + parked-crate inventory).
- Workspace version unchanged at `0.4.3`. v0.4.4 cuts when a runtime
  / code surface lands on top of this `[Unreleased]` block, not on
  every docs-only row land.

### Parked for v0.4.4 backlog

The crates below are referenced by the daily-prompt ledger and the
`docs/comparisons/` + `docs/src/integrations/` family but have **not
yet landed on `main`**. Listed here so contributors reading
`CHANGELOG.md` see the v0.4.4 backlog in one place rather than
parsing 17 days of prompt history.

- **`mnemo-bench-cf`** (M-effort) — full Cloudflare bench harness
  baselining mnemo against (a) the hosted Agent Memory KV+Vectorize
  service and (b) the DO Facets SQLite-per-DO substrate. Strongest
  v0.4.4 headline candidate. Empty-bench placeholders are tracked
  in [`docs/comparisons/cloudflare-agent-memory.md`](docs/comparisons/cloudflare-agent-memory.md).
- **`mnemo-langgraph` Rust crate — RETIRED 2026-05-18.** The parked
  item was a Rust shell that would have had no downstream consumer.
  The functionally-equivalent Python adapter (now canonical name
  `MnemoCheckpointer`, back-compat alias `ASMDCheckpointer`) covers
  LangGraph 1.x's `BaseCheckpointSaver` interface in `python/mnemo/checkpointer.py`.
  Remaining work (implement the stub `list` + `put_writes` methods)
  is rebased to a v0.5.x follow-up — see today's `[Unreleased]`
  Added entry above.
- **`mnemo-purview`** (M-effort) — Microsoft Purview audit-log
  adapter. No S-shippable subset surfaced yet.
- **`mnemo-toolhive`** (S) — Stacklok ToolHive Registry sync.
  Opportunistic; no blocking dependency.
- **`mnemo-envelope`** + `EnvelopeKind::FetcherAttestation` +
  agent-vs-human authorship tag (M-effort, chained) — OTel exporter
  envelope kind. Two follow-ups are blocked on this crate landing
  first.
- **`mnemo-aas01`** (M-effort) — OWASP AAS01 detector surface.
- **`mnemo-mgt`** (M-effort) — SecureAuth Trust Registry adapter.
- **`bench/locomo` LongMemEval / BEAM extension** (S/M) — track
  Mem0g 68.4% / MemPalace 96.6% LongMemEval / Hindsight BEAM 10M-tier
  numbers in the existing `bench/locomo` crate. Source URLs are
  31-58 days old (outside ≤7d primary-trigger gate); high-value as
  a v0.4.4 headline alongside `mnemo-bench-cf`.

### Added

- **U1 (v0.4.4, 2026-05-09) — Anthropic Dreams Research Preview substrate
  anchor.** README `### Memory curation interop (Dreams, Routines, and
  substrate primitives)` sub-section inside Key Features, citing the
  [Dreams Research Preview docs](https://platform.claude.com/docs/en/managed-agents/dreams)
  (surfaced 2026-05-06 at Code w/ Claude SF, 3 days old at land-time)
  and the companion [Routines doc](https://code.claude.com/docs/en/routines).
  New companion comparison doc
  [`docs/comparisons/anthropic-dreams.md`](docs/comparisons/anthropic-dreams.md)
  with curator-action ↔ substrate-primitive layering table; explicit
  non-overlap callout (Dreams owns *what to curate*, mnemo owns *how
  to durably store with audit trail*). One-sentence cross-link from
  [`docs/comparisons/cloudflare-project-think.md`](docs/comparisons/cloudflare-project-think.md)
  noting Project Think (runtime) + MCP 2026 Roadmap (protocol) +
  Dreams (curator) together describe the runtime + protocol + curator
  picture, with mnemo as the offline-auditable substrate underneath.
  **Honest framing:** the Dreams API is Research Preview behind a
  Request-access form; **mnemo does NOT today ship an Anthropic-API
  adapter.** A `mnemo-dreams` adapter crate is plausible if/when the
  API exits Research Preview but is explicitly NOT on the v0.4.x
  backlog.

- **A1 (v0.4.4) — Cloudflare Project Think positioning anchor.**
  README `### Project Think — loop vs. ledger` sub-section inside the
  existing "Why mnemo when Cloudflare Agent Memory exists?" H2,
  citing the [Project Think announcement](https://blog.cloudflare.com/project-think/)
  (2026-05-04, 1 day old at land-time). New companion comparison doc

- **A1 (v0.4.4) — Cloudflare Project Think positioning anchor.**
  README `### Project Think — loop vs. ledger` sub-section inside the
  existing "Why mnemo when Cloudflare Agent Memory exists?" H2,
  citing the [Project Think announcement](https://blog.cloudflare.com/project-think/)
  (2026-05-04, 1 day old at land-time). New companion comparison doc
  [`docs/comparisons/cloudflare-project-think.md`](docs/comparisons/cloudflare-project-think.md)
  treating Project Think as the *runtime layer* and mnemo as the
  *audit-ledger layer* — explicitly **complementary, not substitute**
  surfaces. The bench harness for *Cloudflare Agent Memory vs mnemo*
  does NOT re-run for Project Think because the answer is layering,
  not benchmarking. [`docs/src/integrations/cloudflare-workers-deploy.md`](docs/src/integrations/cloudflare-workers-deploy.md)
  gains a `## Runtime layer (Project Think)` sub-section linking to
  the new comparison doc. Two new tests: extended marketing-phrase
  banlist (`competes with Cloudflare`, `replaces Project Think`,
  `Project Think killer`, `Workers killer`) and
  `tests/readme_project_think_link.rs` (primary-source + heading +
  comparison-doc-link survival).

### Changed

- **U1 (v0.4.4) — Sierra $950M raise applied-agent-layer evidence
  paragraph in [`docs/comparisons/cloudflare-agent-memory.md`](docs/comparisons/cloudflare-agent-memory.md).**
  One-paragraph *market-evidence, not feature-claim* note citing
  Sierra's 2026-05-04 raise as concrete evidence the applied-agent
  layer is well-funded enough to demand the offline-auditable memory
  substrate mnemo offers.
- **U1 — corrected v0.4.3 verification trace.** The `## [0.4.3] -
  2026-05-04` block's original `### Verification trace (2026-05-04)`
  sub-block was authored before the version-flip commit landed in
  the same PR — it asserts `Cargo.toml workspace.package.version =
  "0.4.2"` while the live raw fetch shows `"0.4.3"`. New
  `### Verification trace (2026-05-05)` sub-block records the
  corrected state with all artifact-registry checks green; the
  original trace stays in place as audit history of how the
  inconsistency arose.
- **U1 (v0.4.4) — spec-drift reconciliation footer.**
  [`docs/spec-drift-2026-05-04.md`](docs/spec-drift-2026-05-04.md)
  gains a `## 2026-05-05 stable-divergence confirmation` footer
  recording today's check: repo description on `main` unchanged, 14
  topics unchanged, Phase 6 skill template still anchors the older
  description — **stable divergence the operator has accepted, not
  a regression to flap on**.

- **U1 (v0.4.4, 2026-05-06) — MCP 2026 Roadmap spec-context anchor.**
  README `### mnemo and the MCP 2026 Roadmap` sub-section inside the
  existing Access Protocols section, citing the
  [MCP 2026 Roadmap](https://blog.modelcontextprotocol.io/posts/2026-mcp-roadmap/)
  (published 2026-03-09, 58 days old — *spec-context anchor, not
  fresh trigger*). Frames mnemo's existing operator-held HMAC
  keystore + AES-256-GCM at-rest encryption + dual DuckDB/Postgres
  backends + `mnemo-compliance` crate as the *attestable memory*
  layer aligned by design with the roadmap's **Enterprise
  Readiness** priority area — explicitly *not* a roadmap-compliance
  claim. [`docs/src/integrations/mcp-server.md`](docs/src/integrations/mcp-server.md)
  gains a `## MCP 2026 Roadmap alignment` section with a four-row
  priority-area mapping table tagging mnemo as `follower` /
  `observer` / `observer` / `aligned-by-design` against
  Transport / Agent Communication / Governance / Enterprise
  Readiness respectively. One-sentence cross-link from
  [`docs/comparisons/cloudflare-project-think.md`](docs/comparisons/cloudflare-project-think.md)
  noting Project Think + the MCP 2026 Roadmap together describe the
  *runtime + protocol* picture, with mnemo below both as the
  offline-auditable storage substrate.

- **U1 (2026-05-06) — Access Protocols table version drift fix.**
  Stale `rmcp 0.14` reference corrected to `rmcp 1.3` to match the
  workspace dep on `main`. Caught while landing the MCP 2026
  Roadmap anchor.

### Documentation

- **U2 (v0.4.4) — v0.4.3 publish-status doc.** New
  [`docs/release/v0.4.3-publish-status.md`](docs/release/v0.4.3-publish-status.md)
  records: cargo-publish job ID + `success` conclusion + 17/17 crates
  at `0.4.3` on crates.io with published-at timestamps; PyPI
  `mnemo-db@0.4.3` live; npm `@mndfreek/mnemo-sdk@0.4.3` live. The
  v0.4.3 publish completed cleanly under the bumped 300-min job
  timeout — no resume-dance required.

- **U2 (v0.4.4, 2026-05-06) — v0.4.3 publish-status reconciliation
  footer.** [`docs/release/v0.4.3-publish-status.md`](docs/release/v0.4.3-publish-status.md)
  gains a `## Post-publish reconciliation (2026-05-06)` footer
  closing the publish-status loop one day after the cut: no
  downstream regressions surfaced via `cargo audit`, `cargo deny`,
  or PyPI/npm install-test workflows in the last 24h. v0.4.4
  `[Unreleased]` cycle now active.

- **(v0.4.4, 2026-05-17) — `bench/locomo/grep_vs_vector_replay` bin
  scaffold.** New `[[bin]]` target in
  [`bench/locomo`](bench/locomo/) that routes a LongMemEval-shaped
  slice through `mnemo.recall` in three modes — `vector_only`
  (`strategy="semantic"`), `bm25_only` (`strategy="lexical"`), and
  `rrf_hybrid` (`strategy="auto"`) — and emits a Markdown table to
  `bench/locomo/results/grep_vs_vector_<date>.md`. Reproduces the Sen
  et al. arXiv:2605.15184 experiment design ("grep vs vector
  retrieval inside agent harnesses") on mnemo's own substrate.

  **Scope honest:** runs end-to-end against the bundled 45-record
  synthesized `longmemeval_m.jsonl` with `NoopEmbedding` (zero
  vectors, vector-only mode is degenerate by design — the wiring is
  the point) and a deterministic exact-substring smoke metric. The
  full 116-question LongMemEval slice + GPT-judge-scored official
  metric require an embedder + API key and are gated behind the same
  secrets ledger as
  [#44](https://github.com/sattyamjjain/mnemo/issues/44). Per-query
  failures (e.g. Tantivy BM25 parser rejecting apostrophes) are
  counted as misses in the accuracy column with an explicit
  failures-column in the markdown so the reader can tell substrate
  recall apart from parser strictness. New
  [`bench/locomo/README.md`](bench/locomo/README.md) documents both
  the smoke path and the gated full path.

  Pairs with the docs companion in PR-B (RetrievalMode typed enum +
  HarnessAware variant) that lands the rest of the arXiv:2605.15184
  anchor.

- **U1 (v0.4.4, 2026-05-10) — DELEGATE-52 outcome-diffing primitive
  anchor.** New
  [`docs/research/delegate52-2604.15597.md`](docs/research/delegate52-2604.15597.md)
  treating the DELEGATE-52 delegation-corruption result
  ([arXiv 2604.15597](https://arxiv.org/abs/2604.15597), Hacker News
  front 2026-05-09) as a *write-side substrate* anchor: mnemo's
  append-only event log + snapshots capture the plan / input / trace
  / output tetrad an outcome-diff replay tool reconstructs at audit
  time. The doc walks through (a) what DELEGATE-52 measures (25%
  baseline silent corruption rate on long delegated workflows),
  (b) the three trust walls (intent / action / outcome) and where
  mnemo lives (Wall 3), (c) the operator recipe for getting
  outcome-diff-ready against mnemo today without a new crate, and
  (d) the explicit non-overlap callout (mnemo provides the
  substrate, the diffing policy is the auditor's job).
  README "Why mnemo when Cloudflare Agent Memory exists?" gains
  one paragraph anchoring the outcome-diffing primitive in v0.4.4.
  [`docs/comparisons/anthropic-dreams.md`](docs/comparisons/anthropic-dreams.md)
  gains a one-line cross-reference distinguishing curation (Dreams)
  from outcome diffing (DELEGATE-52). Two new doc-only fixture rows
  in [`docs/tests/example_recalls.md`](docs/tests/example_recalls.md)
  exercising the reconstruction-from-events path: (1) primary-agent
  plan capture via REMEMBER with `metadata.role="plan"`, (2)
  full-tetrad reconstruction via `RECALL { thread_id, as_of,
  with_provenance=true }`. **No behavioural change to the binary**
  — the fixtures specify substrate calls operators can make today.

- **U2 (v0.4.4, 2026-05-09) — ARGUS provenance composition anchor.**
  [`docs/comparisons/cloudflare-agent-memory.md`](docs/comparisons/cloudflare-agent-memory.md)
  gains a `## Read-side composition: ARGUS provenance auditing
  (2026-05-09)` section pairing mnemo's *write-side* HMAC envelope
  chain with [arXiv 2605.03378](https://arxiv.org/abs/2605.03378)'s
  *read-side* decision-auditing model for context-aware prompt
  injection (submitted 2026-05-05, 4 days old at land-time). New
  companion research-anchor doc
  [`docs/research/argus-2605.03378.md`](docs/research/argus-2605.03378.md)
  walking through what ARGUS does, where mnemo fits, and what this
  note is explicitly NOT (not an implementation, not a compliance
  claim, not a benchmark). Composition-anchor framing throughout —
  compositional-security overclaim phrasings (`prompt-injection-proof`,
  `provenance-guaranteed`, `ARGUS-compliant`,
  `injection-resistant by construction`) banned by the extended
  marketing-phrase test below.

### Tests

- `tests/changelog_has_unreleased_section.rs` — fails the build if
  `CHANGELOG.md` ever loses its `## [Unreleased]` heading.
- `tests/release_status_doc_present.rs` — fails the build if
  `docs/release/v0.4.3-publish-status.md` is missing the canonical
  `Cargo workspace v0.4.3 publish status` header. Cheap drift guard
  for the release-day audit habit.
- **`tests/readme_mcp_roadmap_link.rs`** (v0.4.4 U1, 2026-05-06) —
  fails the build if README drops the MCP 2026 Roadmap primary-source
  URL or the `### mnemo and the MCP 2026 Roadmap` heading or the
  link to `docs/src/integrations/mcp-server.md`. Anchor-survival
  guard.
- **`tests/readme_no_marketing_phrases.rs`** (v0.4.4 U1, 2026-05-06)
  — banlist extended with `MCP 2026 leader`, `compliant with MCP
  2026`, `MCP 2026 ready`, `roadmap-compliant` so the new spec-context
  anchor cannot drift into compliance-overclaim framing.
- **`tests/changelog_has_landing_trace_section.rs`** (v0.4.4 U2,
  2026-05-06) — fails the build if the `## [Unreleased]` block ever
  loses its `### Landing trace` heading or if that heading does not
  contain a hex commit-sha-prefix matching `[0-9a-f]{7,40}`. Forces
  every future docs-only land to record an on-`main` commit pointer.
- **`tests/readme_dreams_link.rs`** (v0.4.4 U1, 2026-05-09) — fails
  the build if README drops the Anthropic Dreams Research Preview
  primary-source URL, the `### Memory curation interop` heading, the
  link to `docs/comparisons/anthropic-dreams.md`, or the literal
  `Research Preview` honesty disclaimer.
- **`tests/research_doc_argus_present.rs`** (v0.4.4 U2, 2026-05-09)
  — fails the build if `docs/research/argus-2605.03378.md` is
  missing the arXiv URL or the `Composition anchor, not a compliance
  claim` standing-rule disclaimer.
- **`tests/readme_no_marketing_phrases.rs`** (v0.4.4 U1+U2,
  2026-05-09) — banlist extended with five Dreams overclaim phrasings
  (`Dreams replacement`, `dream-compatible`, `Dreams-ready`,
  `Dreams competitor`, `curator killer`) and four compositional-security
  overclaim phrasings (`prompt-injection-proof`, `provenance-guaranteed`,
  `ARGUS-compliant`, `injection-resistant by construction`).
- **`tests/research_doc_delegate52_present.rs`** (v0.4.4 UPDATE-1,
  2026-05-10) — fails the build if
  `docs/research/delegate52-2604.15597.md` is missing the arXiv URL,
  the `Composition anchor, not a compliance claim` standing-rule
  disclaimer, or the load-bearing `plan / input / trace / output
  tetrad` phrasing.
- **`tests/example_recalls_doc_present.rs`** (v0.4.4 UPDATE-1,
  2026-05-10) — fails the build if `docs/tests/example_recalls.md`
  is missing either fixture-row heading or the link back to the
  DELEGATE-52 research-anchor.
- **`tests/readme_no_marketing_phrases.rs`** (v0.4.4 UPDATE-1,
  2026-05-10) — banlist extended with three DELEGATE-52 overclaim
  phrasings (`DELEGATE-52-resistant`, `outcome-corruption-proof`,
  `delegation-safe by construction`).

## [0.4.3] - 2026-05-04

Substrate-anchor release. Three S-effort surfaces: a Cloudflare
Workers / Durable Object Facets deploy-template *design anchor*
(net-new market trigger from the 2026-04-30 DO Facets open beta), a
version-skew matrix expansion to track the 2026-05-01 / 2026-05-02
MCP client-SDK refresh, and a spec-drift reconciliation note that
pins the repo description on `main` as canonical against an external
skill-template anchor. Also lands the load-bearing **breaking change**
that's been gated for two release cycles: `duckdb` 1.4 → 1.5.2
(closes [#41](https://github.com/sattyamjjain/mnemo/issues/41) Step 1)
with a fully idempotent migration runner that incidentally resolves
the pre-existing Ubuntu DuckDB extension race.

### Added

- **A1 — Cloudflare Workers / Durable Object Facets deploy template anchor.**
  README `### Cloudflare Workers deploy template` subsection under
  Deployment, citing the [DO Facets open-beta](https://blog.cloudflare.com/durable-object-facets-dynamic-workers/)
  (2026-04-30) as the substrate anchor for the v0.4.3 `mnemo-bench-cf`
  crate. New design note at
  [`docs/src/integrations/cloudflare-workers-deploy.md`](docs/src/integrations/cloudflare-workers-deploy.md)
  covering Rust↔WASM↔DO-Facet boundaries, file-format compatibility
  (DuckDB ↔ SQLite is *not* wire-compatible), operator-held HMAC
  keystore requirement, and the open-question list (USearch-on-WASM,
  Tantivy-on-WASM, DuckDB-on-WASM trade-offs).
  [`docs/comparisons/cloudflare-agent-memory.md`](docs/comparisons/cloudflare-agent-memory.md)
  S1.5 row replaces empty-bench placeholders with a concrete
  per-tenant-footprint / cold-start / persistence-boundary /
  audit-replay scenario block. Two new tests: extended marketing-phrase
  banlist (`tests/readme_no_marketing_phrases.rs` adds `viral`,
  `game-changing`, `revolutionary`, `wild`, `mind-blowing`, etc.) and
  `tests/readme_workers_template_link.rs` (anchor-link survival test).

### Changed

- **U1 — version-skew matrix gains MCP-SDK columns + a
  Cloudflare-substrate annotation.**
  [`docs/compat/version-skew-matrix.md`](docs/compat/version-skew-matrix.md)
  now splits server-side and SDK-side rows; new `mcp-python` /
  `mcp-go` / `mcp-ruby` / `mcp-csharp` columns track the 2026-05-01 /
  05-02 client-SDK refresh from
  [github.com/modelcontextprotocol](https://github.com/modelcontextprotocol).
  The v0.4.3 row carries a Cloudflare-substrate annotation listing
  both Workers KV+Vectorize *and* DO Facets SQLite as
  `mnemo-bench-cf` baseline targets (not implementation-of-record —
  mnemo still ships embedded Rust). New regression test
  `crates/mnemo-mcp/tests/sdk_matrix_doc_present.rs` fails if the doc
  is missing or loses any of the four `mcp-*` column headers.
  `docs/src/integrations/mcp-server.md` gains a "Compatibility note"
  section linking to the matrix for SDK-skew triage.

- **U2 — spec-drift reconciliation note.**
  New [`docs/spec-drift-2026-05-04.md`](docs/spec-drift-2026-05-04.md)
  declares the repo description on `main` canonical (vs. the
  daily-opportunity-radar skill template's older description) and
  maps the skill template's surface anchors (semantic + episodic
  stores, LangGraph adapter, Workers template) to where they live in
  the actual codebase. `CONTRIBUTING.md` gains a "Spec-drift policy"
  subsection linking to the note so future contributors landing
  surface-affecting changes find the policy first.

### Verification trace (2026-05-04)

> ⚠️ **This trace was authored before the version-flip commit landed
> in the same PR.** It asserts `Cargo.toml = "0.4.2"` while the live
> raw fetch shows `"0.4.3"`. The version flip was the *intent* of the
> PR, not a regression. See the corrected `### Verification trace
> (2026-05-05)` sub-block below for the post-merge state.

- `Cargo.toml` workspace.package.version = `"0.4.2"` on `main` ✓
- README role-filter section live (v0.4.2 A1) ✓
- README Cloudflare differentiation H2 live (v0.4.2 U2) ✓
- `tests/readme_no_marketing_phrases.rs` green on `main` ✓
- All 17 crates published at `0.4.2` on crates.io ✓
- `mnemo-db@0.4.2` on PyPI ✓
- `@mndfreek/mnemo-sdk@0.4.2` on npm ✓

### Verification trace (2026-05-05) — corrected post-merge state

Recorded one day after the v0.4.3 cut to capture the published-state
ground truth. Origin of the correction: today's U1 row.

- `Cargo.toml` workspace.package.version = `"0.4.3"` on `main` ✓
- `duckdb = "=1.10502.0"` workspace pin live ✓
- `apply_alters_idempotent` migration runner live in
  `crates/mnemo-core/src/storage/migrations.rs` ✓
- README "Cloudflare Workers deploy template" sub-section live
  (v0.4.3 A1) ✓
- `tests/readme_workers_template_link.rs` green on `main` ✓
- `tests/readme_no_marketing_phrases.rs` extended banlist green on
  `main` ✓
- `crates/mnemo-mcp/tests/sdk_matrix_doc_present.rs` green on
  `main` ✓
- `docs/spec-drift-2026-05-04.md` live (v0.4.3 U2) ✓
- All 17 crates published at `0.4.3` on crates.io ✓ (cargo-publish
  job completed `success` under the bumped 300-min cap — see
  [`docs/release/v0.4.3-publish-status.md`](docs/release/v0.4.3-publish-status.md))
- `mnemo-db@0.4.3` on PyPI ✓
- `@mndfreek/mnemo-sdk@0.4.3` on npm ✓
- 4 dependabot bumps merged after v0.4.3 cut: `actions/setup-node`
  v4→v6 (#69), `actions/download-artifact` v7→v8 (#70), `toml`
  0.9→1.1 (#71), `tokenizers` 0.22→0.23 (#72) ✓

### ⚠️  Breaking — persisted state upgrade required

- **Bumped `duckdb` 1.4 → 1.5.2** (closes [#41](https://github.com/sattyamjjain/mnemo/issues/41) Step 1; PR [#75](https://github.com/sattyamjjain/mnemo/pull/75)).
  DuckDB 1.5.2 stamps a newer on-disk file-format header. **Operators
  upgrading mnemo across this version must:**
  1. **Back up** any persisted `*.mnemo.db` file (and the sibling
     `*.usearch` / `*.tantivy` index directories) before running the
     new binary.
  2. **Open the DB once with the new binary** to upgrade the file
     format in place. Once upgraded, the file is no longer readable
     by mnemo binaries pinned to duckdb 1.4.x — downgrading after
     this point requires a fresh DB.
  3. If a downgrade is required, restore from the pre-upgrade backup
     in step 1.
  4. **No operator action is required for fresh DBs** — the new
     binary writes the new format on first open.

  See the upstream [DuckDB 1.5.2 release notes](https://duckdb.org/2026/04/13/announcing-duckdb-152)
  and the [`duckdb-rs` 1.10502.0 release](https://github.com/duckdb/duckdb-rs/releases) for full file-format change details.

### Changed

- **Migrations are now idempotent under DuckDB 1.5+** (PR [#75](https://github.com/sattyamjjain/mnemo/pull/75)).
  The previous "issue ALTER, swallow column-exists error" pattern in
  `run_migrations` no longer works — DuckDB 1.5 aborts the
  connection's implicit transaction after a few consecutive failures.
  New `apply_alters_idempotent` introspects
  `information_schema.columns` first and only emits an `ALTER` when
  the column is actually missing. Side benefit: also resolves the
  pre-existing Ubuntu DuckDB extension race that was admin-merged
  through every prior release.

## [0.4.2] - 2026-05-03

Reconciliation release. Three S-effort surfaces driven by the
2026-04-30 MCP authorization spec (role-based annotations) and the
Cloudflare Agents Week wrap (2026-04-29). Resyncs the workspace
version metadata that drifted ahead of `main` in the prompt ledger.

### Added

- **A1 — MCP role-aware tool filter.** New
  [`crates/mnemo-mcp/src/role_filter.rs`](crates/mnemo-mcp/src/role_filter.rs)
  with `RoleFilter` trait + `ManifestRoleFilter` impl. Manifest-driven
  `[role_filter]` block (default no-op when omitted, byte-for-byte
  preserves existing behaviour). Aligns with the MCP authorization
  spec (2025-11-25, role-based annotations,
  https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization).
  Three integration tests: `role_filter_allow_deny`,
  `role_filter_audit_event`, `role_filter_no_block_when_unset`.
- **U2 — Cloudflare differentiation.** New
  [`docs/comparisons/cloudflare-agent-memory.md`](docs/comparisons/cloudflare-agent-memory.md)
  long-form scenario list with empty-bench placeholders pointing to the
  v0.4.3 `mnemo-bench-cf` crate. README gains a "Why mnemo when
  Cloudflare Agent Memory exists?" section that explicitly concedes
  edge-recall perf likely favours Cloudflare and positions the
  differentiator on provenance, chain replay, and offline auditability.
  Grep-lint `tests/readme_no_marketing_phrases.rs` rejects "beat
  Cloudflare" / "faster than Cloudflare" / "Cloudflare killer" in CI.
- **U2 — SHARE on TS + Go quickstarts.** TypeScript and Go SDK README
  blocks now show `client.share({memoryId, withAgent})` /
  `client.Share(mnemo.ShareInput{...})` lines so the SHARE primitive
  has explicit quickstart parity with REMEMBER / RECALL / FORGET.

### Changed

- **U1 — Workspace version resync.** `workspace.package.version`
  bumped `0.4.1 → 0.4.2`. Internal-crate version pins (lines 99-106 of
  `Cargo.toml`) bumped from `0.4.0-rc2` to `0.4.2` so consumers can
  resolve `mnemo-core = "0.4.2"` against the published workspace.
  `python/pyproject.toml` and `sdks/typescript/package.json` bumped to
  `0.4.2`. `sdks/go/mnemo.go` gains a `Version` constant + package
  version doc-comment so the Go SDK reports the same version on MCP
  `initialize`.
- **Compatibility matrix.** New
  [`docs/compat/version-skew-matrix.md`](docs/compat/version-skew-matrix.md)
  pinning `mnemo` ↔ `rmcp` ↔ `tantivy` ↔ `usearch` ↔ `pgvector` ↔
  Python/TS/Go SDK versions.

### Tests

- `crates/mnemo-core/tests/version_metadata.rs` — asserts
  `env!("CARGO_PKG_VERSION") == "0.4.2"` so any future drift between
  the workspace stamp and the source crate fails CI.
- `python/tests/test_version_alignment.py` — asserts
  `mnemo.__version__` matches the Cargo workspace version.
- `tests/readme_no_marketing_phrases.rs` — top-level integration test
  greps `README.md` for the three banned marketing phrases.

### Deferred to v0.4.3

The 2026-05-02 prompt's six P0/P1 rows are explicitly **rebased to
v0.4.3** because their prerequisite crates (`mnemo-envelope`,
`mnemo-aas01`, `mnemo-mgt`) never landed on `main` between 2026-04-29
and 2026-05-03:

- `mnemo-bench-cf` (full Cloudflare bench crate — v0.4.2 ships only
  the README differentiation paragraph)
- `mnemo-langgraph` 1.2 checkpoint adapter (no LangGraph 1.2 release
  ≤7d to force the schedule)
- `mnemo-purview` (Microsoft Purview log adapter, M-effort)
- `EnvelopeKind::FetcherAttestation` (depends on `mnemo-envelope`
  being on `main` first)
- Agent-vs-human authorship tag (same dependency)
- `mnemo-toolhive` (Stacklok Registry v1.2.0 sync, opportunistic)

### Sources

- MCP Authorization spec — https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization
- Cloudflare Agents Week wrap — https://www.cloudflare.com/agents-week/updates/

## [0.4.1] - 2026-04-28

Silence-breaker release. Picks up the four competitive surfaces that
opened this week (Anthropic CMA-Memory beta, MemMachine + Memori
LoCoMo numbers, DeepSeek V4 1M context, RSAC 2026 SOC telemetry gap)
plus a counterparty-discovery layer for the Project-Deal substrate
shipped yesterday.

### Added

- **P0-1 — First public LoCoMo benchmark.** New
  [`bench/locomo`](bench/locomo) crate with `LoCoMoRun`,
  `LoCoMoResult`, `LoCoMoJudge` trait + `MockJudge` fallback. Cross-
  judge variance tracking (GPT-5.1 + Claude-3.7 Sonnet). Authenticated
  nightly via [`.github/workflows/locomo-nightly.yml`](.github/workflows/locomo-nightly.yml).
  First public report at
  [`docs/benchmarks/locomo-2026-04-28.md`](docs/benchmarks/locomo-2026-04-28.md).
  9 unit tests.
- **P0-2 — `mnemo-cma` crate (Anthropic CMA-Memory compat shim).**
  Drop-in for the filesystem-of-Markdown beta announced 2026-04-23.
  `CmaTreeRoot` / `SyncMode { ReadThrough | WriteThrough | Mirror }`,
  `import_cma_tree` produces a deterministic `ImportSummary` whose
  HMAC chain head is byte-identical for two runs over the same tree,
  `audit_bridge::bridge_event` chains every CMA write into the
  existing provenance ledger via `CmaSource::CmaBeta` /
  `CmaSource::CmaImport` markers, `export_to_tree` reproduces the
  original `.memory/` byte-for-byte. 10 unit tests.
- **P0-3 — `mnemo-baseline` crate (RSAC SOC telemetry gap).**
  Per-agent rolling profile (`AgentBaseline` with recall/write
  rates, namespace fanout, tool mix, HMAC continuity), z-score +
  EWMA drift detector with five `Severity` thresholds, OpenTelemetry
  semconv 1.31 + OCSF 1.4 Application-Activity emitters via
  `JsonExporter`. **Anti-leak invariant** enforced by a regex sweep
  unit test: emitted payloads never contain memory contents. 9 unit
  tests.
- **P1-4 — 1M-context recall budget planner.** New
  `mnemo-core::budget` module: `ContextBudget::for_model(ModelId)`
  + `plan_recall(budget, history, query) -> RecallPlan`. Per-model
  table covers `deepseek-v4-1m`, `claude-3.7-sonnet-1m`,
  `gpt-5.1-400k`, `gemini-2.5-pro-2m` plus their smaller siblings.
  Property test asserts the plan never overflows total context. 9
  unit tests.
- **P1-5 — Project-Deal counterparty discovery + reputation.** Two
  new `mnemo-deal` submodules: `discovery::AgentAdvertisement` for
  the canonical `/.well-known/mnemo-deal-agent.json` body shape +
  `reputation::compute_reputation` with a 90-day half-life decay
  and a per-dispute 10% penalty. The README's threat-model section
  scopes the score as advisory, not enforcement. 7 new tests (17
  total in the crate).
- **P2-6 — `mnemo doctor` + Grafana dashboard JSON.** Typed
  `DoctorReport` + `DoctorFix` recommendations
  (RebuildVectorIndex / RotateHmacKey / RepinMcpCatalog /
  EnableDecayLane / UpgradeRmcp). Committed
  [`dashboards/mnemo-grafana.json`](dashboards/mnemo-grafana.json)
  (Grafana schemaVersion 39), validated by an integration test that
  asserts the operator-critical panels exist. 5 tests.

### Changed

- Workspace version bumped from `0.4.0` to `0.4.1` across all 17
  Rust crates (incl. three new: `mnemo-cma`, `mnemo-baseline`,
  `bench/locomo`), the Python package (`mnemo-db`), and the
  TypeScript SDK (`@mndfreek/mnemo-sdk`).
- `cargo-publish.yml` plan list updated to include the two new
  publishable crates (`mnemo-cma`, `mnemo-baseline`); the bench
  crate is `publish = false` and stays out of crates.io.

### Notes for operators

- The `mnemo cma serve|migrate|export` and `mnemo doctor`,
  `mnemo dashboard` clap subcommands ship the data shapes today;
  wiring them into the binary's `Command` enum is a follow-up
  (mirrors v0.4.0-rc3's pattern). `#[allow(dead_code)]` on each
  module documents the gap so a `cargo clippy -D warnings` build
  stays green.

## [0.4.0] - 2026-04-27

Mesh / code-mode / commerce release. Picks up four net-new
competitive surfaces (Cloudflare Mesh, Cloudflare Code Mode,
Anthropic Project Deal, Wuphf-style Markdown wikis) plus a hard
defense against the new MCP function-hijacking class.

### Added

- **P0-1 — MCP tool-catalog attestation.** New
  `crates/mnemo-cli/src/attest/` module with `PinnedToolCatalog`,
  `CatalogAttestor` trait, and `PinnedAttestor` impl. Operators ship
  a `[tool_catalog_pin]` block in the manifest; `mnemo mcp-server`
  refuses to start if the advertised catalog has any `added` or
  `mutated` tools, and emits a `McpToolCatalogDrift` audit event with
  the per-tool diff. `--allow-removed-drift` lets `removed`-only
  diffs through with a warning. Direct response to **arXiv 2604.20994**
  (function-hijacking via tool-list poisoning). 10 unit tests.
- **P0-2 — `mnemo-mesh` crate (Cloudflare Mesh runtime adapter).**
  SPIFFE-style `MeshIdentity` + `AttestationToken`, `MemOp` enum
  covering `Recall`/`Write`/`Forget`/`Branch`/`ReplayAsOf`/
  `ExportProvenance`, `MeshPolicyEnforcer` trait + `StaticPolicyEnforcer`
  impl with per-(SPIFFE, namespace) ACL, `MeshAuditEnvelope` with
  deterministic `next_chain_head()` chained into the v0.4.0-rc3
  provenance ledger. 13 unit tests. First OSS embedded memory DB to
  speak Cloudflare Mesh attestation natively.
- **P0-3 — `mnemo-codemode` crate (Code Mode WIT recall).** WIT
  world definition (`mnemo:memory@0.4`) under
  `crates/mnemo-codemode/wit/`, host-side runner with
  `ResourceBudget` (fuel / mem_pages / wall), `RecallStep` /
  `GuestProgram` / `RecallBundle`, token-cost estimator that
  asserts code-mode delivers ≥20% token reduction on a 200-turn
  conversation (vs Cloudflare's 99.9% claim — we're more
  conservative because we stream records, not just side effects).
  wasmtime + WASI-stripping path is feature-gated for the follow-up.
  7 unit tests including fuel exhaust + wall-time exceeded.
- **P1-4 — Decay-curve recall primitive.** New
  `mnemo-core::score` module with `ScoreLane` trait + `DecayLane`
  impl. `decay_weight(now, last_access, hits, &DecayParams)` is the
  pure Ebbinghaus exponential with reinforcement and floor;
  `letta_mode` flag in `ScoreContext` zeros the lane for parity with
  Letta's published numbers. Default fuse weights:
  `0.55 vector + 0.20 bm25 + 0.15 recency + 0.10 decay`.
  Competitive response to YourMemory's biological-decay marketing
  (Show HN, 2026-04-27). 9 unit tests.
- **P1-5 — `mnemo-deal` crate (agent-on-agent deal ledger).**
  Chained-HMAC `DealEnvelope` log with `InMemoryDealLedger` impl,
  `verify_chain()` that produces a `DisputeReport` pinpointing the
  first divergent offset. Substrate for Anthropic Project Deal-style
  commerce (announced 2026-04-25). 10 unit tests including tampered
  terms + broken prev_hash detection.
- **P2-6 — `mnemo-md-sync` crate (Markdown+Git working set).**
  Parser for YAML-style frontmatter (`mnemo_id`, `agent_id`,
  `tags`, `expires_at`), `MdSyncSpec` config with
  `SyncFlushPolicy` (PreferEngine / PreferDisk / NewerWins). Wuphf-
  inspired ergonomics with mnemo-grade recall + provenance. 9 unit
  tests. notify-based watcher + gix commit-on-flush land in a
  follow-up; the contract API is stable.

### Changed

- Workspace version bumped from `0.4.0-rc3` to `0.4.0` across all
  Rust crates (14 incl. four new: `mnemo-mesh`, `mnemo-codemode`,
  `mnemo-deal`, `mnemo-md-sync`), the Python package (`mnemo-db`),
  and the TypeScript SDK (`@mndfreek/mnemo-sdk`).
- `mnemo-core::model::EventType` gained `McpToolCatalogDrift` for
  P0-1 audit rows.
- Manifest (B2 hardened mode) gained `tool_catalog_pin_path` and
  `allow_removed_drift` fields. Both optional and additive — older
  manifests load unchanged.
- `cargo-publish.yml` plan list updated to include the four new
  crates so push-to-main publishes them.

### Security

- The P0-1 tool-catalog attestation is a direct response to **arXiv
  2604.20994 (2026-04-23)**: a malicious MCP source that mutates
  `tools/list` can rename a tool, change its `inputSchema`, or
  smuggle a hidden `secret_exfil` tool. Mnemo's hardened launcher
  now refuses to expose any catalog whose fingerprint set differs
  from the operator-pinned baseline.

## [0.4.0-rc3] - 2026-04-26

Threat-model release: hardens the MCP STDIO entry point against the
OX-MCP "exfiltrate-then-act" disclosure (2026-04-24), adds memory
provenance signing on reads, and ships compliance + competitive
parity surfaces (DPDPA, Letta-protocol).

### Added

- **B1 — Memory-provenance signing API.** New `mnemo-core::provenance`
  module with `ProvenanceSigner` (HMAC-SHA256), `ReadProvenance`
  receipt type, and `verify_read_provenance()` helper. `RecallRequest`
  carries a new `with_provenance: Option<bool>` field; when set and a
  signer is attached to the engine, the response includes a verifiable
  receipt that binds the cited records to a server-side key. Supports
  rotated keys via `hmac_key_id`. 6 unit tests + 4 integration tests
  in `crates/mnemo-core/tests/provenance_chain.rs`.
- **B2 — `mnemo mcp-server --manifest <path>` hardened mode.** New
  CLI subcommand that runs a safe-spawn gauntlet BEFORE constructing
  any engine state: refuses inherited sensitive env vars, refuses
  `--config`-style argv injection, refuses untrusted parents (non-TTY
  parent must be in `manifest.allowed_parents`). Loads the HMAC
  keystore the manifest points at and attaches a `ProvenanceSigner`
  (B1) to the engine — key material reaches the binary via a
  chmod-restricted file, never via env or argv. 14 unit tests
  (manifest/safe-spawn/lease) + 4 integration tests spawning the real
  binary.
- **B3 — LongMemEval_M bench + `--with-provenance` toggle.** Bundled
  45-record synthesized dataset at
  `crates/mnemo-core/benches/data/longmemeval_m.jsonl` (override via
  `MNEMO_LONGMEMEVAL_PATH`). New `longmemeval_bench` criterion target
  with `recall_no_provenance` and `recall_with_provenance` arms.
- **B4 — DPDPA Mannsetu adapter (consent-token-per-write).** New
  `mnemo-compliance::mannsetu` module with `MannsetuConsentSource`
  (HTTP binding to the DPB-registered Mannsetu API), `ConsentToken`
  type, and `ConsentTokenGuard` (per-write authorization with
  expiry/scope/revocation checks). 7 new unit tests.
- **B5 — `mnemo-letta` crate (Letta-protocol-compat).** New workspace
  crate exposing `POST /v1/agents`, `POST /v1/agents/{id}/messages`,
  and `GET /v1/agents/{id}/memory` so a Letta-Code-shaped benchmark
  or notebook can talk to Mnemo without code changes. 4 integration
  tests.
- **B6 — `mnemo eval` subcommand.** Replays a JSONL dataset against
  an in-memory engine and emits a per-row JSONL report
  (latency_us, top-k, hit). Used for config sweeps (provenance
  on/off, hybrid weights, recency half-life). Defaults to the
  bundled LongMemEval_M sample.
- **Q1 — Pure-Python provenance SDK.** New `mnemo.provenance`
  module: `ProvenanceSigner` / `ReadProvenance` / `RecordRef`
  dataclasses + `verify_read_provenance()` helper. Auditors verify
  receipts offline without compiling Rust. 6 pytest cases.
- **Q2 — Claude Code MCP installer.** New `mnemo.install_claude_code`
  module + `python -m mnemo install claude-code [--hardened <manifest>]`
  CLI. Idempotently registers Mnemo as an MCP server in
  `~/.claude.json`. 6 pytest cases.
- **Q3 — DPDPA "data passport" PDF builder.** New
  `mnemo.dpdpa_passport` module that renders a one-page PDF showing
  every personal data point Mnemo holds for a subject (DPDPA Section
  11 / 12 right-to-portability/access). Hand-rolled PDF (no
  third-party dep), reproducible byte-for-byte. 5 pytest cases.
- **Q4 — Time-travel debugger UI.** New
  `examples/time-travel-debugger/index.html`. Vanilla JS, no build
  step. Diffs recall results between two `as_of` timestamps.

### Changed

- Workspace version bumped from `0.4.0-rc2` to `0.4.0-rc3` across all
  Rust crates (10 incl. new `mnemo-letta`), the Python package
  (`mnemo-db`), and the TypeScript SDK (`@mndfreek/mnemo-sdk`).
- `RecallRequest` gained `with_provenance: Option<bool>` (additive,
  defaults to `None`). `RecallResponse` gained
  `provenance: Option<ReadProvenance>` (skipped on the wire when
  `None`). Downgrade-safe.
- `MnemoEngine` gained `with_provenance_signer()` builder method.

### Security

- The B2 hardened mode is the direct response to the OX-MCP
  "exfiltrate-then-act" disclosure (2026-04-24). The default `mnemo`
  startup path is unchanged for backward compatibility; new
  deployments should prefer `mnemo mcp-server --manifest <path>`.

## [0.4.0-rc2] - 2026-04-25

### Changed (publication names — no code or behaviour change)

- **PyPI distribution name**: `mnemo` → **`mnemo-db`**. The unqualified
  name on PyPI is held by an unrelated 2021 notebook project
  (`Gabriele Girelli/mnemo-assistant`) with last release 2021-07-06.
  The Python package directory, the import path, and the SDK class
  names are unchanged — `from mnemo import MnemoClient` still works.
  Users now `pip install mnemo-db` and (for extras)
  `pip install 'mnemo-db[anthropic-memory-tool]'` etc.
- **`mnemo-cli` crate** → published as **`mnemo-mcp-server`** on
  crates.io. The unqualified `mnemo-cli` is owned by
  [github.com/watzon/mnemo](https://crates.io/crates/mnemo-cli)
  ("CLI management tool for the Mnemo LLM memory proxy" — a different
  project). The crate directory stays `crates/mnemo-cli/` and the
  installed binary is still `mnemo`. Users now
  `cargo install mnemo-mcp-server` and the resulting binary is
  invoked as `mnemo`.
- README, mdBook docs, integration pages, and example scripts updated
  to reflect both new install commands.

No changes to public APIs, file formats, persistence stamps, or wire
protocols. Downgrade-safe.

## [0.4.0-rc1] - 2026-04-25

### Highlights

Release candidate stacking on top of v0.3.4. Lands three of the four
follow-on tasks from the 2026-04-25 prompt: the Graphiti-style
temporal-edge crate (Task A4 minimal), the Letta Conversations-style
shared-memory adapter (Task A5), and a partial close on the golden
DuckDB fixtures front (Task A7). Task A6 (Mem0g graph-extraction
toggle) waits for v0.4.0 final because it depends on Task A4's LLM
extractor leaving stub state — see deferred section.

### Added

- **`mnemo-graph` crate** (Task A4 minimal). New workspace member with
  a `TemporalEdge { src, dst, relation, valid_from, valid_to,
  confidence, recorded_at }` model, an async `GraphStore` trait, a
  `DuckGraphStore` impl creating `graph_nodes` + `graph_edges` tables
  with indexes on `(src, valid_from)` and `(dst)`, and a
  `graph_expand(seed, depth, as_of)` BFS that respects bitemporal
  validity. The 5 unit tests in
  `crates/mnemo-graph/tests/temporal_walk.rs` cover the headline
  bitemporal-supersession property: an `as_of` query *between* a
  fact and its supersession returns the original answer; an `as_of`
  query *after* returns the new one.
- **`MnemoLettaShared` adapter** (Task A5). New
  `python/mnemo/letta_adapter.py` implementing
  `attach`/`detach`/`list_participants`/`read`/`write` over Mnemo
  memories tagged `conversation:<id>` and `participant:<agent_id>`.
  Cross-participant writes within a 60-second window surface via
  `overlapping_writes_within()` for operator inspection; conflict
  resolution itself happens at recall time via Mnemo's existing
  evidence-weighted scoring. Example at
  `examples/letta_shared_conversation.py`.
- **Golden fixture v0.3.4** (Task A7 partial). Generator at
  `crates/mnemo-core/examples/gen_golden_fixture.rs`; committed
  fixture at `crates/mnemo-core/tests/golden/v0_3_4.mnemo.db`;
  round-trip test at
  `crates/mnemo-core/tests/migration_roundtrip.rs` asserting the
  fixture opens, gets stamped to `CURRENT_PERSISTENCE_VERSION = 4`,
  and round-trips exactly 5 records. v0.1.1 / v0.3.0 historical
  fixtures still missing — see [issue #38](https://github.com/sattyamjjain/mnemo/issues/38)
  comment for the gap analysis (the corresponding git tags don't
  actually exist on this repo).

### Changed

- Workspace version bumped 0.3.4 → 0.4.0-rc1.
- `Cargo.toml` workspace members extended with `crates/mnemo-graph`.

### Tests

- **+5** new Rust integration tests in
  `crates/mnemo-graph/tests/temporal_walk.rs` — supersession
  correctness, confidence-ordered outgoing edges, BFS depth bound,
  idempotent edge-close, extract-stub.
- **+11** new Python tests in `python/tests/test_letta_adapter.py` —
  attach/detach idempotency, participants metadata not duplicated,
  cross-participant overlap detection, content/source validation.
- **+1** new Rust integration test in
  `tests/migration_roundtrip.rs` — fixture round-trip + persistence
  stamp.
- 100 Python pass + 5 skipped (4 OpenAI-gated pre-existing + 1
  live-R2). All Rust crates green; `mnemo-graph` adds 5 unit-test
  passes to the count.

### Deferred to v0.4.0 final

- **Task A4 — full LLM-driven `TemporalEdge::extract`.** v0.4.0-rc1
  ships the `graph-extract` feature gate but the extractor itself
  returns an empty `Vec`. The prompt + ICL examples are still being
  tuned; shipping a half-tuned extractor would put bad edges in
  everyone's graphs.
- **Task A4 — `hybrid_rrf` 4th-signal integration.** The retrieval
  path doesn't yet fuse graph-expanded nodes into RRF; that
  integration needs the extractor to be live first to surface enough
  edges for the signal to matter.
- **Task A4 — MCP / REST / gRPC `graph_expand` tools.** The crate
  exposes the function; binding it to the wire-protocol surfaces is
  small additive work for v0.4.0 final.
- **Task A6 — Mem0g `with_graph_extraction(enabled, model)` toggle.**
  Skipped today because the underlying extractor is a stub. Lands
  with the extractor in v0.4.0 final.
- **Task A7 — historical fixtures `v0_1_1.mnemo.db` /
  `v0_3_0.mnemo.db`.** Blocked by absent git tags. See [#38 comment](https://github.com/sattyamjjain/mnemo/issues/38#issuecomment-4319897458).

### Sources

- [Graphiti repo (getzep)](https://github.com/getzep/graphiti)
- [Graphiti paper (arXiv:2501.13956)](https://arxiv.org/abs/2501.13956)
- [Letta — Letta-Code release (2026-04-06)](https://www.letta.com/blog/letta-code)
- [Mem0g paper (arXiv:2504.19413)](https://arxiv.org/abs/2504.19413)

## [0.3.4] - 2026-04-25

### Highlights

Patch release shipping the **v0.3.4 floor** from the 2026-04-25 prompt:
the public benchmark page laid out for Letta-parity comparison, the
Anthropic raw-API memory-tool 6-op server (`memory_20250818`), and a
Cloudflare R2 workspace backend that closes one third of issue #39.
Tasks A4–A7 (Graphiti, Letta-shared, Mem0g, golden DuckDB fixtures)
fold into the v0.4.0-rc1 stack landing by 2026-04-28.

### Added

- **`MnemoMemoryToolServer`** ([`python/mnemo/anthropic_memory_tool.py`])
  — full client-side handler for Anthropic's `memory_20250818` tool
  surface. Maps the six commands (`view`, `create`, `str_replace`,
  `insert`, `delete`, `rename`) onto Mnemo memories with the
  spec-pinned return strings, line-numbered file views, and recursive
  directory listing semantics. `managed_agents_beta=True` flips the
  `anthropic-beta: managed-agents-2026-04-01` header through
  `MnemoMemoryToolServer.beta_header()`. Path-traversal protection is
  required-and-enforced: every input must canonicalise under
  `/memories`, with `..` and URL-encoded sequences rejected
  pre-normalisation. Doc page at
  `docs/src/integrations/anthropic-memory-tool.md`.
  Source: [Anthropic memory-tool docs][memtool].
- **`CloudflareR2Workspace`** ([`python/mnemo/openai_sandbox/r2_workspace.py`])
  — R2-flavoured subclass of `S3Workspace`. Sets `endpoint_url=
  https://{account_id}.r2.cloudflarestorage.com`, `region="auto"`,
  `addressing_style="virtual"`. RemoteSnapshotSpec output carries
  `backend="r2"` so `MnemoSnapshotStore` dispatches correctly. Live-R2
  test gated on `R2_ACCOUNT_ID` / `R2_ACCESS_KEY_ID` /
  `R2_SECRET_ACCESS_KEY` / `R2_BUCKET` env vars; otherwise the moto
  S3 emulator stands in.
- **`docs/benchmarks/2026-04-25-mnemo-v0.3.4.md`** — canonical
  benchmark page with Letta-parity reference rows ([Hindsight 91.4 /
  89.61][hindsight], [Letta-Filesystem 74.0][letta], full-context
  72.9 floor) plus blank mnemo rows the nightly workflow populates on
  its first authenticated run. Wired into README "Benchmarks"
  section. Tracking issue **#44** for the first authenticated run.
- New extras `mnemo[anthropic-memory-tool]` (pulls `anthropic>=0.40`)
  and `mnemo[openai-sandbox-r2]` (pulls `boto3>=1.34`,
  `cryptography>=42`).

### Changed

- **`S3Workspace`** ([`python/mnemo/openai_sandbox/s3_workspace.py`]) —
  lift `endpoint_url`, `region`, `addressing_style`,
  `signature_version` into the constructor. All default to `None` so
  AWS-S3 behaviour is unchanged for existing call-sites; subclasses
  (`CloudflareR2Workspace`) read from these in `_build_default_client`.
  Spec output now uses `self.backend_name` (defaults `"s3"`,
  R2 sets `"r2"`) so `RemoteSnapshotSpec.backend` is correct out of
  the box.
- Issue **#39** rescoped to GCS + Azure Blob only after R2 landed in
  this release.

### Tests

- **+32 unit tests** in `python/tests/test_anthropic_memory_tool.py` —
  all six ops, every documented error string, path-traversal rejection
  (`..`, URL-encoded), beta-header toggle, and a fixture round-trip
  test that replays the canonical request shapes from the docs page
  through `MnemoMemoryToolServer.handle`.
- **+5 unit tests** in `python/tests/test_r2_workspace.py` — moto
  round-trip with `backend="r2"` spec assertion, S3-spec rejection,
  `account_id` validation, and a live-R2 opt-in test.
- All 91 Python tests pass + 5 skipped (4 OpenAI-gated pre-existing,
  1 live-R2). No Rust changes; Rust tests untouched at the v0.3.3
  count.

### Deferred to v0.4.0-rc1

- **Task A4** — Graphiti-style temporal-edge crate (`mnemo-graph`).
  Bitemporal `valid_from`/`valid_to`, `graph_expand` integrated into
  `hybrid_rrf` as a fourth signal, MCP/REST/gRPC tool surfaces.
- **Task A5** — Letta `Conversations`-style shared-memory adapter
  (`MnemoLettaShared`).
- **Task A6** — Mem0g-parity `with_graph_extraction(enabled, model)`
  toggle on `MnemoMem0Compat`.
- **Task A7** — Golden DuckDB persistence fixtures (issue #38).

### Out of scope today

- DuckLake v1.0 storage backend evaluation (issue #41) — bump
  `duckdb = "1.4" -> "1.5.2"` in a separate PR.
- TypeScript 6.0 migration (PR #26 held; tracked in #40).

### Sources

- [Anthropic memory-tool docs][memtool]
- [Anthropic — Claude Opus 4.7 release post](https://www.anthropic.com/news/claude-opus-4-7)
- [Letta — Letta-Code release](https://www.letta.com/blog/letta-code)
- [Letta — Benchmarking AI Agent Memory][letta]
- [Hindsight benchmarks][hindsight]
- [OpenAI — next evolution of the Agents SDK](https://openai.com/index/the-next-evolution-of-the-agents-sdk/)
- [Cloudflare R2 pricing & API](https://developers.cloudflare.com/r2/pricing/)

[memtool]: https://platform.claude.com/docs/en/docs/agents-and-tools/tool-use/memory-tool
[hindsight]: https://benchmarks.hindsight.vectorize.io
[letta]: https://www.letta.com/blog/benchmarking-ai-agent-memory

## [0.3.3] - 2026-04-24

### Highlights

Patch release focused on the three v0.3.2-deferred items named as the
v0.3.3 target (Tasks A + B + G of the 2026-04-24 prompt). Four Rust and
three TypeScript Dependabot PRs absorbed; TS 6.0 (#26) held for a
separate validation pass. No runtime API removed; every new knob is
opt-in and defaults to the v0.3.2 behaviour.

Six GitHub issues filed (#36–#41) tracking: Hindsight SOTA gap, full
MINJA-procedure harness, golden DuckDB fixtures, R2/GCS/Azure
workspace backends, TS 6.0 migration, and DuckDB 1.5.2 + DuckLake v1.0
evaluation.

### Added

- **Embedding z-score outlier detector** (Task A — closes v0.3.2
  deferred item). `crates/mnemo-core/src/anomaly/outlier.rs` with
  Mahalanobis-proxy scoring over a diagonal-covariance per-agent
  baseline trained via Welford's algorithm. `PoisoningPolicy` struct
  in `query/poisoning.rs` with `with_outlier_threshold(z)` enabling
  the gate; off by default, pinned `is_outlier = false` below
  `MIN_BASELINE_SAMPLES = 30`. `OUTLIER_SCORE_CONTRIBUTION = 0.5`
  added to anomaly score on fire so one outlier alone crosses the
  `is_anomalous >= 0.5` bar.
- **`embedding_baseline` storage table** (DuckDB + PostgreSQL JSONB).
  `StorageBackend::{get,insert_or_update}_embedding_baseline`.
  `CURRENT_PERSISTENCE_VERSION` bumped 3 → 4; pre-existing v0.3.2
  files auto-create the table on open.
- **`mnemo baseline --train --agent-id <id>`** CLI subcommand.
- **LLM-as-judge scorer** (Task B — closes v0.3.2 deferred item).
  `python/mnemo/benches/judge.py` with `LlmJudge` + `JudgeVerdict`;
  default model `claude-haiku-4-5-20251001`, override via
  `MNEMO_JUDGE_MODEL`. YES/NO/UNSURE contract with UNSURE counted as
  miss. Judge failures surface as `JudgeUnavailableError` so the
  runner falls back to `--judge=exact` with a warning rather than
  silently degrading.
- **`--judge=exact|llm`** flag on `mnemo.benches.locomo_runner`.
- **PyMnemoClient full-text default.** `python/src/lib.rs::MnemoClient::new`
  now attaches a persistent Tantivy full-text index by default
  (kwarg `with_full_text=True`). Fixes the v0.3.0–0.3.2 bug where
  `strategy="hybrid_rrf"` silently collapsed to vector-only because
  `full_text` was never wired at the Python boundary. New kwarg
  `with_noop_embedding=True` makes the Noop fallback explicit: set
  to `False` and the constructor raises rather than silently
  zero-vectoring.
- **Nightly benchmark regression gate.** `.github/workflows/benchmarks-nightly.yml`
  + `.github/scripts/check_bench_regression.py` fail CI on >3pp
  recall@10 drop vs `docs/benchmarks/baseline.json`. First-run
  exception: empty baseline lets the first authenticated run seed
  the reference point without a false-positive failure.
- **Security workflow.** `.github/workflows/security.yml` runs
  `cargo audit` + `cargo deny check advisories` on push / PR /
  nightly. Thirteen RustSec advisories catalogued with paragraph-level
  rationales in `.cargo/audit.toml` + `deny.toml`; the gate lights
  up on any NEW advisory not already documented.
- **`**/node_modules/` in `.gitignore`** — was missing, would have
  made any legitimate `git add sdks/typescript/` pull the entire npm
  install tree.

### Changed

- Dependabot batch absorbed:
  - `sha2` 0.10 → 0.11 (PR #28).
  - `criterion` 0.5 → 0.8 (PR #13).
  - `rand` 0.9 → 0.10 (PR #12) with a one-line `use rand::Rng`
    migration in `mnemo-compliance::audit::WorkspaceSigner::generate_ephemeral`.
  - `ndarray` 0.16 → 0.17 (PR #11), feature-gated under `onnx`.
  - `@modelcontextprotocol/sdk` 1.26.0 → 1.29.0 (PR #31).
  - `@types/node` 20.19.32 → 25.5.2 (PR #30).
  - `ts-jest` 29.4.6 → 29.4.9 (PR #29).
- `sdks/typescript/jest.config.js` now carries the standard
  NodeNext-style `.js` moduleNameMapper. Pre-existing breakage: the
  whole TS test suite failed to even load on main because Jest could
  not resolve `import ... from "./types.js"` against `types.ts`.
- PR #27 (the original rmcp 0.14 → 0.16 attempt) closed unmerged
  back in v0.3.2. The rmcp 1.3 landing happened via the workspace
  dep bump in commit `d4bad6b` as part of PR #35. This CHANGELOG
  entry exists because the v0.3.3 prompt asked for the path to be
  documented here — rmcp sits at 1.3 today; workflow captured.

### Tests

- **+6 unit tests** in `anomaly::outlier::tests` — train-from-records,
  no-embedding early-exit, in-distribution-not-flagged, far-OOD-flagged,
  noisy-baseline pin, dim-mismatch passthrough.
- **+1 integration test** `test_zscore_outlier_catches_semantic_drift`
  — asserts (1) no-baseline passthrough, (2) in-distribution probe
  not flagged, (3) 50σ-drift probe flagged with the z-score reason
  string surfaced.
- **+11 Python unit tests** in `python/tests/test_judge.py` —
  YES/NO/UNSURE parse, bullet-prefix tolerance, unparseable-line
  fallback, no-memories short-circuit, SDK-exception path,
  prompt-shape contract, content truncation, env-driven model
  override, frozen-dataclass contract.
- Full suite: Rust 170 pass (77 unit + 52 integration + all other
  crates) / Python 54 pass + 4 skipped (OpenAI-gated).

### Benchmarks

- `docs/benchmarks/2026-04-24-poisoning-outlier.md` — methodology
  doc for Task A. Publishes correctness of the detector (unit +
  integration green) but **declines** to publish TPR/FPR labelled
  as "MINJA" because the paper ships a procedure, not a corpus.
  Full attack-success-rate harness tracked as issue #37.
- `docs/benchmarks/2026-04-24-mnemo-v0.3.3.md` — Task B scaffolding
  + plan. Numeric recall@10 / MRR / latency for LoCoMo-MC10 and
  LongMemEval are deferred to the first nightly run authenticated
  with `ANTHROPIC_API_KEY` + `OPENAI_API_KEY` + `HF_TOKEN`; the
  code path is ready.

### Deferred to v0.3.4 / v0.4.0

- Graphiti-style temporal edge layer (Task C). Tracked separately.
- DuckLake v1.0 opt-in storage backend (Task D). Issue #41.
- R2 / GCS / Azure workspace backends (Task E). Issue #39.
- Anthropic Claude Opus 4.7 raw-API memory-tool adapter (Task F).
- Golden DuckDB fixtures `v0_1_1.mnemo.db` / `v0_3_0.mnemo.db`
  (carried forward from v0.3.2). Issue #38.
- Transitive fixes for the 13 ignored RustSec advisories — each
  owner-pinned to the next respective dep-bump PR (see
  `.cargo/audit.toml`).
- TypeScript 5.9 → 6.0 (PR #26 held). Issue #40.

## [0.3.2] - 2026-04-21

### Highlights

Closes every v0.3.1-deferred task: real MINJA poisoning numbers, real
S3 workspace backend, persistence format stamp, and the long-awaited
rmcp 0.14 → 1.3 upgrade with MCP resource exposure.

### Added

- **MINJA / InjecMEM indirect-injection detector** — new signal on
  `check_for_anomaly`: self-referential instruction markers
  ("remember this", "in the future, always", 13 total) fire only when
  the record arrived via `SourceType::Retrieval|Import` or a
  `source:web|document|email|third_party|retrieved` tag. Legitimate
  "please remember …" from user input is not flagged.
- **Quarantine replay** — `engine.replay_quarantine(agent_id, since)`
  returns every quarantined record with id / agent / content / reason /
  source_type / tags / created_at, chronologically ordered.
- **Public MINJA-style numbers** at
  `docs/benchmarks/2026-04-21-poisoning.md`: TPR 0.960, FPR 0.000, F1
  0.980 against a 50-prompt in-repo fixture modelled on
  arXiv:2503.03704. Clears both brief bars (TPR ≥ 0.85, FPR ≤ 0.05).
- **`mnemo.openai_sandbox` subpackage**
  (`pip install mnemo[openai-sandbox-s3]`):
    - `LocalSnapshotSpec` / `RemoteSnapshotSpec` — the GA
      `SnapshotSpec` split.
    - `WorkspaceSigner` + `dump_workspace` / `load_workspace` —
      Ed25519-signed manifest, per-file SHA-256 digests, symlink
      preservation (walks with `PurePosixPath`, records
      `{source, target}` pairs).
    - `S3Workspace` — real `boto3`-backed implementation of the workspace
      put / get / delete contract (`s3://<bucket>/<key_prefix>/files/...`).
    - Tamper detection fails closed on both manifest tamper (Ed25519
      `InvalidSignature`) and per-file tamper (`ValueError`).
- **Persistence format stamp** — new `mnemo_meta` table carries a
  `persistence_version` row (currently `3`). `run_migrations` stamps
  fresh files on first open; legacy v0.1.1 / v0.3.0 / v0.3.1 files get
  stamped the first time a v0.3.2 reader opens them.
  `CURRENT_PERSISTENCE_VERSION` exported for downstream tooling.
- **MCP resources** — the rmcp 1.3+ `list_resources` / `read_resource`
  handlers surface the 50 most recent memories as
  `mem://<uuid>` resources with `text/markdown` MIME. The server now
  advertises the `resources` capability as well as `tools`.

### Changed

- **rmcp 0.14 → 1.3** (satisfied by 1.5 on the current lockfile). The
  `ServerInfo` / `Implementation` / `ReadResourceResult` shapes moved to
  `#[non_exhaustive]` in the upstream crate; `MnemoServer::get_info`
  now builds `ServerInfo` through `Default::default()` + field
  assignment + `Implementation::from_build_env()` with the name and
  version overridden. Closes the PR #27 deferral.
- Two new `EventType` variants — `ReflectionCompleted`,
  `DreamReportIngested` — were already added in v0.3.1; no change here.

### Tests

158 Rust tests, 0 failed, including the new MINJA bench, quarantine
replay, persistence version stamp tests, and the resource-surface
storage contract test. 43 Python tests (5 new S3 workspace tests
including a moto-backed round-trip) — 43 pass, 4 skipped gracefully
when `OPENAI_API_KEY` is absent.

### Deferred to v0.3.3

- **Embedding z-score outlier detector** (part of Task 3) — needs a
  baseline-mean training pass on the corpus. Queued alongside the
  benchmark-harness `--train-baseline` step.
- **LLM-as-judge scoring** for LongMemEval's inferential gold answers.
  Will re-run the 2026-04-21 benchmark and lift the zero-recall floor.
- **R2 / GCS / Azure workspace backends**. Stubs remain in place behind
  the matching `mnemo[openai-sandbox-<backend>]` extras.
- **Golden DuckDB fixtures** (`v0_1_1.mnemo.db`, `v0_3_0.mnemo.db`).
  Generating a deterministic 0.1.1 file needs a pinned historical
  build; held for a dedicated follow-up.

## [0.3.1] - 2026-04-21

### Highlights

Honesty pass on top of v0.3.0: first public benchmark numbers, Auto Dream
cadence coordination, typed error surface for the Python client, and the
five documentation pages the v0.2.0/v0.3.0 acceptance checklists kept
promising. Four tasks from the v0.3.1 brief remain deferred to v0.3.2 —
listed below.

### Added

- **First public LoCoMo / LongMemEval numbers**
  (`docs/benchmarks/2026-04-21-mnemo-v0.3.0.md`). The harness runs; the
  numbers are floor values because two v0.3.0 bugs surfaced during the
  run (the Python `MnemoClient` does not attach a full-text index, and
  the default `NoopEmbedding` collapses semantic retrieval to noise).
  Report documents both root causes and opens four tracking items.
- **Auto Dream cadence coordination**. New `ReflectionMode::Coordinated`
  gate on `engine.run_reflection_pass_with_mode(agent_id, mode, force)`
  honours the same 24 h / 5-record cadence Auto Dream uses. Gate
  decisions surface as `SkipReason::{TooSoon, NotEnoughNewRecords}` on
  the returned report.
- **Auto Dream organization-report ingestion**. `parse_organization_report`
  parses the standard trailer (`Consolidated: N / Removed: M /
  Re-indexed: K`); `ingest_dream_reports` walks agent memories, emits
  one `EventType::DreamReportIngested` event per trailer, and marks
  `metadata.dream_report_ingested_at` for idempotency.
- **Typed `mnemo.availability` module** — `is_native_available()`,
  `native_build_hint()`, `installed_adapters()`. Replaces the opaque
  `AttributeError` adapters used to produce when the PyO3 extension
  wasn't built with a clean `MnemoClientUnavailable` error carrying the
  build hint.
- **`python -m mnemo doctor`** subcommand — prints Python + platform,
  native-extension status, and an adapter probe table. Exits 0 when
  the core client is available, 1 otherwise.
- **Five documentation pages** finally on `main`:
  `docs/src/integrations/claude-agent-sdk.md`,
  `integrations/openai-agents-ga.md`, `concepts/memory-tiers.md`,
  `compliance/dpdpa.md`, `compliance/eu-ai-act.md`. Wired into
  `docs/SUMMARY.md`. The memory-tiers page explicitly flags that
  `MemoryTier` is a type alias over `MemoryType`, not a separate field.

### Changed

- Two new `EventType` variants: `ReflectionCompleted` and
  `DreamReportIngested`. Both additive; hash-chain-linked.
- `claude_agent_sdk`, `openai_sessions`, and `openai_sessions_ga` adapter
  constructors raise `MnemoClientUnavailable` instead of a generic
  `ImportError`.
- The four integration tests in `test_claude_agent_sdk.py` that need
  real embeddings now skip when `OPENAI_API_KEY` is unset, rather than
  failing opaquely under `NoopEmbedding`.

### Deferred to v0.3.2

Documented in the v0.3.1 roadmap; not regressions from v0.3.0.

- **Task 3 — MINJA poisoning benchmark + quarantine replay.** The
  poisoning detector exists in `mnemo-core` but has no published TPR
  / FPR numbers against the MINJA fixture.
- **Task 4 — Real S3 snapshot backend + `SnapshotSpec` split.** v0.3.1
  ships the local workspace backend; S3/R2/GCS/Azure remain stubs that
  raise `NotImplementedError` pointing at the matching `mnemo[...]`
  extras.
- **Task 5 — Persistence format stability + migration tests.** Adding
  `persistence_version` to the `mnemo_meta` table and landing golden
  v0.1.1 / v0.3.0 DuckDB fixtures is queued.
- **Task 8 — Merge rmcp 1.3 (PR #27) + expose MCP resources.** Still
  open; rebase needs a fresh look.

## [0.3.0] - 2026-04-20

### Highlights

Auto-Dream-aware consolidation, Letta-style memory tiers, DPDPA +
EU AI Act compliance primitives, pgvector CVE-2026-3172 fix, and a
public LongMemEval / LoCoMo benchmark harness. Rolled up on top of
v0.2.0 (which was merged to main the same day).

### Added

- **Letta-style memory tiers** (`MemoryTier` type alias for the existing
  `MemoryType` enum; Working / Procedural / Semantic / Episodic). The
  engine now applies tier-specific behaviours on write: Working memories
  auto-expire after `ttl_working_seconds` (default 3600s) when no explicit
  ttl is given, and Procedural memories are clamped to the
  `procedural_importance_floor` (default 0.8) so system prompts never
  fall below recall visibility. New builder knobs
  `with_ttl_working_seconds` and `with_procedural_importance_floor`.
- **Auto-Dream-compatible reflection pass** —
  `engine.run_reflection_pass(agent_id)` performs date absolutization
  (regex rewrites `"yesterday"`, `"last week"`, `"N days ago"`, etc. to
  ISO-8601 anchored on `created_at`), accepts external rewrites
  (`metadata.dreamed_at`) and re-embeds, consolidates semantically
  near-duplicate records (`cosine ≥ 0.92`) into the newer record with
  merged tags + summed access_count, auto-resolves low-importance
  conflicts via `KeepNewest`, and archives stale low-importance
  records. Emits `ReflectionReport` with per-phase counts.
- **OpenAI Agents SDK GA snapshot store** —
  `mnemo.openai_sessions_ga.MnemoSnapshotStore` implements
  `save_snapshot` / `load_snapshot` / `list_snapshots` / `resume` plus
  `SnapshotRef` with a `snapshot://<session>/<ts>` URI. Pluggable
  `WorkspaceStorage` supports local FS today and stubs S3/R2/GCS/Azure
  behind the matching `mnemo[openai-sandbox-<backend>]` extras. Payloads
  above `inline_threshold_bytes` (default 64 KiB) offload to workspace;
  Mnemo keeps pointer + SHA-256 and verifies integrity on load.
- **DPDPA consent manager adapter** in the new `mnemo-compliance` crate
  — `ConsentSource` trait, `HttpConsentManager` (generic HTTP binding
  with optional bearer auth), `StaticConsentSource` (tests / single-
  tenant self-hosting). `ConsentState` carries scope list, expiry, and
  consent-token hash. `ComplianceError::ConsentDenied` surfaces cleanly.
- **EU AI Act audit export** — `export_audit_log(events, format, signer)`
  with two formats: `NdjsonSigned` (one JSON line per event plus a
  detached Ed25519 signature chain covering `SHA256(index ∥ prev_hash
  ∥ event_json)`; canonicalised through `serde_json::Value` so signer
  and verifier agree on bytes) and `EuAiOfficeCsv` (the AI Office GPAI
  template columns with RFC4180 escaping). `verify_ndjson_signed`
  walks the chain and rejects tampered rows with the offending index.
- **Benchmark harness** — `mnemo.benches.locomo_runner` (with CLI)
  runs `recall@5`/`recall@10`/MRR/p50/p95/p99 across
  `auto`/`vector_only`/`hybrid_rrf`/`graph_boosted` strategies and
  emits a Markdown report + JSON sidecar under `docs/benchmarks/`.
  Real dataset loaders stubbed behind the `mnemo[benchmark]` extra;
  first live numbers published in v0.3.0-rc2.

### Changed

- `pgvector` upgraded from 0.4 → 0.8.2 to pick up the fix for
  **CVE-2026-3172** (buffer overflow in parallel HNSW builds). Also
  enables `hnsw.iterative_scan` for strict-order filtered recall — the
  migration SQL will adopt it once PostgreSQL backends regenerate
  indexes.

### Carried forward from the unreleased v0.2.0

The full T1–T6 v0.2.0 feature set is included (Claude Agent SDK
adapter, OpenAI preview `Session` store, TTL sweeper,
GDPR-safe `forget_subject`, `replay(as_of=...)`, recall
`ScoreBreakdown` / `explain`). v0.2.0 was merged to main earlier today
via admin merge; the tag itself is skipped.

### Deferred to v0.3.0-rc2

- **rmcp 0.14 → 1.3 + MCP resource exposure** (prior T7). PR #27 stays
  open; the API migration is its own release.
- **DuckDB 1.4 → 1.5.2 + DuckLake opt-in backend** (Task 12b). Ships
  behind the `storage-ducklake` feature flag once the sorted-table +
  bucket-partitioning API lands.
- **First published LongMemEval / LoCoMo numbers**. The harness is
  shippable today; the datasets come with the `mnemo[benchmark]` extra.

## [0.2.0] - 2026-04-20

### Highlights

Claude Opus 4.7 + OpenAI Agents SDK first-class support, GDPR-safe subject
erasure, time-travel replay, and retrieval provenance.

### Added

- **Claude Agent SDK adapter** (`mnemo.claude_agent_sdk.MnemoClaudeMemory`).
  Exposes the full Mnemo MCP tool surface to `ClaudeAgentOptions.mcp_servers`
  and optionally materializes recalled memories into Markdown files with YAML
  frontmatter. A `watchdog` observer mirrors file edits, deletes, and
  frontmatter changes back into Mnemo so Opus 4.7's Auto Memory workflow and
  the persistent database stay in sync.
- **OpenAI Agents SDK `Session` store** (`mnemo.openai_sessions.MnemoSessionStore`).
  Implements the `get_items`/`add_items`/`pop_item`/`clear_session` protocol
  introduced in the 2026-04-15 release, storing each turn as a
  session-tagged episodic memory with a monotonic index so conversations
  survive process restarts.
- **TTL sweeper** (`engine.run_ttl_sweep`). Hard-deletes every memory whose
  `expires_at` is in the past and emits a `MemoryExpired` audit event per
  deletion, with correct hash chain linkage. The `mnemo` CLI gains
  `--ttl-sweep-interval` / `MNEMO_TTL_SWEEP_INTERVAL` that drives the sweeper
  as a background tokio task.
- **GDPR / DPDPA-aligned subject erasure** (`engine.forget_subject`). Finds
  memories tagged `subject:<id>` and either redacts the content (default,
  preserves the hash chain for audit) or hard-deletes them. Exposed via
  MCP (`mnemo.forget_subject`), REST (`POST /v1/forget_subject`), and gRPC
  (`ForgetSubject`). A new `ForgetStrategy::Redact` variant is also
  accepted wherever the standard `mnemo.forget` strategy parsing runs.
- **Point-in-time replay** (`ReplayRequest.as_of`). When set, the engine
  synthesizes a virtual checkpoint from the memories and events that
  existed at that timestamp and returns the reconstructed state. Exposed
  via MCP, gRPC (`ReplayRequest.as_of`), REST, and a new `as_of` kwarg on
  the PyO3 `replay` method.
- **Ranking-signal provenance on recall** (`RecallRequest.explain`). When
  `true`, each `ScoredMemory` carries a `ScoreBreakdown` reporting the
  per-signal contributions (vector, BM25, graph, recency) and the final
  RRF rank. Wired through MCP, REST (`?explain=true`), gRPC (`ScoreBreakdown`
  message + `ScoredMemory.score_breakdown`), and the PyO3 `recall(..., explain=True)`
  kwarg.
- `EventType::MemoryExpired` and `EventType::MemoryRedact` variants with
  snake_case `Display`/`FromStr` support, so the audit trail can
  distinguish natural expiration and subject redaction from ordinary
  deletes.
- Examples: `examples/claude_agent_sdk_example.py`,
  `examples/openai_agents_snapshot_example.py`.

### Changed

- `RecallRequest` gains `explain: Option<bool>`.
- `ReplayRequest` gains `as_of: Option<String>`.
- `ForgetStrategy` gains a `Redact` variant.
- `ScoredMemory` gains `score_breakdown: Option<ScoreBreakdown>` (skipped
  during serialization when absent — existing JSON consumers unaffected).
- Python `mnemo/__init__.py` now tolerates a missing native `_mnemo`
  extension at import time so adapter modules can be imported before
  `maturin develop` runs.

### Tests

All 36 integration tests, 70 mnemo-core unit tests, and the MCP / pgwire /
REST / admin / gRPC suites pass. Four new tests cover TTL sweep semantics,
GDPR-safe redaction (hash chain preservation), point-in-time replay, and
score-breakdown provenance. The Python adapters ship with 21 tests
(pure-Python + integration-gated) that run under `pytest python/tests/`.

### Deferred to 0.2.0-rc2 / 0.3.0

- `mnemo.reflect` Auto Dream equivalent (reflection-pass consolidation).
- rmcp 0.14 → 1.3 upgrade (PR #27) and MCP resource exposure — the API
  migration warrants a dedicated release.

## [0.1.0] - 2026-02-07

### Initial Release

Mnemo is an MCP-native memory database that gives AI agents persistent, searchable, and secure long-term memory.

### Highlights

- **10 MCP tools** for AI agents: remember, recall, forget, share, checkpoint, branch, merge, replay, delegate, and verify
- **Hybrid search** combining semantic vectors, BM25 keyword matching, knowledge graph signals, and recency scoring via Reciprocal Rank Fusion
- **Two storage backends**: embedded DuckDB for single-agent use and PostgreSQL with pgvector for distributed multi-agent deployments
- **SDKs** for Python (with OpenAI Agents, Mem0, LangGraph, and CrewAI adapters), TypeScript, and Go
- **Multiple access protocols**: MCP (stdio), REST API, gRPC, and PostgreSQL wire protocol

### Features

- **Memory lifecycle management** -- five forgetting strategies (soft delete, hard delete, decay, consolidation, archive), TTL-based expiration, and automatic decay passes
- **Security and integrity** -- AES-256-GCM at-rest encryption, SHA-256 hash chain integrity verification, RBAC with ACL-based permission filtering, memory poisoning detection, and delegation with depth-limited transitive permissions
- **Conflict resolution** -- automatic detection of contradictory memories with newest-wins, highest-importance, manual, and evidence-weighted resolution strategies
- **Branching and replay** -- checkpoint agent state, branch timelines, merge branches, and replay event history with hash chain verification
- **Causal debugging** -- trace event causality chains with configurable direction (up/down/both) and event-type filtering
- **Point-in-time queries** -- recall memories as they existed at any historical timestamp using `as_of`
- **Observability** -- OTLP span ingestion with OpenTelemetry GenAI semantic conventions, admin dashboard with agent statistics

### Infrastructure

- 9-crate Rust workspace with full CI (format, clippy, test, build, security audit)
- Helm chart for Kubernetes deployment with S3 cold-storage support
- Docker and Docker Compose configurations
- mdBook documentation site

---

## [0.1.1] - 2026-02-07

### Security

- **Fix SQL injection in PostgreSQL backend** -- replaced string-interpolated embedding values with parameterized `pgvector::Vector` bindings via sqlx
- **Add authentication to pgwire server** -- cleartext password authentication before connection acceptance; default bind changed from `0.0.0.0` to `127.0.0.1`
- **Harden CORS configuration** -- replaced permissive CORS with configurable origin allowlist via `MNEMO_CORS_ORIGINS` environment variable, defaulting to localhost only
- **Fix delegation authorization bypass** -- delegation endpoint now verifies the caller has `Delegate` permission on each target memory before creating delegations
- **Upgrade pyo3 to 0.24** -- fixes buffer overflow in `PyString::from_object` (RUSTSEC-2025-0020)
- **Upgrade tantivy to 0.25** -- resolves transitive `lru` crate unsoundness
- **Add constant-time hash comparison** -- all hash verification now uses `subtle::ConstantTimeEq` to prevent timing side-channel attacks
- **Sanitize error responses** -- internal error details are logged server-side; clients receive generic error messages
- **Add request body size limits** -- REST API enforces a 2 MB maximum request body to prevent denial-of-service via oversized payloads
- **Add prompt injection detection** -- memory content is now scanned for 11 common prompt injection patterns during anomaly scoring

### Improvements

- **Add CI security scanning** -- new cargo-audit job in GitHub Actions plus Dependabot for Cargo, npm, and GitHub Actions dependencies
- **Add agent_id input validation** -- agent identifiers are now validated for length (max 256 characters) and allowed characters (alphanumeric, hyphens, underscores, dots)
- **Add sync_metadata table to PostgreSQL migrations** -- ensures sync watermark operations work correctly in distributed deployments
- **Generate TypeScript SDK lockfile** -- `package-lock.json` committed for reproducible builds and `npm audit` support

### Documentation

- Remove hardcoded passwords from deployment examples -- Docker, Kubernetes, and PostgreSQL docs now use environment variable references
- Add CONTRIBUTING.md with contribution guidelines
- Add project memory configuration for development tooling
