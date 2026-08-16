# ADR 0001 — Capability-leased reads (per-read lease tokens for privileged tools)

- Status: **Accepted — built and enforced, all four properties**, opt-in via
  `--lease-ttl-seconds` (`crates/mnemo-mcp/src/lease.rs`, wired into
  `mnemo.recall` and `mnemo.forget_subject`)
- Date: 2026-08-04 (built 2026-08-15; subject-scope narrowing 2026-08-16)
- Tracking: [#126](https://github.com/sattyamjjain/mnemo/issues/126),
  [#160](https://github.com/sattyamjjain/mnemo/issues/160)
- Supersedes: the removed `crates/mnemo-cli/src/lease.rs` (v0.4.0-rc3 Task B2 dead code, deleted in the publish-drift-and-dead-lease pass)

> **2026-08-15 — this is no longer an unbuilt design.** The text below was written while it was, and is kept as the record of that reasoning. See *What shipped* at the end for the differences between the design and the implementation.

This ADR exists so an unbuilt design is recorded honestly. An unbuilt feature written down in a dated ADR is credibility. The same feature listed on a capability matrix as if it shipped is the opposite — it is the "claimed-but-not-wired" defect this repo has fixed twice (`role_filter`, #124; serve-time tool-catalog attestation, v0.5.20). Nothing below is wired. If you are looking for what mnemo enforces today, this is not it.

## The threat it addresses

The OX-MCP "exfiltrate-then-act" injection chain (disclosed 2026-04-24). A prompt-injected agent is steered to (1) read sensitive memory, then (2) invoke a privileged, irreversible tool — `mnemo.forget_subject` (a hard, subject-scoped delete), or audit-log export — using what it just read. The two steps are separately plausible; the damage is in their composition. The defensive goal is to make the "act" step impossible unless it is causally, freshly, and narrowly downstream of a legitimate "read."

## The design

Every `mnemo.recall` mints a **per-read lease token**: a short-TTL (default 60s), scoped capability naming the `agent_id` and the subject/scope the read covered. Privileged tools refuse to run without a presented lease that is unexpired, correctly scoped, and bound to the same `agent_id`. `forget_subject` gains a required `lease` argument; it deletes only within the lease's scope.

## Why per-read leases and not a session token

A session token is a bearer capability for the whole session: obtain it once and every later privileged call is unlocked. That is precisely what the exfiltrate-then-act chain wants — an injection that acquires the token early enables every downstream "act." It does not constrain the *act* to a *specific, legitimate read*.

A per-read lease binds the privileged op to one concrete recall:

- **Freshness.** The 60s TTL means a leaked or stale lease is inert; the act must follow its read within the window, not at an attacker's leisure.
- **Scope.** The lease names what the read covered, so `forget_subject` can only delete inside that scope — an injected "delete everything" cannot ride a narrow read's lease.
- **Causality.** The act cannot fire without a matching fresh read the caller also performed; the two steps can no longer be composed from independently-injected fragments.

A session token gives none of these. It is coarse where the threat is about the *pairing* of a specific read with a specific act.

## What it costs at the recall call site

This is not free, and the cost lands on the hottest path:

- **Breaking wire change.** `mnemo.recall`'s response shape grows a `lease` field; `mnemo.forget_subject` gains a required argument. Both are shipped, docs-drift-tested MCP tools, so this is a compatibility break for every existing caller.
- **Per-recall work.** Every `recall` mints, scopes, and stores a token (a `LeaseStore` write) plus TTL bookkeeping and a purge task — overhead on reads, which are far more frequent than the privileged writes the lease guards.
- **Caller threading.** The caller must carry the token from the `recall` result into the later `forget_subject` call. Agents that recall and forget in separate turns must persist it.
- **New failure surface.** Expired / wrong-scope / wrong-agent leases become a new class of (correct) refusal the caller has to handle and that needs negative tests.

## The honest reason it is not built yet

- On the **stdio transport there is no per-call caller identity** — the operator *is* the single caller. A lease keyed on `agent_id` there adds a round-trip and a response-shape break for **no cross-caller isolation**; it is ceremony, not a boundary. This is the same limitation #124 documented for the role filter, but sharper: with one caller, a lease guards nothing an attacker who controls that caller cannot also mint.
- The prior implementation was **dead code** — `LeaseStore` was allocated but never consumed while its docstring claimed the defence in the present tense. It was removed rather than left as a lie.
- One of the two named privileged surfaces, **audit-log export, is not even an MCP tool** — it is the `mnemo_compliance::export_audit_log` library function, so there is nothing at the MCP layer to gate.

## When to revisit

The lease model earns its keep on a **multi-caller, authenticated transport** (e.g. an authenticated HTTP MCP transport) where distinct callers hold distinct identities and a token meaningfully binds an act to a caller's own fresh read. Revisit alongside that transport:

1. Reintroduce the store in `mnemo-mcp` (not the CLI, where it was dead).
2. Make `recall` mint a scoped, agent-bound, TTL'd token in its response.
3. Gate `forget_subject` (and audit-log export, once it is an MCP tool) on a valid, unexpired, correctly-scoped, correctly-agent-bound lease.
4. Ship it with negative tests: expired, wrong-scope, and wrong-agent leases each rejected.

Until that transport exists, this stays a documented design, not a claimed capability. The removed implementation (`issue`/`check`/`purge`, `LeaseScope`, `LeaseError`, and its five unit tests) is preserved in git history for reference.

---

## What shipped (2026-08-15)

Built in `crates/mnemo-mcp/src/lease.rs` and wired into the two tools. Enabled
with `--lease-ttl-seconds <N>` (which requires `--capability-key`).

### The blocker was removed, not waited out

This ADR deferred the work "pending a multi-caller authenticated transport",
because on single-caller stdio a lease keyed on `agent_id` is ceremony: with one
possible caller, binding an act to that caller proves nothing.

What actually unblocked it was **per-request identity** (ADR 0002), not the
transport. A lease is bound to the capability-verified principal of the request
that minted it, so a replay by a different principal fails — including on stdio,
where a gateway may multiplex several agents over one pipe. The authenticated
HTTP transport shipped alongside it and is where the property is most visible,
but it was never the thing standing in the way.

### Three deliberate departures from the design above

(A fourth — dropping subject-scope narrowing — was listed until 2026-08-16, when
it landed instead. See the [#160 section](#subject-scope-narrowing--landed-2026-08-16-160).)

1. **No `ExportAuditLog` scope.** The design named two privileged tools, but
   `export_audit_log` is not an MCP tool — it is the
   `mnemo_compliance::export_audit_log` library function. A scope gating a tool
   that does not exist would be precisely the claimed-but-not-wired defect this
   ADR's own preamble warns about. It goes in when the tool does.
2. **Opt-in rather than mandatory.** `mnemo.recall` and `mnemo.forget_subject`
   are shipped, docs-drift-tested tools. Enforcing unconditionally would break
   every existing client on upgrade. Unattached, both behave exactly as before;
   attached, the gate is real. That trade — a defence nobody has enabled is not
   a defence — is the operator's to make, and `--lease-ttl-seconds` is where
   they make it.
3. **The lease is checked against the caller, not the requested `agent_id`.**
   `forget_subject` takes an optional `agent_id` scope argument, and validating
   the lease against *that* would let a caller nominate the identity its own
   lease is checked against — answering its own question. The check uses the
   request's resolved principal.

   The `subject_id` check added in [#160](https://github.com/sattyamjjain/mnemo/issues/160)
   is *not* the same shape, despite also reading a caller-supplied field. There,
   the caller says which subject it wants to erase and the server tests that
   against a set **the server derived from its own recall response**. The caller
   picks the question; it does not get to pick the answer.

### Subject-scope narrowing — landed 2026-08-16 ([#160](https://github.com/sattyamjjain/mnemo/issues/160))

**All four of the design's properties are now enforced.** This section previously
recorded scope narrowing as *not implemented*, and the reasoning it gave for
deferring turned out to be one step short of the answer. Both are kept below,
because a deferral that was wrong is worth reading next to what corrected it.

The original argument: a recall is a *query* (possibly semantic, possibly
filtered) while `forget_subject` erases by `subject:<id>` tag, so deriving "which
subjects did this query cover" from a ranked result set would either over-narrow
(breaking legitimate erasures whose subject ranked below the limit) or
over-broaden into the blanket authority the lease is meant to prevent. The
conclusion drawn was that closing it needed the caller to *declare* a subject set
at recall time — a further breaking change to `mnemo.recall`.

**What was missed: the returned records are not an inference.** Both failure
modes are properties of guessing from the *query*. The recall *response* is what
the caller was actually handed, and every record in it carries its own `subject:`
tags. So the lease records the subjects present on the records the read returned,
and `forget_subject(subject_id = S)` is authorised iff `S` is in that set.
Nothing is derived, so neither over-narrowing nor over-broadening applies, and
the declared-set contract change is unnecessary — a scope the caller nominates is
a scope the caller chose, which lets it answer its own question. The
result-derived set is both stricter and free.

Two consequences worth stating outright:

- **A read that surfaced no subject-tagged record authorises no erasure.** The
  empty set means *nothing*, never *anything*. Reading the empty case as
  unrestricted would have turned every subject-free recall into a blanket
  erasure grant — strictly worse than the gap this closes.
- **It narrows, it does not block.** A wide read still earns a wide lease; the
  binding is to what was read, not to one subject per lease. And where the
  original deferral worried about over-narrowing, the caller's remedy is
  ordinary: recall the subject you intend to erase, then spend that lease.

`LeaseError::SubjectNotCovered` names the subject asked for and the subjects the
read covered, so a refusal is diagnosable rather than a bare denial. The subject
check is ordered *after* agent-binding, expiry and scope, so an expired lease
still reports "expired" and a stale lease does not leak which subjects it covered.

Tests: `a_narrow_read_cannot_be_spent_on_a_different_subject`,
`a_read_covering_no_subject_authorises_no_erasure`,
`a_read_covering_several_subjects_authorises_each_of_them`, and
`subject_check_is_ordered_after_the_coarser_refusals` in
[`crates/mnemo-mcp/src/lease.rs`](../../crates/mnemo-mcp/src/lease.rs), plus
`a_lease_earned_by_a_narrow_read_cannot_erase_a_wider_subject` and
`a_read_that_surfaced_no_subject_authorises_no_erasure` in
[`crates/mnemo-mcp/tests/capability_leased_reads.rs`](../../crates/mnemo-mcp/tests/capability_leased_reads.rs).

### What it does not defend

- **A single compromised caller.** If the same principal is induced to recall
  and then erase, the lease is issued and spent legitimately. The lease breaks
  *cross-principal* replay and *stale* authority, not an agent acting against
  its own interest throughout. Scope narrowing raises the cost — the injection
  must now steer a read of *the subject it wants erased*, not any read at all,
  so a single stray recall no longer arms an arbitrary delete — but a caller
  that can be driven twice can still be driven through both steps.
- **Anything outside MCP.** The REST, gRPC and pgwire surfaces have their own
  auth and are untouched by this.
- **Durability.** Leases are process-local and die with the server, by design:
  a lease that survived a restart would be the long-lived ambient grant ADR 0002
  rejected.
