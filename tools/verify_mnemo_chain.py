#!/usr/bin/env python3
"""Verify a mnemo memory-write chain. Standalone. Read this file before you trust it.

    python3 verify_mnemo_chain.py chain.jsonl

Exit 0 if the chain is internally consistent. Exit 1 at the first break, naming
the record index and both hashes. Exit 2 if the input cannot be parsed.

WHY THIS FILE EXISTS
--------------------
mnemo can verify its own chain (`mnemo_core::hash::verify_chain`, the REST
`POST /v1/verify` endpoint, the `mnemo.verify` MCP tool). All of those require
you to link or run mnemo. An auditor running the vendor's code to check the
vendor's log is trusting the vendor.

So: this file depends on nothing but the Python standard library. It imports
`hashlib`, `json`, `sys` and `argparse`. There is no network call in it, no
subprocess, no file write. You can read the whole thing in a few minutes and
confirm that.

WHAT IT CHECKS
--------------
Two properties, per record, in order:

  1. content_hash == SHA256(content || agent_id || created_at)

     This is the property that makes the log tamper-EVIDENT. The hash is
     recomputed from the content in front of you. Edit a byte of `content` and
     this fails.

  2. prev_hash == SHA256(content_hash || previous record's content_hash)

     This is the property that makes the log a CHAIN — it is what stops a record
     being removed, reordered, or spliced in.

Byte order is exactly: the UTF-8 bytes of each field, concatenated, in the order
written above. No separators, no length prefixes, no JSON canonicalisation.
(A concatenation without separators is ambiguous in principle — see LIMITS.)

WHERE THIS IS DELIBERATELY STRICTER THAN MNEMO
----------------------------------------------
mnemo's own `verify_chain` skips check 2 entirely when a record's `prev_hash` is
absent:

    if let Some(ref prev_hash) = record.prev_hash
        && !hashes_equal(prev_hash, &expected_chain) { ... fail ... }

A record with `prev_hash: null` therefore PASSES mnemo's verifier without its
link ever being checked. That is trusting the log's own metadata: the file says
"there is no link here" and the verifier believes it. Deleting one field is
enough to silence the check.

This verifier treats a missing `prev_hash` on any record after the first as a
break. It is the stricter reading, and the difference is reported rather than
hidden: run with --mnemo-compat to get mnemo's exact behaviour and see whether
the two disagree on your file.

LIMITS — what a PASS here does and does not mean
------------------------------------------------
It means: the file is internally consistent under the two rules above. Nobody
edited a record's content, and nobody reordered or removed a record, without
also recomputing every subsequent hash.

It does NOT mean:

  * that the log is COMPLETE. This verifies the chain you were given. If the
    exporter never wrote records 400-500, every remaining link still verifies.
    Chain consistency cannot detect a truncation you were not told about, and
    no amount of hashing inside the file will fix that — it needs an external
    anchor (a signed head, a witness, a published root).
  * that the contents are TRUE. A faithfully-chained lie is still a lie.
  * that the timestamps are real. `created_at` is an input to the hash, not an
    attestation; a writer choosing its own clock is bound to that choice, not to
    the truth of it.
  * that the whole log came from mnemo, or from any particular installation.
    There is no signature here. `mnemo-compliance` has a separate signed-NDJSON
    export for that, which is a different property with a different key.

Also, because fields are concatenated without separators, two different field
splits can in principle produce the same preimage (agent_id "ab" + timestamp "c"
hashes the same as "a" + "bc"). Exploiting it requires control of the field
values, and it does not let you change `content` without detection. It is a real
property of the format, stated here rather than left for you to find.
"""

import argparse
import hashlib
import json
import sys

EXIT_OK = 0
EXIT_BROKEN = 1
EXIT_BAD_INPUT = 2


def sha256_hex(*parts: bytes) -> str:
    h = hashlib.sha256()
    for p in parts:
        h.update(p)
    return h.hexdigest()


def content_hash(content: str, agent_id: str, created_at: str) -> str:
    """SHA256(content || agent_id || created_at), UTF-8, no separators."""
    return sha256_hex(
        content.encode("utf-8"),
        agent_id.encode("utf-8"),
        created_at.encode("utf-8"),
    )


def chain_hash(this_content_hash: str, prev_content_hash: str | None) -> str:
    """SHA256(content_hash || prev_content_hash), over the RAW hash bytes."""
    parts = [bytes.fromhex(this_content_hash)]
    if prev_content_hash is not None:
        parts.append(bytes.fromhex(prev_content_hash))
    return sha256_hex(*parts)


def load(path: str) -> list[dict]:
    records = []
    stream = sys.stdin if path == "-" else open(path, "r", encoding="utf-8")
    try:
        for lineno, line in enumerate(stream, 1):
            line = line.strip()
            if not line:
                continue
            try:
                records.append(json.loads(line))
            except json.JSONDecodeError as e:
                die(EXIT_BAD_INPUT, f"{path}:{lineno}: not valid JSON: {e}")
    finally:
        if stream is not sys.stdin:
            stream.close()
    return records


def die(code: int, msg: str) -> None:
    print(msg, file=sys.stderr)
    sys.exit(code)


REQUIRED = ("id", "agent_id", "content", "created_at", "content_hash")


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Verify a mnemo memory-write chain offline. Reads a JSONL "
        "export; writes nothing; makes no network call.",
    )
    ap.add_argument("chain", help="path to the exported JSONL chain, or - for stdin")
    ap.add_argument(
        "--mnemo-compat",
        action="store_true",
        help="reproduce mnemo's own verify_chain exactly, which SKIPS the link "
        "check on any record whose prev_hash is null. Use it to see whether the "
        "strict and built-in readings disagree on your file.",
    )
    ap.add_argument(
        "--quiet",
        action="store_true",
        help="print nothing on success; the exit code is the answer",
    )
    args = ap.parse_args()

    records = load(args.chain)
    if not records:
        # An empty chain is vacuously consistent, and saying so is more useful
        # than an error — but it is NOT evidence of anything, so say that too.
        if not args.quiet:
            print("0 records: nothing to verify. An empty file proves nothing.")
        return EXIT_OK

    prev_content_hash: str | None = None
    for i, r in enumerate(records):
        missing = [f for f in REQUIRED if f not in r]
        if missing:
            die(EXIT_BAD_INPUT, f"record {i}: missing required field(s): {', '.join(missing)}")

        # --- 1. content hash, recomputed from the content in front of us ---
        expected = content_hash(r["content"], r["agent_id"], r["created_at"])
        actual = r["content_hash"]
        if expected != actual:
            print(f"BROKEN at record index {i}", file=sys.stderr)
            print(f"  id            {r['id']}", file=sys.stderr)
            print(f"  expected      {expected}", file=sys.stderr)
            print(f"  found         {actual}", file=sys.stderr)
            return EXIT_BROKEN

        # --- 2. chain link ---
        if i > 0:
            stated = r.get("prev_hash")
            if stated is None:
                if args.mnemo_compat:
                    # mnemo's own verifier skips the check here. Reproduced only
                    # under the flag, and announced, because a silent skip is the
                    # failure this file exists to avoid.
                    print(
                        f"note: record {i} has no prev_hash; mnemo's verifier skips "
                        f"its link check. Re-run without --mnemo-compat to treat "
                        f"this as a break.",
                        file=sys.stderr,
                    )
                    prev_content_hash = actual
                    continue
                print(f"BROKEN at record index {i}", file=sys.stderr)
                print(f"  id            {r['id']}", file=sys.stderr)
                print(f"  expected      {chain_hash(actual, prev_content_hash)}", file=sys.stderr)
                print("  found         null  (link absent; the file asserts no link here)", file=sys.stderr)
                return EXIT_BROKEN

            expected_link = chain_hash(actual, prev_content_hash)
            if stated != expected_link:
                print(f"BROKEN at record index {i}", file=sys.stderr)
                print(f"  id            {r['id']}", file=sys.stderr)
                print(f"  expected      {expected_link}", file=sys.stderr)
                print(f"  found         {stated}", file=sys.stderr)
                return EXIT_BROKEN

        prev_content_hash = actual

    if not args.quiet:
        mode = "mnemo-compat" if args.mnemo_compat else "strict"
        print(f"OK: {len(records)} records, chain intact ({mode}).")
    return EXIT_OK


if __name__ == "__main__":
    sys.exit(main())
