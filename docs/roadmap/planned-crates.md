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

_Last reconciled: 2026-07-03 (v0.5.5)._
