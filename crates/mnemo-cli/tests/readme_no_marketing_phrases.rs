//! v0.4.2 (U2) — README marketing-phrase lint.
//!
//! The "Why mnemo when Cloudflare Agent Memory exists?" section is
//! deliberately framed as an honest concession + differentiation pitch.
//! It explicitly cedes edge-recall p50 to Cloudflare and positions
//! mnemo's axis on provenance + chain replay + sovereignty.
//!
//! This test fails the build if any of three marketing phrases shows
//! up in `README.md`. They are the phrases an operator skim-reading
//! the README would translate to "the mnemo authors think they beat
//! Cloudflare on perf" — which would be wrong, and which the v0.4.2
//! prompt explicitly called out as the marketing risk to lint against.

use std::path::Path;

const BANNED_PHRASES: &[&str] = &[
    // v0.4.2 (U2) — Cloudflare-comparison banned framing.
    "beat Cloudflare",
    "faster than Cloudflare",
    "Cloudflare killer",
    // v0.4.3 (A1) — extended canonical banned-phrases ledger. The
    // operator's running policy ("ship one row honestly over five rows
    // aspirationally") rules out every form of viral / breakthrough /
    // game-changing framing in the README. Per-claim primary-source
    // links, honest concessions, and bench numbers are the substrates
    // we promote.
    "blow up",
    "viral",
    "game-changing",
    "game changing",
    "revolutionary",
    "wild",
    "mind-blowing",
    "mind blowing",
    // v0.4.4 (A1) — Project Think positioning. The new section is
    // explicit about loop-vs-ledger being COMPLEMENTARY, not
    // substitute. Block any drift into adversarial framing:
    "competes with Cloudflare",
    "replaces Project Think",
    "Project Think killer",
    "Workers killer",
    // v0.4.4 (U1) — MCP 2026 Roadmap anchor. The README + design-doc
    // reference is explicitly a *spec-context anchor*, not a
    // compliance claim. mnemo is "aligned-by-design with Enterprise
    // Readiness" — one priority of four — not roadmap-compliant.
    // Block compliance-overclaim drift before it ships:
    "MCP 2026 leader",
    "compliant with MCP 2026",
    "MCP 2026 ready",
    "roadmap-compliant",
    // v0.4.4 (2026-05-09 U1) — Anthropic Dreams Research Preview
    // substrate anchor. Dreams API is Research Preview behind a
    // Request-access form; mnemo ships NO Anthropic-API adapter
    // today. The README paragraph is substrate-level interop only.
    // Block adapter-overclaim drift before it ships:
    "Dreams replacement",
    "dream-compatible",
    "Dreams-ready",
    "Dreams competitor",
    "curator killer",
    // v0.4.4 (2026-05-09 U2) — ARGUS provenance composition anchor.
    // ARGUS is a research artifact (arXiv 2605.03378), not a spec
    // and not a product. mnemo's HMAC envelope chain is a
    // *write-side* complement, not a guarantee against any class of
    // injection by construction. Block compositional-security
    // overclaim drift:
    "prompt-injection-proof",
    "provenance-guaranteed",
    "ARGUS-compliant",
    "injection-resistant by construction",
    // v0.4.4 (2026-05-10 UPDATE-1) — DELEGATE-52 outcome-diffing
    // anchor. The DELEGATE-52 paper (arXiv 2604.15597) measures a
    // 25% baseline silent-corruption rate; mnemo's append-only event
    // log is the *substrate* an external diffing tool reads from,
    // not a guarantee against any DELEGATE-52 outcome class. The
    // research doc + example_recalls fixtures explicitly cite these
    // overclaim phrasings as banned; this test enforces it:
    "DELEGATE-52-resistant",
    "outcome-corruption-proof",
    "delegation-safe by construction",
    // v0.4.4 (2026-05-17) — arXiv 2605.15184 grep-vs-vector anchor.
    // The paper measures BM25 outperforming vector on its experiment-1
    // corpus; mnemo's hybrid-RRF default is already hedged, but the
    // anchor is a composition note, not a claim that mnemo is the
    // "best" retriever for any harness. Block adversarial framing:
    "grep killer",
    "vector retrieval is dead",
    "RAG killer",
    "harness-perfect",
    // v0.4.5 (2026-05-20) — arXiv 2605.18226 Context Memorization
    // substrate anchor. mnemo ships the STORE; the producer +
    // consumer of attention-state blobs are out of scope. Block
    // compliance-overclaim drift:
    "Context-Memorization-compliant",
    "attention-state-compatible",
    "KV-cache-portable",
    "prefix-cache by construction",
    // v0.4.6 (2026-05-21) — golem:vector WIT provider host-runner
    // anchor. mnemo ships 3 of 30 upstream WIT functions + the
    // host-runner architecture; the wasmtime-loader wiring is
    // deferred. Block adversarial framing:
    "Golem-durable by construction",
    "golem:vector-compliant",
    "Qdrant killer",
    "Pinecone killer",
    "WIT-component-perfect",
    // v0.4.7 (2026-05-22) — MINTEval / current-fact resolver. The
    // resolver is a post-process over recall results; it is NOT a
    // contradiction detector and NOT a write-side guard. Block
    // compliance-overclaim drift:
    "MINTEval-compliant",
    "interference-proof",
    "supersession-perfect",
    "MINTEval-resistant",
    // v0.4.8 (2026-05-23) — PEEK / orientation cache. The cache is
    // a heuristic post-process over recall results; it is NOT a
    // learned summariser, NOT a context-window extender, NOT a
    // write-side consolidator, and NOT persisted across restarts.
    // Block adversarial framing:
    "PEEK-compliant",
    "PEEK-perfect",
    "context-window-extender",
    "infinite-context",
    "orientation-perfect",
];

#[test]
fn readme_does_not_carry_banned_marketing_phrases() {
    let readme_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("README.md");
    let body =
        std::fs::read_to_string(&readme_path).expect("README.md must be readable from repo root");

    let lower = body.to_lowercase();
    let mut hits: Vec<&'static str> = Vec::new();
    for phrase in BANNED_PHRASES {
        if lower.contains(&phrase.to_lowercase()) {
            hits.push(phrase);
        }
    }

    assert!(
        hits.is_empty(),
        "README.md carries banned marketing phrase(s): {hits:?}. \
         The v0.4.2 Cloudflare-differentiation section must concede \
         edge-recall perf, not claim parity or superiority. See \
         docs/comparisons/cloudflare-agent-memory.md for the framing."
    );
}

#[test]
fn readme_carries_required_cloudflare_section_anchor() {
    // Inverse: confirm the differentiation section is actually present
    // so a future blanket README rewrite can't silently delete it.
    let readme_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("README.md");
    let body = std::fs::read_to_string(&readme_path).expect("README.md must be readable");
    assert!(
        body.contains("Why mnemo when Cloudflare Agent Memory exists?"),
        "README.md must keep the Cloudflare differentiation H2 from v0.4.2 (U2)."
    );
    assert!(
        body.contains("docs/comparisons/cloudflare-agent-memory.md"),
        "README.md must link to docs/comparisons/cloudflare-agent-memory.md."
    );
}
