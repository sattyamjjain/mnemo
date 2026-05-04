# Spec-drift reconciliation — 2026-05-04

> **Status:** descriptive, not prescriptive. Records a long-standing
> divergence between an external skill template and this repo's own
> description so that future operators can stop relitigating it each
> day and act on the canonical source.

## What's drifting

The daily-opportunity-radar skill template (v4.3) used in mnemo's
daily product-prompt pipeline asserts that this repo's description
should read:

> "agent-memory library — semantic + episodic stores, MCP-server
> interface, LangGraph adapter, Cloudflare Workers template."

The repo's actual description on `main` (verified via
`https://api.github.com/repos/sattyamjjain/mnemo` 2026-05-04 IST)
is:

> "MCP-native embedded memory database for AI agents built in Rust.
> REMEMBER/RECALL/FORGET/SHARE primitives with hybrid vector search,
> AES-256-GCM encryption, DuckDB/PostgreSQL backends & SDKs for
> Python, TypeScript and Go."

The 54-task lifetime ledger (2026-04-19 → 2026-05-03) has operated
against the actual repo description for weeks. This divergence is
recorded so it doesn't get auto-flagged as "drift" again.

## Which is canonical

**The repo description on `main` is canonical.** Reasons:

1. **Operator policy.** "Ship one row honestly over five rows
   aspirationally" — the repo description names what mnemo *is*
   today; the skill template names what mnemo *might one day be*.
2. **SEO / discovery surface.** The repo description is what GitHub
   search indexes, what `cargo install mnemo-mcp-server`'s metadata
   pulls from, and what the v0.4.2 / v0.4.3 release notes route
   readers to. Rewriting it to match the template would silently
   degrade those signals.
3. **Topic ranking.** The repo's GitHub topics today are
   `ai-agents, ai-memory, duckdb, embeddings, encryption, llm-tools,
   mcp, memory-management, model-context-protocol, postgresql, rag,
   rust, semantic-search, vector-database` — these align with the
   actual description, not the skill-template version.

A future description rewrite is a v0.5.0+ marketing decision, not a
daily-prompt cleanup. If it happens, this file gets updated; until
then it stays as is.

## How the skill-template anchors map to this repo

The skill template anchors on four surfaces; here is where each one
actually lives in the codebase today:

| Skill-template anchor | Where it lives in this repo (2026-05-04) |
|---|---|
| **Semantic store** | Vector RRF in [`crates/mnemo-core/src/index/`](../crates/mnemo-core/src/index) (USearch HNSW) + [`crates/mnemo-core/src/search/`](../crates/mnemo-core/src/search) (Tantivy BM25). Hybrid scoring fused via Reciprocal Rank Fusion in the recall path. |
| **Episodic store** | Append-only event log in `agent_events` table + [`mnemo.replay`](../crates/mnemo-mcp/src/server.rs) MCP tool for reconstructing agent context at any prior point. PostgreSQL backend additionally enforces append-only via a `prevent_event_modification` trigger. |
| **MCP server interface** | [`crates/mnemo-mcp/`](../crates/mnemo-mcp) (Rust, `rmcp = "1.3"`) + the [`mnemo-mcp-server`](../crates/mnemo-cli) binary (`cargo install mnemo-mcp-server`). 11 tools: `mnemo.{remember,recall,forget,forget_subject,share,checkpoint,branch,merge,replay,delegate,verify}`. |
| **LangGraph adapter** | `MnemoLangGraphTools` in [`python/mnemo/langgraph_mcp.py`](../python/mnemo/langgraph_mcp.py); Python-side. The Rust-native `mnemo-langgraph` crate (LangGraph 1.2 checkpoint adapter) is parked for the v0.4.3 backlog — see [CHANGELOG](../CHANGELOG.md) carry list. |
| **Cloudflare Workers template** | v0.4.3 anchor only — the [README's `### Cloudflare Workers deploy template`](../README.md) section + the design note at [`docs/src/integrations/cloudflare-workers-deploy.md`](src/integrations/cloudflare-workers-deploy.md) document the contract. The actual `deploy/cloudflare/` scaffold + the `mnemo-bench-cf` crate are parked for v0.4.3 backlog. |

## Policy going forward

1. **Daily prompts treat the repo description on `main` as the
   ground truth.** The skill template's description is not consulted
   for surface-drift checks.
2. **Surface-drift audits compare the repo description against the
   actual code in the repo**, not against the skill template.
3. **A future description rewrite** (e.g. for v0.5.0) requires:
   - A docs PR updating both this file and the repo description in
     one shot.
   - A topics review (the GitHub topics list may need to gain
     `agent-memory`, `cloudflare-workers`, `langgraph`).
   - A README-side "Why mnemo" section update so first-impression
     framing matches.
4. **Until that PR happens**, contributors landing surface-affecting
   changes should keep `docs/spec-drift-*.md` in sync if the
   divergence shifts. See `CONTRIBUTING.md` for the policy link.

## Cross-references

- Repo description (canonical, 2026-05-04): https://api.github.com/repos/sattyamjjain/mnemo
- v0.4.3 carry list: [CHANGELOG.md](../CHANGELOG.md) `[Unreleased]` section
- Workers design note: [`docs/src/integrations/cloudflare-workers-deploy.md`](src/integrations/cloudflare-workers-deploy.md)
- LangGraph adapter (today's Python, tomorrow's Rust): [`python/mnemo/langgraph_mcp.py`](../python/mnemo/langgraph_mcp.py)
