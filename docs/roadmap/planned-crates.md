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

## Published-but-not-version-tracked crates (exist, on crates.io at 0.4.4)

> **Different category from the Planned list above.** These crates **do exist**
> under `crates/`, **are** `[workspace] members`, and **are already on crates.io**
> — but only at **0.4.4**, because they are **not** in the tag-gated publish
> closure (`.github/workflows/release-crate.yml`) and so do not move with the
> workspace version. A user reading crates.io sees 0.4.4 against a 0.5.x
> workspace. This section exists so that gap is **recorded, not silent.**

**Decision (2026-07-31, v0.5.21): keep them OUT of the publish closure.** The
closure is scoped to exactly what `README.md` tells a user to install — the engine
+ compliance + the three interface surfaces (`cargo add mnemo-core
mnemo-compliance mnemo-mcp`; `mnemo-postgres` / `mnemo-rest` / `mnemo-grpc`) plus
the `mnemo-db` name-pointer. The eight below are **advanced integration adapters**
listed only in the README feature table (by repo path, "New in v0.4.x"), each an
optional shim to an external system with **no in-workspace consumer** (verified:
seven are depended on by nobody; `mnemo-admin` is a dep of the unpublishable
`mnemo-cli` only). Widening the auto-publish set to carry eight adapters that no
documented install path references would be scope creep, so they stay at 0.4.4 and
are recorded here instead.

| Crate | crates.io | What it is | Why not in the closure |
|---|---|---|---|
| `mnemo-admin` | 0.4.4 | Admin dashboard API handlers | Dep of `mnemo-cli` only (itself unpublishable); no `cargo add` path |
| `mnemo-baseline` | 0.4.4 | Per-agent behavioural baseline + OTel/OCSF drift emitters | Optional telemetry adapter; no workspace consumer |
| `mnemo-cma` | 0.4.4 | Anthropic CMA-Memory drop-in compat shim | Optional external-format bridge; no workspace consumer |
| `mnemo-codemode` | 0.4.4 | Sandboxed-WIT code-mode recall | Optional; no workspace consumer |
| `mnemo-deal` | 0.4.4 | Chained-HMAC agent-deal ledger + discovery/reputation | Optional substrate; no workspace consumer |
| `mnemo-md-sync` | 0.4.4 | Bidirectional Markdown-wiki ↔ Mnemo sync | Optional; no workspace consumer |
| `mnemo-mesh` | 0.4.4 | SPIFFE-style identity + per-namespace ACL | Optional; no workspace consumer |
| `mnemo-letta` | 0.4.4 | Letta-protocol-compatible REST surface | Optional; no workspace consumer |

**Not yanked.** Yanking is destructive to anyone who pinned `0.4.4`, buys nothing
here (these are honest older cuts, not broken or malicious), and requires the same
crates.io credential the closure publish is currently blocked on. If any is later
judged genuinely dead, check its crates.io reverse-dependencies first, then yank
deliberately — do not fold it into a routine release.

**Unpublished by design (leave alone).** `mnemo-amp`, `mnemo-golem-host`, and
`mnemo-golem-wit` are **intentionally not on crates.io**: the two `golem-*` crates
are the WASM-component vertical slice (`golem-wit` is a `wasm32-wasip2` cdylib whose
version-script link the host toolchain rejects — the chronic Build/Test red), and
`mnemo-amp` is a workspace-internal crate with no standalone publish story. They
carry workspace version pins for lockstep builds but ship no crates.io release.
Do not re-litigate publishing them.

_Last reconciled: 2026-07-31 (v0.5.21). The seven Planned entries above were
re-verified against `ls crates/` — none of `mnemo-envelope` / `mnemo-aas01` /
`mnemo-mgt` / `mnemo-bench-cf` / `mnemo-langgraph` / `mnemo-purview` /
`mnemo-toolhive` exists in the tree; the list is unchanged and still accurate._
