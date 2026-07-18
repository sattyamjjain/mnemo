//! EMBER-anchored eval (arXiv:2606.05894): budgeted evidence retention
//! vs naive truncation on a LongMemEval-style fixture.
//!
//! Both arms are handed the same recalled candidate set and the same
//! fixed retained-token budget (8192). We measure **recall@budget** —
//! the fraction of distinct gold facts whose evidence survives the
//! budget — for:
//!
//! - **naive truncation**: concatenate raw chunks in retrieval order,
//!   cut the stream at the token budget (the obvious baseline an agent
//!   uses when it must keep evidence resident in a bounded window); and
//! - **budgeted retention**: pack verbatim evidence capsules
//!   (excerpt + retrieval key) under the same budget, ranked by the
//!   `recency × hit-rate` recoverability heuristic.
//!
//! Because each capsule costs a fraction of a raw chunk, far more
//! distinct gold facts survive the budget — so budgeted recall@budget
//! strictly beats naive truncation. The test asserts that gap, which is
//! the measurable value of the `retained_token_budget` knob. A second
//! test drives the real `engine.recall` path to confirm the knob is
//! wired end-to-end.

use std::sync::Arc;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::query::retained::{
    DEFAULT_EXCERPT_TOKENS, RetentionCandidate, est_tokens, retain_within_budget,
};
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;

const AGENT: &str = "ember-eval-agent";
const BUDGET_TOKENS: usize = 8192;
const N_FACTS: usize = 60;

/// One LongMemEval-style record: a unique gold fact at the front of a
/// larger chunk of session filler.
struct Fixture {
    id: uuid::Uuid,
    gold: String,
    content: String,
}

fn build_fixture() -> Vec<Fixture> {
    (0..N_FACTS)
        .map(|i| {
            let gold = format!("GOLD-{i:03}");
            // Salient fact first, then ~700 chars of session filler — the
            // realistic shape where most of a chunk is noise around one
            // fact. ~700 chars ≈ 175 estimated tokens per raw chunk.
            let content = format!(
                "{gold}: the user's recorded preference number {i} is value-{i}. {}",
                "session filler context that surrounds the salient fact ".repeat(12)
            );
            Fixture {
                id: uuid::Uuid::now_v7(),
                gold,
                content,
            }
        })
        .collect()
}

/// Naive baseline: walk the candidates in retrieval order, append each
/// whole raw chunk, and stop once the running token total would exceed
/// the budget. Returns the count of distinct golds whose text survived.
fn naive_truncation_recall(fixture: &[Fixture], budget: usize) -> usize {
    let mut used = 0usize;
    let mut covered = 0usize;
    for f in fixture {
        let cost = est_tokens(&f.content);
        if used + cost > budget {
            break;
        }
        used += cost;
        if f.content.contains(&f.gold) {
            covered += 1;
        }
    }
    covered
}

#[test]
fn budgeted_retention_beats_naive_truncation_at_8192() {
    let fixture = build_fixture();

    // ----- Naive truncation arm.
    let naive_covered = naive_truncation_recall(&fixture, BUDGET_TOKENS);

    // ----- Budgeted retention arm. Uniform recency/hits here so the win
    // is attributable to capsule compactness, not ranking.
    let candidates: Vec<RetentionCandidate> = fixture
        .iter()
        .map(|f| RetentionCandidate {
            id: f.id,
            content: &f.content,
            access_count: 3,
            age_hours: 24.0,
            retrieval_score: 1.0,
        })
        .collect();
    let report = retain_within_budget(&candidates, BUDGET_TOKENS, DEFAULT_EXCERPT_TOKENS);

    // A gold is "covered" by the budgeted arm iff its verbatim prefix
    // survived in a capsule excerpt (recoverable evidence).
    let budgeted_covered = fixture
        .iter()
        .filter(|f| {
            report
                .capsules
                .iter()
                .any(|c| c.id == f.id && c.excerpt.contains(&f.gold))
        })
        .count();

    let naive_recall = naive_covered as f64 / N_FACTS as f64;
    let budgeted_recall = budgeted_covered as f64 / N_FACTS as f64;

    // F1 against the full gold set (precision = covered / retained items;
    // recall = covered / all golds).
    let budgeted_precision = if report.capsules.is_empty() {
        0.0
    } else {
        budgeted_covered as f64 / report.capsules.len() as f64
    };
    let budgeted_f1 = if budgeted_precision + budgeted_recall == 0.0 {
        0.0
    } else {
        2.0 * budgeted_precision * budgeted_recall / (budgeted_precision + budgeted_recall)
    };

    println!("\n=== EMBER budgeted-evidence-retention eval (arXiv:2606.05894) ===");
    println!("budget = {BUDGET_TOKENS} tokens, {N_FACTS} gold facts");
    println!("| arm               | covered | recall@budget | retained tokens |");
    println!("|-------------------|--------:|--------------:|----------------:|");
    println!(
        "| naive truncation  | {naive_covered:>7} | {naive_recall:>13.3} | {:>15} |",
        "n/a"
    );
    println!(
        "| budgeted capsules | {budgeted_covered:>7} | {budgeted_recall:>13.3} | {:>15} |",
        report.retained_tokens
    );
    println!("budgeted F1 = {budgeted_f1:.3}");

    // The knob's value: budgeted retention strictly covers more gold
    // facts than naive truncation under the same budget.
    assert!(
        budgeted_covered > naive_covered,
        "budgeted ({budgeted_covered}) must beat naive ({naive_covered}) at {BUDGET_TOKENS} tokens"
    );
    // And it never exceeds the budget it was given.
    assert!(report.retained_tokens <= BUDGET_TOKENS);
}

#[tokio::test]
async fn engine_recall_surfaces_retained_evidence() {
    let storage =
        Arc::new(mnemo_core::storage::duckdb::DuckDbStorage::open_in_memory().expect("duckdb"));
    let index = Arc::new(UsearchIndex::new(8).expect("usearch"));
    let embedding = Arc::new(NoopEmbedding::new(8));
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().expect("tantivy"));
    let engine =
        MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None).with_full_text(ft);

    let fixture = build_fixture();
    for f in &fixture {
        let mut req = RememberRequest::new(f.content.clone());
        req.tags = Some(vec!["ember".to_string()]);
        engine.remember(req).await.expect("remember");
    }

    // Lexical (BM25) recall so the no-op embedder stays valid — every fixture
    // fact carries the word "preference", so BM25 returns them all. (The
    // vector-dependent "auto" default now hard-errors under a no-op embedder,
    // v0.5.13; that path is covered by semantic_recall_hard_error.rs.)
    // Baseline recall (no budget) leaves the response unchanged.
    let mut plain = RecallRequest::new("preference".to_string());
    plain.limit = Some(N_FACTS);
    plain.strategy = Some("lexical".to_string());
    let plain_resp = engine.recall(plain).await.expect("recall");
    assert!(plain_resp.retained_evidence.is_none());

    // Budgeted recall surfaces capsules under the cap, additively (the
    // memories list is untouched).
    let mut budgeted = RecallRequest::new("preference".to_string());
    budgeted.limit = Some(N_FACTS);
    budgeted.strategy = Some("lexical".to_string());
    budgeted.retained_token_budget = Some(BUDGET_TOKENS);
    let resp = engine.recall(budgeted).await.expect("recall");

    let report = resp
        .retained_evidence
        .as_ref()
        .expect("retained_evidence present when budget is set");
    assert!(report.retained_tokens <= BUDGET_TOKENS);
    assert!(!report.capsules.is_empty());
    assert_eq!(report.candidates_examined, resp.memories.len());
    // Capsules carry verbatim excerpts + a parseable retrieval key.
    for c in &report.capsules {
        assert_eq!(c.retrieval_key, c.id.to_string());
        assert!(!c.excerpt.is_empty());
    }
    // memories list is unchanged by the additive capsule view.
    assert_eq!(resp.memories.len(), N_FACTS);
}
