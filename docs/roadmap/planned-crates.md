# Planned crates — not yet implemented

> **Status: PLANNED. None of the crates on this page exist under `crates/`,
> none are in `[workspace] members`, and none ship any code today.** This
> page is the single source of truth for crate names that appear in the
> daily-prompt ledger, comparison docs, and integration design notes but
> have **not** been built. If a name is on this list, treat every mention of
> it anywhere in the repo as aspirational, not shipped.

This file closes the loop on
[#74](https://github.com/sattyamjjain/mnemo/issues/74) — "workspace-member
drift: prompt-referenced crates not in `[workspace] members`." The rule going
forward is truth-in-advertising: **either a crate exists and is wired, or its
only home is this Planned list.** A CI guard
(`crates/mnemo-cli/tests/readme_crate_claims_are_real.rs`) fails the build if a
`mnemo-*` crate name is presented in `README.md` without being a real
workspace member, so this drift cannot silently return.

## Why these were not stubbed

Each entry below is an adapter to an external system (Purview, ToolHive,
LangGraph, OWASP AAS01, SecureAuth MGT, an OTel envelope kind, a Cloudflare
bench substrate). An empty Rust shell with no downstream consumer is exactly
what the workspace already **retired** for `mnemo-langgraph` (see the
`## [Unreleased]` / v0.4.4-backlog CHANGELOG note): a shell that compiles but
does nothing is drift wearing a workspace-member badge. So the honest state is
a labelled roadmap entry, not a placeholder crate. Any of these graduates to a
real `crates/<name>/` + `[workspace] members` entry only when there is a
concrete design + a consumer that wires it in.

## The list

| Crate | Intended purpose | Status | Tracking |
|---|---|---|---|
| `mnemo-envelope` | OTel exporter envelope kind (`EnvelopeKind::FetcherAttestation`, agent-vs-human authorship tag) | Planned — not built | [#74](https://github.com/sattyamjjain/mnemo/issues/74) |
| `mnemo-aas01` | OWASP AAS01 detector surface | Planned — not built | [#74](https://github.com/sattyamjjain/mnemo/issues/74) |
| `mnemo-mgt` | SecureAuth Trust Registry adapter | Planned — not built | [#74](https://github.com/sattyamjjain/mnemo/issues/74) |
| `mnemo-bench-cf` | Cloudflare Agent Memory bench harness (KV+Vectorize vs DO-Facets SQLite) | Planned — not built | [#74](https://github.com/sattyamjjain/mnemo/issues/74) |
| `mnemo-langgraph` | LangGraph Rust checkpoint adapter | **Retired** — superseded by the Python `MnemoCheckpointer` (`python/mnemo/checkpointer.py`); no Rust consumer | [#74](https://github.com/sattyamjjain/mnemo/issues/74) |
| `mnemo-purview` | Microsoft Purview audit-log adapter | Planned — not built | [#74](https://github.com/sattyamjjain/mnemo/issues/74) |
| `mnemo-toolhive` | Stacklok ToolHive Registry sync | Planned — not built | [#74](https://github.com/sattyamjjain/mnemo/issues/74) |

## What *does* exist (so the contrast is unambiguous)

- The functional equivalent of `mnemo-langgraph` ships today as the Python
  `MnemoCheckpointer` (back-compat alias `ASMDCheckpointer`) in
  `python/mnemo/checkpointer.py` — that is the wired path; the Rust crate is
  retired, not pending.
- The Cloudflare comparison is a **design contract** only:
  [`docs/comparisons/cloudflare-agent-memory.md`](../comparisons/cloudflare-agent-memory.md)
  and [`docs/src/integrations/cloudflare-workers-deploy.md`](../src/integrations/cloudflare-workers-deploy.md)
  describe what `mnemo-bench-cf` *would* measure. Every number there is a `TBD`
  placeholder, not a run result.

## Closed: the crates this page used to list as version-stranded

> **This section listed eight crates as "exist, on crates.io at **0.4.4**", under a
> 2026-07-31 (v0.5.21) decision to keep them out of the tag-gated publish closure.
> That gap has closed, and the rows are deleted rather than annotated** — a roadmap
> describing a gap that no longer exists reads to a first-time visitor as an
> abandoned repo.

Verified against the crates.io API on **2026-09-04**: `mnemo-admin`,
`mnemo-baseline`, `mnemo-cma`, `mnemo-codemode`, `mnemo-deal`, `mnemo-letta`,
`mnemo-md-sync` and `mnemo-mesh` each serve **0.5.29** — the workspace version.
All eight are now in the `WALK` in
[`.github/workflows/release-crate.yml`](../../.github/workflows/release-crate.yml),
so they move with every tagged release instead of being stranded behind it.

The same check found one claim wrong in the other direction. This page stated that
`mnemo-amp` was **"intentionally not on crates.io … do not re-litigate publishing
them."** It is published, at 0.5.29, and it is in `WALK`. That sentence is deleted
rather than softened.

Per-crate published versions are not tracked on this page at all any more: the
live table is generated from the registries into
[`README.md`](../../README.md) by
[`scripts/gen_published_versions.py`](../../scripts/gen_published_versions.py),
and [`scripts/check_version_drift.sh`](../../scripts/check_version_drift.sh) plus
[`scripts/registry_parity.sh`](../../scripts/registry_parity.sh) fail a release if
it drifts. A hand-maintained mirror of a generated table is how the 0.4.4 rows went
on asserting a number the registry had already moved past.

**Still unpublished, by design (leave alone).** `mnemo-golem-host` and
`mnemo-golem-wit` are the WASM-component vertical slice — `golem-wit` is a
`wasm32-wasip2` cdylib whose version-script link the host toolchain rejects, which
is why it sits in `[workspace] exclude` and builds standalone. Both carry workspace
version pins for lockstep builds but ship no crates.io release, and both were
confirmed absent from crates.io on 2026-09-04.

_Last reconciled: **2026-09-04**. The seven Planned entries above were re-verified
two ways — against `ls crates/` (none exists in the tree) and against
`https://crates.io/api/v1/crates/<name>` (none is published) — so the list is
unchanged and still accurate. The eleven other crate names on this page were
checked the same way, which is what surfaced the eight closed rows and the wrong
`mnemo-amp` claim above. When re-running that check, send a `User-Agent` header:
crates.io rejects requests without one, and a naive loop reports every crate as
unpublished, including the ones that plainly are._
