# Verify a mnemo log yourself, without trusting mnemo

Five minutes, one Python file, no dependencies. At the end you will have watched
a tampered record get caught by name.

This page does not tell you what your obligations are. mnemo is a component. It
produces **a tamper-evident, independently verifiable event log**; whether that
satisfies a particular regulation is a question about your system and your
regulator, not about this tool.

## Why not just use `mnemo verify`?

Because you would be running the vendor's code to check the vendor's log.

mnemo can verify its own chain several ways — `mnemo_core::hash::verify_chain`,
the REST `POST /v1/verify` endpoint, the `mnemo.verify` MCP tool. All of them
are real, and all of them require linking or running mnemo. For an internal
regression test that is fine. For an audit it is circular.

So the verifier below is a **single Python file whose only imports are
`hashlib`, `json`, `sys` and `argparse`** — all standard library. No network
call, no subprocess, no file write. You can read it end to end in a few minutes
and satisfy yourself about that, which is the entire point.

## 1. Write three records

Any write path works; this uses the MCP tool surface. One invocation per record
here is incidental — concurrent writes chain too, which was [not true when this
page was written](#what-writing-this-page-found).

```bash
for C in "Consent obtained from data subject 4471 on 2026-09-01." \
         "Retention period set to 24 months per policy R-12." \
         "Access request from subject 4471 fulfilled on 2026-09-01."; do
  printf '%s\n' \
   '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"a","version":"0"}}}' \
   '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
   "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"mnemo.remember\",\"arguments\":{\"content\":\"$C\"}}}" \
   | MNEMO_DB_PATH=./audit.db mnemo >/dev/null
done
```

## 2. Export the chain

```bash
mnemo --db-path ./audit.db audit export --out chain.jsonl
```

```
exported 3 record(s) for agent 'default'. Verify with:
  python3 tools/verify_mnemo_chain.py chain.jsonl
```

The export is deliberately dull: JSONL, one record per line, every field a
string or null, hashes in lowercase hex. Nothing in it requires a mnemo type to
parse — if it did, the verifier would have to link mnemo, and we would be back
where we started.

## 3. Verify it. It passes.

```bash
python3 tools/verify_mnemo_chain.py chain.jsonl; echo "\$? = $?"
```

```
OK: 3 records, chain intact (strict).
$? = 0
```

## 4. Tamper with a record — this is the part that matters

A verifier that only ever passes demonstrates nothing. Shorten the retention
period from 24 months to 6, directly in the file, the way someone quietly
rewriting history would:

```bash
grep -o '"content":"Retention[^"]*"' chain.jsonl
```

```
"content":"Retention period set to 24 months per policy R-12."
```

```bash
sed -i '' 's/24 months/6 months/' chain.jsonl     # GNU sed: drop the ''
grep -o '"content":"Retention[^"]*"' chain.jsonl
```

```
"content":"Retention period set to 6 months per policy R-12."
```

Nothing else in the file changed. The hashes still say what they said.

## 5. Verify again. It fails, and names the record.

```bash
python3 tools/verify_mnemo_chain.py chain.jsonl; echo "\$? = $?"
```

```
BROKEN at record index 1
  id            01a05ce1-dec9-77c0-8183-884d160f444f
  expected      94b2b247b69a6a7a4db20294eaff0fedffeb0cd905d645248a2e3f6dcf93d3ee
  found         962928272922516456f21567e9f33341515f51985bbf06f887a3516d34ff3394
$? = 1
```

`expected` is what SHA-256 of the record as it now reads comes to. `found` is
what the file claims. They differ, so the content was changed after the hash was
taken. Non-zero exit, so this drops straight into a pipeline.

To rewrite that record undetected you would have to recompute its hash *and*
every link after it — which is what a hash chain is for.

## What a PASS does not mean

Stated plainly, because a verifier oversold is worse than none:

- **Not completeness.** This verifies the chain you were handed. If records
  400–500 were never exported, every remaining link still verifies. Chain
  consistency cannot detect a truncation you were not told about; that needs an
  external anchor — a signed head, a witness, a published root — and mnemo does
  not currently publish one.
- **Not truth.** A faithfully chained lie is still a lie.
- **Not authenticity.** There is no signature here, so this does not prove the
  log came from your mnemo instance rather than being manufactured wholesale.
  `mnemo-compliance` has a separate signed-NDJSON export for that; it is a
  different property with a different key.
- **Not real timestamps.** `created_at` is an input to the hash, not an
  attestation. A writer choosing its own clock is bound to that choice, not to
  its truth.

## What writing this page found

Writing the page surfaced a real defect, so it is recorded here rather than
quietly worked around. It is fixed as of v0.5.29; the account stays because a
page that only ever shows things working is not evidence.

**Concurrent writes did not chain.** `remember()` read the current chain head and
then inserted, with the record's construction, TTL resolution and encryption in
between. Three `tools/call` requests issued in one MCP session are processed
concurrently, all three read the same head, and all three were written as chain
*heads* — `prev_hash = SHA256(content_hash)` with no predecessor. The result was
three unlinked records rather than a chain of three.

The source had always said so — *"Concurrent writes for the same agent_id may
race on prev_hash lookup"* in `crates/mnemo-core/src/query/remember.rs` — but
nothing measured it, and the standalone verifier is what made it visible:

```
BROKEN at record index 1
  expected      834b96f66117c3581ef79507b80e0ba3ee9af43ca4d50d8afb0967bd9d674838
  found         b7510884b9bf61daba14a660798e1ea7670bb2f5cc64413cea726bdc33a85c97
```

That failure came from an untampered log. Under concurrency the content hashes
still protected each record individually — an edit was still caught — but the
*ordering* guarantee was absent, so removal and reordering between racing writes
were not detected.

**What the fix turned out to be.** Serialising the read-and-insert per
`(agent_id, thread_id)` took the head count from 16 to 1 at 16 concurrent
writes — and the chain still forked. The second cause is more interesting than
the first: the head was being found with `ORDER BY <timestamp> DESC LIMIT 1`, and
a write's timestamp records when it *started*, not when it was inserted. A write
that begins earlier can be inserted after one that begins later, and from that
moment the largest-timestamp row is no longer the tip; every later append links
to that stale row and the chain forks. All 16 timestamps were distinct, so this
was never a clock-resolution problem. The tip is now recorded in a `chain_heads`
pointer table, written inside the same critical section as the insert.

**And the export had the same disease.** With the chain correct in the database,
`mnemo audit export` still produced a file this verifier rejected — `BROKEN at
record index 4`, on an untampered log. The exporter emitted in `created_at` order
and repaired only *tie groups* by following chain links. Concurrency is not a
tie: every timestamp was distinct and the two orders were still out of step,
because `created_at` records when a write started and the chain records what was
inserted. The exporter now walks the whole chain from its head, with a linear
fast path for a log already in order.

That cannot launder a tampered log: an edited `content` leaves the hashes
untouched and is still caught, and a removed record leaves everything after it
unreachable from the head, which the exporter counts, reports on stderr and emits
anyway — a shorter file that verifies is the one thing an audit export must never
produce.

There is a cost worth naming: the reordering walk is quadratic in the number of
records, because `SHA256(content_hash ‖ predecessor_content_hash)` cannot be
inverted to index a predecessor directly. Only a log that is genuinely out of
order pays it.

**Still true, and out of scope of that fix:** an `agent_events` row written by
the REST OTLP ingest path carries `prev_hash: null` and is therefore outside the
chain. mnemo's own verifier skips the link check on a null `prev_hash`, so such
rows pass it silently; the standalone verifier's strict mode calls them a break,
which is the disagreement `--mnemo-compat` exists to show you.

## The verifier

[`tools/verify_mnemo_chain.py`](../tools/verify_mnemo_chain.py). Two checks, per
record, in order:

1. `content_hash == SHA256(content ‖ agent_id ‖ created_at)` — recomputed from
   the content in front of you. This is what makes an edit visible.
2. `prev_hash == SHA256(content_hash ‖ previous content_hash)` — this is what
   makes it a chain rather than a pile of hashes.

It is **stricter than mnemo's own verifier in one place**, deliberately. mnemo's
`verify_chain` skips check 2 entirely when a record's `prev_hash` is null:

```rust
if let Some(ref prev_hash) = record.prev_hash
    && !hashes_equal(prev_hash, &expected_chain) { /* fail */ }
```

A record with `prev_hash: null` therefore passes mnemo's verifier without its
link ever being checked — the file says "no link here" and is believed. Deleting
one field silences the check. This verifier treats that as a break. Run it with
`--mnemo-compat` to reproduce mnemo's exact behaviour and see whether the two
readings disagree on your file.
