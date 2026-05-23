//! v0.4.8 — Orientation cache: an opt-in, namespace-scoped,
//! constant-token "context map" post-processor over the standard
//! `recall` result set.
//!
//! # Anchor
//!
//! [arXiv:2605.19932](https://arxiv.org/abs/2605.19932) (PEEK —
//! Prefix-Encoded Episodic Knowledge) shows that a small,
//! token-budgeted "orientation map" maintained alongside an agent's
//! retrieval surface (key entities, constants, schemas that have
//! been useful) helps the agent re-enter long-running contexts with
//! a fraction of the recall payload. The default mnemo recall path
//! (semantic, BM25, graph, recency) returns whole memory records;
//! it has no notion of a *distilled* orientation summary.
//!
//! The orientation cache runs *after* the normal recall result set
//! is computed. Given a namespace (operator-chosen — typically
//! `(org_id, agent_id)`), a `Distiller` extracts transferable
//! knowledge from each hit (capitalized entities, `UPPER_SNAKE =
//! value` constants, fenced schema fragments), and an `Evictor`
//! enforces a fixed token budget. The recall response carries the
//! resulting bounded map alongside `top-k` so the caller has both
//! "what is in scope" and "what is relevant right now" in one
//! payload.
//!
//! # Design contract
//!
//! - **Opt-in.** Triggered only when
//!   [`RecallRequest::orientation_cache`][crate::query::recall::RecallRequest::orientation_cache]
//!   is `Some` AND the engine has an [`OrientationCacheStore`]
//!   attached via
//!   [`MnemoEngine::with_orientation_cache_store`][crate::query::MnemoEngine::with_orientation_cache_store].
//!   The default read path is unchanged.
//! - **Post-processor, not a replacement.** Runs over whatever
//!   candidates the underlying `RetrievalMode` produced. Does not
//!   re-issue a query.
//! - **Constant-token guarantee.** Each rendered map is bounded by
//!   the caller's `token_budget` (default 512). The Evictor drops
//!   entries by `priority = freq × recency × (1 - token_share)`
//!   until the rendered map fits.
//! - **Namespace-scoped.** The in-memory store is keyed by
//!   `(org_id, agent_id)` (or `"__global__"` if neither is set).
//!   Updates from one namespace never bleed into another.
//! - **Deterministic distiller.** Pure regex/heuristic extraction —
//!   no LLM call, no network. Keeps the recall hot path predictable
//!   and the bench reproducible.
//!
//! # What this module is NOT
//!
//! - **Not a write-side memory consolidator.** It only summarises
//!   hits as they pass through recall; it does not rewrite or
//!   compact memories on disk.
//! - **Not a learned summariser.** The Distiller is heuristic by
//!   choice (`v0.4.8` ships the deterministic core; an LLM-backed
//!   variant is parked for v0.5.x). Treat extracted entries as
//!   pointers, not paraphrases.
//! - **Not a context-window extender.** The map fits inside the
//!   recall response and is bounded by the caller's token budget.
//!   It does not bypass any model context limit.
//! - **Not a faithful PEEK reproduction.** PEEK uses a learned
//!   prefix encoder and a write-side update path. This module
//!   adopts the *orientation map + constant-token budget* shape
//!   only; faithfulness is left to operators who can plug an
//!   embedder/LLM behind the Distiller trait in a follow-up cut.
//! - **Not persisted.** The store is in-process
//!   (`Arc<RwLock<HashMap<..>>>`). Restart drops it. Persistence
//!   to DuckDB / Postgres is a v0.5.x knob, documented in the
//!   v0.4.8 CHANGELOG entry.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::query::recall::ScoredMemory;

/// Default token budget per rendered context map.
pub const DEFAULT_TOKEN_BUDGET: u32 = 512;

/// Default namespace when neither `org_id` nor `agent_id` is supplied.
pub const GLOBAL_NAMESPACE: &str = "__global__";

/// Heuristic token estimate (~4 chars per token).
#[inline]
fn estimate_tokens(s: &str) -> u32 {
    (s.len().div_ceil(4)).max(1) as u32
}

/// Opt-in config for the orientation cache. Carried on
/// [`RecallRequest::orientation_cache`][crate::query::recall::RecallRequest::orientation_cache].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrientationCacheConfig {
    /// Operator-chosen namespace label. If `None`, the engine
    /// derives one from `(org_id, agent_id)` at request time
    /// (falling back to `"__global__"` if both are absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Maximum rendered tokens. Defaults to
    /// [`DEFAULT_TOKEN_BUDGET`]. Hard upper bound on the rendered
    /// payload — the Evictor drops entries until the rendered map
    /// fits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<u32>,
    /// When `true` (default), the rendered map is returned in
    /// [`RecallResponse::orientation_cache`][crate::query::recall::RecallResponse::orientation_cache].
    /// Set to `false` to update the in-process store without
    /// growing the response payload (useful for warm-up calls).
    #[serde(default = "default_true")]
    pub include_in_response: bool,
    /// When `true` (default), the Distiller runs over the recall
    /// hits and updates the in-process map. Set to `false` to read
    /// the current map without mutating it (useful for inspection).
    #[serde(default = "default_true")]
    pub distill: bool,
}

fn default_true() -> bool {
    true
}

impl OrientationCacheConfig {
    pub fn new() -> Self {
        Self {
            namespace: None,
            token_budget: None,
            include_in_response: true,
            distill: true,
        }
    }
    pub fn with_namespace<S: Into<String>>(mut self, ns: S) -> Self {
        self.namespace = Some(ns.into());
        self
    }
    pub fn with_token_budget(mut self, b: u32) -> Self {
        self.token_budget = Some(b);
        self
    }
    pub fn read_only(mut self) -> Self {
        self.distill = false;
        self
    }
}

impl Default for OrientationCacheConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal entry in a [`ContextMap`].
#[derive(Debug, Clone)]
pub struct Entry {
    pub value: String,
    pub freq: u32,
    pub last_seen_unix: u64,
    pub token_estimate: u32,
}

/// Per-namespace context state held by the [`OrientationCacheStore`].
#[derive(Debug, Default, Clone)]
pub struct ContextMap {
    pub entities: BTreeMap<String, Entry>,
    pub constants: BTreeMap<String, Entry>,
    pub schemas: BTreeMap<String, Entry>,
    pub hit_count: u64,
}

/// In-process per-engine store. Keyed by namespace string.
#[derive(Debug, Default)]
pub struct OrientationCacheStore {
    inner: RwLock<HashMap<String, ContextMap>>,
}

impl OrientationCacheStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Snapshot a namespace without mutating it.
    pub fn snapshot(&self, namespace: &str) -> ContextMap {
        self.inner
            .read()
            .ok()
            .and_then(|guard| guard.get(namespace).cloned())
            .unwrap_or_default()
    }

    /// Number of namespaces currently tracked. Diagnostic only.
    pub fn namespace_count(&self) -> usize {
        self.inner.read().map(|g| g.len()).unwrap_or(0)
    }

    fn with_namespace_mut<F, R>(&self, namespace: &str, f: F) -> R
    where
        F: FnOnce(&mut ContextMap) -> R,
        R: Default,
    {
        match self.inner.write() {
            Ok(mut guard) => {
                let map = guard.entry(namespace.to_string()).or_default();
                f(map)
            }
            Err(_) => R::default(),
        }
    }
}

/// Bounded, serialisable rendering of a [`ContextMap`] returned in
/// the recall response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderedContextMap {
    pub namespace: String,
    pub entities: Vec<RenderedEntry>,
    pub constants: Vec<RenderedEntry>,
    pub schemas: Vec<RenderedEntry>,
    pub token_estimate: u32,
    pub budget: u32,
    pub hit_count: u64,
}

/// One rendered entry of a [`RenderedContextMap`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedEntry {
    pub key: String,
    pub value: String,
    pub freq: u32,
    pub token_estimate: u32,
}

/// Derive a namespace key from `(operator override, org_id, agent_id)`.
pub fn resolve_namespace(
    cfg: &OrientationCacheConfig,
    agent_id: &str,
    org_id: Option<&str>,
) -> String {
    if let Some(ref ns) = cfg.namespace {
        return ns.clone();
    }
    match (org_id, agent_id.is_empty()) {
        (Some(o), false) if !o.is_empty() => format!("{o}:{agent_id}"),
        (Some(o), true) if !o.is_empty() => o.to_string(),
        (_, false) => agent_id.to_string(),
        _ => GLOBAL_NAMESPACE.to_string(),
    }
}

/// Distiller output bucketed by knowledge kind.
#[derive(Debug, Default)]
pub struct DistillerOutput {
    pub entities: Vec<(String, String)>,
    pub constants: Vec<(String, String)>,
    pub schemas: Vec<(String, String)>,
}

/// Heuristic distiller. Pure-Rust, regex-free, deterministic.
///
/// - **Entities:** sequences of capitalized whitespace-separated
///   tokens (length ≥ 3) anchored on a leading capital letter.
/// - **Constants:** `UPPER_SNAKE_CASE` token followed by `=` or
///   `:` and a non-whitespace value.
/// - **Schemas:** fenced ```` ``` ```` blocks (any language) and
///   lines beginning with `CREATE TABLE` / `interface ` / `type ` /
///   `struct `.
pub fn distill(content: &str) -> DistillerOutput {
    DistillerOutput {
        entities: extract_entities(content),
        constants: extract_constants(content),
        schemas: extract_schemas(content),
    }
}

fn extract_entities(content: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let push = |cur: &mut Vec<&str>, out: &mut Vec<(String, String)>| {
        if !cur.is_empty() {
            let phrase = cur.join(" ");
            if phrase.len() >= 3 {
                out.push((phrase.clone(), phrase));
            }
            cur.clear();
        }
    };
    for raw_tok in content.split(|c: char| c.is_whitespace() || matches!(c, ',' | '.' | ';')) {
        let tok = raw_tok.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-');
        if is_entity_token(tok) {
            current.push(tok);
        } else {
            push(&mut current, &mut out);
        }
    }
    push(&mut current, &mut out);
    // De-duplicate while preserving order.
    let mut seen = std::collections::BTreeSet::new();
    out.retain(|(k, _)| seen.insert(k.clone()));
    out
}

fn is_entity_token(tok: &str) -> bool {
    if tok.len() < 2 {
        return false;
    }
    let mut chars = tok.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_uppercase() {
        return false;
    }
    // Allow CamelCase / PascalCase / leading-cap words.
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

fn extract_constants(content: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    // Token-by-token scan. Splits on whitespace + common punctuation
    // (commas, semicolons, parens, quotes) so a mid-line
    // `Foo uses API_BASE = https://api.example.com` reduces to the
    // token sequence `["Foo", "uses", "API_BASE", "=", "https://..."]`
    // and we match the `UPPER_SNAKE` + `[:=]` + value triple.
    let tokens: Vec<&str> = content
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '(' | ')' | '\'' | '"' | '`'))
        .filter(|t| !t.is_empty())
        .collect();
    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i];
        // Bare `UPPER_SNAKE` key — value is in the next 1-2 tokens.
        if is_const_key(tok) {
            if i + 2 < tokens.len() && is_separator(tokens[i + 1]) && !is_separator(tokens[i + 2]) {
                push_const(&mut out, &mut seen, tok, tokens[i + 2]);
                i += 3;
                continue;
            }
            if i + 1 < tokens.len() {
                let next = tokens[i + 1];
                if let Some(val) = next.strip_prefix('=').or_else(|| next.strip_prefix(':'))
                    && !val.is_empty()
                {
                    push_const(&mut out, &mut seen, tok, val);
                    i += 2;
                    continue;
                }
            }
        } else if let Some((key, val)) = tok.split_once('=').or_else(|| tok.split_once(':')) {
            // Glued forms: `KEY=value`, `KEY:value`, or `KEY:` /
            // `KEY=` with the value in the next token.
            if is_const_key(key) {
                if !val.is_empty() {
                    push_const(&mut out, &mut seen, key, val);
                } else if i + 1 < tokens.len() && !is_separator(tokens[i + 1]) {
                    push_const(&mut out, &mut seen, key, tokens[i + 1]);
                    i += 2;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

fn is_const_key(s: &str) -> bool {
    s.len() >= 3
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && s.chars().any(|c| c.is_ascii_uppercase())
}

fn is_separator(s: &str) -> bool {
    matches!(s, "=" | ":" | "==" | "=>" | "->")
}

fn push_const(
    out: &mut Vec<(String, String)>,
    seen: &mut std::collections::BTreeSet<String>,
    key: &str,
    value: &str,
) {
    if seen.insert(key.to_string()) {
        out.push((key.to_string(), value.to_string()));
    }
}

fn extract_schemas(content: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    // Fenced code blocks.
    let mut in_fence = false;
    let mut fence_buf = String::new();
    let mut fence_lang = String::new();
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("```") {
            if in_fence {
                let key = if fence_lang.is_empty() {
                    format!("fenced:{}", fence_buf.lines().next().unwrap_or("").trim())
                } else {
                    format!("fenced:{fence_lang}")
                };
                let value = truncate_value(&fence_buf, 240);
                if !value.is_empty() {
                    out.push((key, value));
                }
                fence_buf.clear();
                fence_lang.clear();
                in_fence = false;
            } else {
                fence_lang = rest.trim().to_string();
                in_fence = true;
            }
            continue;
        }
        if in_fence {
            fence_buf.push_str(line);
            fence_buf.push('\n');
            continue;
        }
        // Schema-shaped declarations on a single line.
        let t = line.trim_start();
        let leading: Option<&str> = ["CREATE TABLE", "interface ", "type ", "struct "]
            .into_iter()
            .find(|p| t.starts_with(p));
        if let Some(prefix) = leading {
            let key = format!("decl:{}", prefix.trim_end());
            let value = truncate_value(t, 240);
            out.push((key, value));
        }
    }
    // Flush an unclosed fence so partial schemas still register.
    if in_fence && !fence_buf.is_empty() {
        let key = if fence_lang.is_empty() {
            "fenced:unclosed".to_string()
        } else {
            format!("fenced:{fence_lang}:unclosed")
        };
        out.push((key, truncate_value(&fence_buf, 240)));
    }
    let mut seen = std::collections::BTreeSet::new();
    out.retain(|(k, _)| seen.insert(k.clone()));
    out
}

fn truncate_value(s: &str, max_chars: usize) -> String {
    let mut buf = String::with_capacity(max_chars.min(s.len()));
    for (i, c) in s.chars().enumerate() {
        if i >= max_chars {
            buf.push('…');
            break;
        }
        buf.push(c);
    }
    buf
}

fn merge_into(bucket: &mut BTreeMap<String, Entry>, items: Vec<(String, String)>, now_unix: u64) {
    for (k, v) in items {
        let entry = bucket.entry(k.clone()).or_insert_with(|| Entry {
            value: v.clone(),
            freq: 0,
            last_seen_unix: now_unix,
            token_estimate: estimate_tokens(&format!("{k}: {v}")),
        });
        entry.freq = entry.freq.saturating_add(1);
        entry.last_seen_unix = now_unix;
        if entry.value != v {
            entry.value = v.clone();
            entry.token_estimate = estimate_tokens(&format!("{k}: {v}"));
        }
    }
}

/// Score = freq × recency_weight × size_penalty.
/// Higher is better. Used by the Evictor to keep the most useful entries.
fn priority(entry: &Entry, now_unix: u64, budget: u32) -> f64 {
    let age_s = now_unix.saturating_sub(entry.last_seen_unix) as f64;
    let recency = 1.0 / (1.0 + age_s / 86_400.0);
    let size_share = (entry.token_estimate as f64) / (budget.max(1) as f64);
    let size_penalty = (1.0 - size_share).max(0.05);
    (entry.freq as f64) * recency * size_penalty
}

fn evict_to_budget(map: &mut ContextMap, budget: u32, now_unix: u64) {
    // Compute total tokens; if already under budget, no-op.
    let bucket_totals = |m: &ContextMap| -> u32 {
        m.entities.values().map(|e| e.token_estimate).sum::<u32>()
            + m.constants.values().map(|e| e.token_estimate).sum::<u32>()
            + m.schemas.values().map(|e| e.token_estimate).sum::<u32>()
    };
    if bucket_totals(map) <= budget {
        return;
    }
    // Build a flat list of (priority, kind, key) tuples and drop the
    // lowest-priority entry until under budget.
    while bucket_totals(map) > budget {
        let mut candidates: Vec<(f64, u8, String)> = Vec::new();
        for (k, e) in &map.entities {
            candidates.push((priority(e, now_unix, budget), 0, k.clone()));
        }
        for (k, e) in &map.constants {
            candidates.push((priority(e, now_unix, budget), 1, k.clone()));
        }
        for (k, e) in &map.schemas {
            candidates.push((priority(e, now_unix, budget), 2, k.clone()));
        }
        if candidates.is_empty() {
            break;
        }
        candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let (_, kind, key) = candidates.remove(0);
        match kind {
            0 => {
                map.entities.remove(&key);
            }
            1 => {
                map.constants.remove(&key);
            }
            _ => {
                map.schemas.remove(&key);
            }
        }
    }
}

fn render(map: &ContextMap, namespace: &str, budget: u32) -> RenderedContextMap {
    fn render_bucket(bucket: &BTreeMap<String, Entry>) -> Vec<RenderedEntry> {
        bucket
            .iter()
            .map(|(k, e)| RenderedEntry {
                key: k.clone(),
                value: e.value.clone(),
                freq: e.freq,
                token_estimate: e.token_estimate,
            })
            .collect()
    }
    let entities = render_bucket(&map.entities);
    let constants = render_bucket(&map.constants);
    let schemas = render_bucket(&map.schemas);
    let token_estimate = entities.iter().map(|e| e.token_estimate).sum::<u32>()
        + constants.iter().map(|e| e.token_estimate).sum::<u32>()
        + schemas.iter().map(|e| e.token_estimate).sum::<u32>();
    RenderedContextMap {
        namespace: namespace.to_string(),
        entities,
        constants,
        schemas,
        token_estimate,
        budget,
        hit_count: map.hit_count,
    }
}

/// Update the per-namespace map with the hits and return a bounded
/// rendering. Called from `recall::execute` when both
/// [`RecallRequest::orientation_cache`][crate::query::recall::RecallRequest::orientation_cache]
/// is `Some` and the engine has an
/// [`OrientationCacheStore`][OrientationCacheStore] attached.
pub fn update_and_render(
    store: &OrientationCacheStore,
    cfg: &OrientationCacheConfig,
    namespace: &str,
    hits: &[ScoredMemory],
) -> RenderedContextMap {
    let budget = cfg.token_budget.unwrap_or(DEFAULT_TOKEN_BUDGET).max(64);
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    store.with_namespace_mut(namespace, |map| {
        if cfg.distill {
            for hit in hits {
                let out = distill(&hit.content);
                merge_into(&mut map.entities, out.entities, now_unix);
                merge_into(&mut map.constants, out.constants, now_unix);
                merge_into(&mut map.schemas, out.schemas, now_unix);
                map.hit_count = map.hit_count.saturating_add(1);
            }
            evict_to_budget(map, budget, now_unix);
        }
        render(map, namespace, budget)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::memory::{MemoryType, Scope};
    use serde_json::json;
    use uuid::Uuid;

    fn hit(content: &str) -> ScoredMemory {
        ScoredMemory {
            id: Uuid::now_v7(),
            content: content.to_string(),
            agent_id: "a".to_string(),
            memory_type: MemoryType::Episodic,
            scope: Scope::Private,
            importance: 0.5,
            tags: vec![],
            metadata: json!({}),
            score: 1.0,
            access_count: 0,
            created_at: "2026-05-23T00:00:00Z".to_string(),
            updated_at: "2026-05-23T00:00:00Z".to_string(),
            score_breakdown: None,
        }
    }

    #[test]
    fn distill_extracts_entities() {
        let out = distill("The Foo Bar service calls QuxClient in the Frobnicator pipeline.");
        let keys: Vec<&str> = out.entities.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.iter().any(|k| k.contains("Foo Bar")));
        assert!(keys.iter().any(|k| k.contains("QuxClient")));
        assert!(keys.iter().any(|k| k.contains("Frobnicator")));
    }

    #[test]
    fn distill_extracts_uppercase_constants() {
        let out = distill("API_BASE = https://api.example.com\nMAX_RETRIES: 5\nlower = noop");
        let keys: Vec<&str> = out.constants.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"API_BASE"));
        assert!(keys.contains(&"MAX_RETRIES"));
        assert!(!keys.contains(&"lower"));
    }

    #[test]
    fn distill_extracts_fenced_schemas() {
        let content = "preamble\n```sql\nCREATE TABLE users (id BIGINT);\n```\nafter";
        let out = distill(content);
        assert!(out.schemas.iter().any(|(k, _)| k == "fenced:sql"));
    }

    #[test]
    fn distill_extracts_inline_decls() {
        let out = distill("interface Foo { id: number; }\nCREATE TABLE orders (id BIGINT);");
        let keys: Vec<&str> = out.schemas.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.iter().any(|k| k.contains("interface")));
        assert!(keys.iter().any(|k| k.contains("CREATE TABLE")));
    }

    #[test]
    fn namespace_falls_back_to_agent_when_org_missing() {
        let cfg = OrientationCacheConfig::new();
        assert_eq!(resolve_namespace(&cfg, "agent-1", None), "agent-1");
        assert_eq!(
            resolve_namespace(&cfg, "agent-1", Some("acme")),
            "acme:agent-1"
        );
        assert_eq!(resolve_namespace(&cfg, "", None), GLOBAL_NAMESPACE);
    }

    #[test]
    fn explicit_namespace_overrides_derivation() {
        let cfg = OrientationCacheConfig::new().with_namespace("custom");
        assert_eq!(resolve_namespace(&cfg, "agent-1", Some("acme")), "custom");
    }

    #[test]
    fn update_renders_bounded_map_and_grows_hit_count() {
        let store = OrientationCacheStore::new();
        let cfg = OrientationCacheConfig::new();
        let hits = vec![
            hit(
                "Foo Bar uses API_BASE = https://api.example.com\n```sql\nCREATE TABLE x (id BIGINT);\n```",
            ),
            hit("BazQux also depends on MAX_RETRIES: 3 for the Frobnicator pipeline"),
        ];
        let rendered = update_and_render(&store, &cfg, "ns-a", &hits);
        assert_eq!(rendered.namespace, "ns-a");
        assert_eq!(rendered.hit_count, 2);
        assert!(!rendered.entities.is_empty());
        assert!(!rendered.constants.is_empty());
        assert!(!rendered.schemas.is_empty());
        assert!(rendered.token_estimate <= rendered.budget);
    }

    #[test]
    fn budget_evicts_low_priority_entries() {
        let store = OrientationCacheStore::new();
        let cfg = OrientationCacheConfig::new().with_token_budget(64);
        let mut hits = Vec::new();
        for i in 0..30 {
            hits.push(hit(&format!(
                "EntityNumber{i} uses CONST_{i} = value_{i} in schemaword"
            )));
        }
        let rendered = update_and_render(&store, &cfg, "ns-evict", &hits);
        assert!(
            rendered.token_estimate <= rendered.budget,
            "rendered {} exceeded budget {}",
            rendered.token_estimate,
            rendered.budget
        );
        // Some entries were evicted: the rendered map is smaller
        // than the total possible 30 × 2 entries.
        assert!(rendered.entities.len() + rendered.constants.len() < 60);
    }

    #[test]
    fn namespaces_are_isolated() {
        let store = OrientationCacheStore::new();
        let cfg = OrientationCacheConfig::new();
        update_and_render(&store, &cfg, "ns-1", &[hit("Foo Bar = 1\nALPHA = 1")]);
        update_and_render(&store, &cfg, "ns-2", &[hit("Baz Qux = 2\nBETA = 2")]);
        let ns1 = store.snapshot("ns-1");
        let ns2 = store.snapshot("ns-2");
        assert!(!ns1.constants.contains_key("BETA"));
        assert!(!ns2.constants.contains_key("ALPHA"));
        assert_eq!(store.namespace_count(), 2);
    }

    #[test]
    fn read_only_config_does_not_distill() {
        let store = OrientationCacheStore::new();
        let warm = OrientationCacheConfig::new();
        update_and_render(&store, &warm, "ns-r", &[hit("Foo Bar = 1\nALPHA = 1")]);
        let before = store.snapshot("ns-r");
        let read = OrientationCacheConfig::new().read_only();
        let _ = update_and_render(&store, &read, "ns-r", &[hit("Baz Qux = 2\nBETA = 2")]);
        let after = store.snapshot("ns-r");
        assert_eq!(before.entities.len(), after.entities.len());
        assert_eq!(before.constants.len(), after.constants.len());
        assert_eq!(before.hit_count, after.hit_count);
    }

    #[test]
    fn rendered_token_estimate_stays_under_budget_across_many_updates() {
        let store = OrientationCacheStore::new();
        let cfg = OrientationCacheConfig::new().with_token_budget(128);
        for round in 0..50 {
            let h = hit(&format!(
                "RoundEntity{round} has CONSTANT_{round} = val_{round}"
            ));
            let r = update_and_render(&store, &cfg, "ns-rounds", &[h]);
            assert!(r.token_estimate <= r.budget);
        }
    }
}
