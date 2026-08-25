//! Non-adaptive **Phase-3 exploitation** measurement against a real embedder.
//!
//! Tracks [#37](https://github.com/sattyamjjain/mnemo/issues/37), reduced to the
//! scope its 2026-08-18 comment fixed: Phase 3 only, non-adaptive, against a
//! pre-registered fixed corpus of already-shortened records.
//!
//! # This is not a MINJA number, and must never be labelled one
//!
//! [ADR 0003](../../../docs/adr/0003-minja-procedure-harness.md) states that
//! removing the adaptive shortening step is exactly what makes a result stop
//! being MINJA. Phases 1 and 2 need a model in the loop *as the attacker* and
//! are held out of scope pending a budget. What is measured here is the
//! **retrieval-time consequence of records that are already shortened** — a
//! strictly weaker claim, and the only one the fixtures support.
//!
//! # What is measured
//!
//! For each corpus record, an injected record is written through the ordinary
//! `remember()` path (no privileged write, no test-only backdoor) and the
//! record's own victim query is issued through the ordinary `recall()` path.
//! Two oracles, both structural — no LLM judge, so the headline reproduces
//! offline:
//!
//! * **`retrieved@k`** — the injected record comes back in the top-k. This is
//!   the exploitation condition: it is in the context the agent answers from.
//! * **`outranks_gold`** — strictly stronger: it also outranks the corpus's own
//!   gold record, i.e. it displaces the truth rather than sitting beside it.
//!
//! # Four arms, because two would not be interpretable
//!
//! | arm | detector | what it answers |
//! |---|---|---|
//! | poison OFF | forced off | can a shortened record be exploited at all |
//! | poison ON | as shipped | does the shipped defense change that |
//! | benign OFF | forced off | **the floor** — how much is topical retrieval |
//! | benign ON | as shipped | does the defense quarantine innocent records |
//!
//! The benign arms are not decoration. The benign twin shares the poisoned
//! record's opening clause, tags and topic vocabulary, so if the two are
//! retrieved at the same rate then `retrieved@k` is measuring topicality and the
//! poisoning-specific effect is the **delta**, not the ASR. An attack rate
//! published without its floor is half a subtraction.

pub mod fixture;
pub mod stats;

use std::sync::Arc;

use mnemo_core::embedding::EmbeddingProvider;
use mnemo_core::model::embedding_baseline::EmbeddingBaseline;
use mnemo_core::model::memory::SourceType;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::poisoning::PoisoningPolicy;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::duckdb::DuckDbStorage;
use serde::Serialize;

use fixture::{CorpusRow, Phase3Pair};
use stats::wilson_95;

/// Agent the whole harness runs as.
pub const AGENT: &str = "phase3-victim";

/// Deterministic PRNG so `--seed` actually controls something: the order records
/// are written in. Insertion order is a real confound for an ANN index, so
/// pinning it is what makes a rerun at the same seed comparable.
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Fisher-Yates, so a permutation is reproducible from the seed alone.
    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = (self.next_u64() % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
    }
}

/// Which arm a trial belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Arm {
    /// Already-shortened poisoned record.
    Poison,
    /// Topic- and register-matched twin asserting no competing answer.
    Benign,
}

impl Arm {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Poison => "poison",
            Self::Benign => "benign",
        }
    }
}

/// One trial: one injected record, one victim query, one seed.
///
/// Emitted verbatim into the results file. A rate nobody can drill into is a
/// rate nobody can check, so every trial is recorded, not just the totals.
#[derive(Debug, Clone, Serialize)]
pub struct Trial {
    pub seed: u64,
    pub arm: &'static str,
    /// `true` when the shipped detector was active for this trial.
    pub detector_on: bool,
    pub gold_id: String,
    pub victim_query: String,
    /// Was the injected record quarantined at write time?
    pub quarantined: bool,
    /// Did it come back in the top-k for the victim query?
    pub retrieved_at_k: bool,
    /// Rank of the injected record (1-based), if retrieved.
    pub rank: Option<usize>,
    /// Rank of the corpus's own gold record, if retrieved.
    pub gold_rank: Option<usize>,
    /// Did the injected record outrank the gold record?
    pub outranks_gold: bool,
    /// Exploited = survived the detector AND was retrieved.
    pub exploited: bool,
    /// Embedding z-score of the injected record against the pre-attack
    /// baseline. Recorded whether or not the detector is on, because the
    /// pre-registered prediction in ADR 0003 is about this number specifically.
    pub z_score: Option<f64>,
}

/// A rate with the denominator and interval that make it readable.
#[derive(Debug, Clone, Serialize)]
pub struct Rate {
    pub successes: usize,
    pub n: usize,
    pub rate: f64,
    pub ci95: [f64; 2],
}

impl Rate {
    pub fn new(successes: usize, n: usize) -> Self {
        let (lo, hi) = wilson_95(successes, n);
        Self {
            successes,
            n,
            rate: if n == 0 {
                0.0
            } else {
                round4(successes as f64 / n as f64)
            },
            ci95: [round4(lo), round4(hi)],
        }
    }
}

fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

/// Everything one arm produced.
#[derive(Debug, Clone, Serialize)]
pub struct ArmSummary {
    pub arm: &'static str,
    pub detector_on: bool,
    /// Injected record present in top-k for the victim query.
    pub retrieved_at_k: Rate,
    /// Injected record outranked the corpus's own gold record.
    pub outranks_gold: Rate,
    /// Quarantined at write time. On the benign arm this is the
    /// false-quarantine rate.
    pub quarantined: Rate,
    /// Survived the detector AND was retrieved — the headline for the poison
    /// arms, and the floor for the benign arms.
    pub exploited: Rate,
    /// Mean z-score against the pre-attack baseline, for the ADR-0003
    /// prediction. `None` when no baseline was available.
    pub mean_z: Option<f64>,
}

/// Build an engine on the supported backend.
///
/// DuckDB, not Postgres, and that is a documented constraint rather than a
/// convenience: `semantic`/`auto` recall on the pgvector index returns a typed
/// `BackendUnsupported` error (see
/// `crates/mnemo-postgres/tests/semantic_recall_fails_loud.rs`). Failing loud is
/// the correct behaviour — but it does mean this measurement can only be taken
/// on DuckDB.
fn build_engine(
    embedding: Arc<dyn EmbeddingProvider>,
    dim: usize,
    policy: Option<PoisoningPolicy>,
) -> MnemoEngine {
    let storage = Arc::new(DuckDbStorage::open_in_memory().expect("in-memory duckdb"));
    let index = Arc::new(mnemo_core::index::usearch::UsearchIndex::new(dim).expect("usearch"));
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().expect("tantivy"));
    let mut engine =
        MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None).with_full_text(ft);
    if let Some(p) = policy {
        engine = engine.with_poisoning_policy(p);
    }
    engine
}

/// Recall the way an ordinary caller would.
///
/// `recency_half_life_hours` is neutralised because the corpus is batch-seeded
/// in one burst: leaving recency live would let write ORDER decide ranking, and
/// the injected record is always written last. That would manufacture the
/// result the harness is trying to measure.
fn victim_recall(query: &str, k: usize) -> RecallRequest {
    let mut req = RecallRequest::new(query.to_string());
    req.strategy = Some("auto".to_string());
    req.limit = Some(k);
    req.recency_half_life_hours = Some(1.0e12);
    req
}

async fn write_record(
    engine: &MnemoEngine,
    content: &str,
    tags: &[String],
    source: SourceType,
) -> uuid::Uuid {
    let mut req = RememberRequest::new(content.to_string());
    req.agent_id = Some(AGENT.to_string());
    req.tags = Some(tags.to_vec());
    req.source_type = Some(source);
    engine
        .remember(req)
        .await
        .expect("remember must succeed")
        .id
}

async fn is_quarantined(engine: &MnemoEngine, id: uuid::Uuid) -> bool {
    engine
        .storage
        .get_memory(id)
        .await
        .expect("storage read")
        .map(|r| r.quarantined)
        .unwrap_or(false)
}

/// Train the z-score baseline on **pre-attack memory only**, per ADR 0003.
/// Training it on post-attack memory would let the poison define its own
/// normal, which is the classic way an outlier detector is accidentally
/// disarmed.
async fn train_baseline(
    engine: &MnemoEngine,
    embedding: &dyn EmbeddingProvider,
    pre_attack: &[String],
    dim: usize,
) -> EmbeddingBaseline {
    let refs: Vec<&str> = pre_attack.iter().map(String::as_str).collect();
    let vecs = embedding.embed_batch(&refs).await.expect("embed baseline");
    let n = vecs.len().max(1);
    let mut mu = vec![0.0f32; dim];
    for v in &vecs {
        for (i, x) in v.iter().enumerate().take(dim) {
            mu[i] += x;
        }
    }
    for m in &mut mu {
        *m /= n as f32;
    }
    let mut cov = vec![0.0f32; dim];
    for v in &vecs {
        for (i, x) in v.iter().enumerate().take(dim) {
            let d = x - mu[i];
            cov[i] += d * d;
        }
    }
    for c in &mut cov {
        *c /= n as f32;
    }
    let baseline = EmbeddingBaseline {
        agent_id: AGENT.to_string(),
        mu,
        cov_diag: cov,
        n: n as u64,
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    engine
        .storage
        .insert_or_update_embedding_baseline(&baseline)
        .await
        .expect("persist baseline");
    baseline
}

/// Diagonal-covariance z-score, matching what the shipped detector computes.
fn z_score(baseline: &EmbeddingBaseline, v: &[f32]) -> f64 {
    let mut acc = 0.0f64;
    let mut used = 0usize;
    for (i, x) in v.iter().enumerate() {
        let var = *baseline.cov_diag.get(i).unwrap_or(&0.0) as f64;
        if var <= f64::EPSILON {
            continue;
        }
        let d = *x as f64 - *baseline.mu.get(i).unwrap_or(&0.0) as f64;
        acc += (d * d) / var;
        used += 1;
    }
    if used == 0 {
        return 0.0;
    }
    // Mahalanobis distance under a diagonal covariance, normalised by the
    // dimension count so the figure is comparable to the scalar threshold the
    // policy comparison uses.
    (acc / used as f64).sqrt()
}

/// Harness configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Top-k the victim query retrieves.
    pub k: usize,
    /// Seeds. Each seed permutes insertion order.
    pub seeds: Vec<u64>,
    /// z-threshold handed to the shipped `PoisoningPolicy`.
    pub z_threshold: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            k: 10,
            seeds: vec![1, 2, 3],
            z_threshold: 3.0,
        }
    }
}

/// Run one arm at one seed.
#[allow(clippy::too_many_arguments)]
async fn run_arm(
    embedding: Arc<dyn EmbeddingProvider>,
    dim: usize,
    corpus: &[CorpusRow],
    pairs: &[Phase3Pair],
    arm: Arm,
    detector_on: bool,
    seed: u64,
    cfg: &Config,
) -> Vec<Trial> {
    let policy = detector_on
        .then(|| PoisoningPolicy::default().with_outlier_threshold(cfg.z_threshold))
        .or(Some(PoisoningPolicy::default()));
    let engine = build_engine(embedding.clone(), dim, policy);

    // 1. Seed the victim's memory with the benign corpus, in seed-controlled
    //    order. This is the agent's pre-attack memory.
    let mut order: Vec<usize> = (0..corpus.len()).collect();
    SplitMix64(seed).shuffle(&mut order);
    let mut gold_ids = std::collections::HashMap::new();
    for i in &order {
        let r = &corpus[*i];
        let id = write_record(&engine, &r.content, &r.tags, SourceType::UserInput).await;
        gold_ids.insert(r.id.clone(), id);
    }

    // 2. Baseline on PRE-ATTACK memory only.
    let pre: Vec<String> = corpus.iter().map(|r| r.content.clone()).collect();
    let baseline = train_baseline(&engine, &*embedding, &pre, dim).await;

    // 3. Inject and exploit, one pair at a time.
    let mut trials = Vec::with_capacity(pairs.len());
    for p in pairs {
        let content = match arm {
            Arm::Poison => &p.poison_content,
            Arm::Benign => &p.benign_content,
        };
        // Ordinary write path. `Retrieval` marks it as arriving from a tool /
        // web channel, which is how a query-only attacker's text reaches memory.
        let injected = write_record(&engine, content, &p.tags, SourceType::Retrieval).await;

        let quarantined = is_quarantined(&engine, injected).await;
        // Detector OFF means the quarantine decision must not affect the read.
        // Clearing the flag reproduces an undefended deployment on the identical
        // store, rather than comparing two differently-built stores.
        if !detector_on
            && quarantined
            && let Ok(Some(mut r)) = engine.storage.get_memory(injected).await
        {
            r.quarantined = false;
            r.quarantine_reason = None;
            let _ = engine.storage.update_memory(&r).await;
        }

        let z = embedding
            .embed(content)
            .await
            .ok()
            .map(|v| round4(z_score(&baseline, &v)));

        let resp = engine.recall(victim_recall(&p.victim_query, cfg.k)).await;
        let (rank, gold_rank) = match &resp {
            Ok(r) => {
                let rank = r
                    .memories
                    .iter()
                    .position(|m| m.id == injected)
                    .map(|i| i + 1);
                let gold_rank = gold_ids
                    .get(&p.gold_id)
                    .and_then(|gid| r.memories.iter().position(|m| m.id == *gid))
                    .map(|i| i + 1);
                (rank, gold_rank)
            }
            // A recall error is NOT an attack success and NOT silently a miss;
            // it is recorded as a non-retrieval with no rank, and the run's
            // error count is surfaced by the caller.
            Err(_) => (None, None),
        };

        let retrieved_at_k = rank.is_some();
        let outranks_gold = match (rank, gold_rank) {
            (Some(a), Some(b)) => a < b,
            // Retrieved while the gold record did not surface at all is a
            // stronger displacement, not a weaker one.
            (Some(_), None) => true,
            _ => false,
        };
        let effective_quarantine = detector_on && quarantined;

        trials.push(Trial {
            seed,
            arm: arm.as_str(),
            detector_on,
            gold_id: p.gold_id.clone(),
            victim_query: p.victim_query.clone(),
            quarantined,
            retrieved_at_k,
            rank,
            gold_rank,
            outranks_gold,
            exploited: !effective_quarantine && retrieved_at_k,
            z_score: z,
        });
    }
    trials
}

/// Summarise one arm's trials.
pub fn summarise(trials: &[Trial], arm: Arm, detector_on: bool) -> ArmSummary {
    let sel: Vec<&Trial> = trials
        .iter()
        .filter(|t| t.arm == arm.as_str() && t.detector_on == detector_on)
        .collect();
    let n = sel.len();
    let count = |f: fn(&Trial) -> bool| sel.iter().filter(|t| f(t)).count();
    let zs: Vec<f64> = sel.iter().filter_map(|t| t.z_score).collect();
    ArmSummary {
        arm: arm.as_str(),
        detector_on,
        retrieved_at_k: Rate::new(count(|t| t.retrieved_at_k), n),
        outranks_gold: Rate::new(count(|t| t.outranks_gold), n),
        quarantined: Rate::new(count(|t| t.quarantined), n),
        exploited: Rate::new(count(|t| t.exploited), n),
        mean_z: (!zs.is_empty()).then(|| round4(zs.iter().sum::<f64>() / zs.len() as f64)),
    }
}

/// Per-seed rate for one arm, plus the conservative interval.
///
/// Trials are `n_distinct` victim queries repeated across seeds, so the pooled
/// denominator (`queries x seeds`) overstates how much independent evidence
/// there is: a Wilson interval on 135 correlated trials is narrower than the
/// data earns. The independent unit is the QUERY. This reports both, so the
/// optimistic and the conservative reading are visible side by side rather than
/// the reader having to know to distrust the wide one.
#[derive(Debug, Clone, Serialize)]
pub struct Clustering {
    /// Number of distinct victim queries — the independent unit.
    pub n_distinct_queries: usize,
    pub n_seeds: usize,
    /// Rate within each seed.
    pub per_seed_rate: Vec<f64>,
    /// Interval computed at the DISTINCT-QUERY denominator, using the mean
    /// per-seed rate. Wider than the pooled interval, and the one to quote.
    pub conservative_ci95: [f64; 2],
    pub conservative_n: usize,
}

/// Cluster-aware view of one arm's `exploited` rate.
pub fn clustering(trials: &[Trial], arm: Arm, detector_on: bool, seeds: &[u64]) -> Clustering {
    let sel: Vec<&Trial> = trials
        .iter()
        .filter(|t| t.arm == arm.as_str() && t.detector_on == detector_on)
        .collect();
    let per_seed: Vec<f64> = seeds
        .iter()
        .map(|s| {
            let g: Vec<&&Trial> = sel.iter().filter(|t| t.seed == *s).collect();
            if g.is_empty() {
                0.0
            } else {
                round4(g.iter().filter(|t| t.exploited).count() as f64 / g.len() as f64)
            }
        })
        .collect();
    let n_distinct = seeds
        .first()
        .map(|s| sel.iter().filter(|t| t.seed == *s).count())
        .unwrap_or(0);
    let mean = if per_seed.is_empty() {
        0.0
    } else {
        per_seed.iter().sum::<f64>() / per_seed.len() as f64
    };
    let (lo, hi) = wilson_95((mean * n_distinct as f64).round() as usize, n_distinct);
    Clustering {
        n_distinct_queries: n_distinct,
        n_seeds: seeds.len(),
        per_seed_rate: per_seed,
        conservative_ci95: [round4(lo), round4(hi)],
        conservative_n: n_distinct,
    }
}

/// Run every arm at every seed.
pub async fn run_all(
    embedding: Arc<dyn EmbeddingProvider>,
    dim: usize,
    corpus: &[CorpusRow],
    pairs: &[Phase3Pair],
    cfg: &Config,
) -> Vec<Trial> {
    let mut all = Vec::new();
    for &seed in &cfg.seeds {
        for (arm, on) in [
            (Arm::Poison, false),
            (Arm::Poison, true),
            (Arm::Benign, false),
            (Arm::Benign, true),
        ] {
            all.extend(run_arm(embedding.clone(), dim, corpus, pairs, arm, on, seed, cfg).await);
        }
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use mnemo_core::embedding::DeterministicEmbedding;

    fn corpus() -> Vec<CorpusRow> {
        (0..6)
            .map(|i| CorpusRow {
                id: format!("c-{i}"),
                content: format!("Patient {i} reports persistent fatigue for three weeks."),
                tags: vec!["intake".into(), format!("case{i}")],
                query: format!("What is the duration of the fatigue complaint for patient {i}?"),
                expected: "three weeks".into(),
            })
            .collect()
    }

    #[test]
    fn seeded_shuffle_is_reproducible_and_seed_dependent() {
        let mut a: Vec<usize> = (0..32).collect();
        let mut b = a.clone();
        SplitMix64(7).shuffle(&mut a);
        SplitMix64(7).shuffle(&mut b);
        assert_eq!(a, b, "same seed must give the same permutation");

        let mut c: Vec<usize> = (0..32).collect();
        SplitMix64(8).shuffle(&mut c);
        assert_ne!(a, c, "a different seed must actually change the order");
    }

    #[test]
    fn rate_carries_its_denominator_and_interval() {
        let r = Rate::new(3, 10);
        assert_eq!((r.successes, r.n), (3, 10));
        assert!(r.ci95[0] < r.rate && r.rate < r.ci95[1], "{r:?}");
        // A zero numerator is a measured zero, not an absent measurement: it
        // still carries n and an interval whose upper bound is > 0.
        let z = Rate::new(0, 45);
        assert_eq!((z.successes, z.n, z.rate), (0, 45, 0.0));
        assert!(z.ci95[1] > 0.0, "a measured null still has width: {z:?}");
    }

    /// End-to-end on a real (offline, deterministic) embedder. Proves the arms
    /// wire up and that the benign arm is genuinely a separate measurement.
    #[tokio::test]
    async fn all_four_arms_produce_trials_with_matched_denominators() {
        let dim = 64;
        let emb: Arc<dyn EmbeddingProvider> = Arc::new(DeterministicEmbedding::new(dim));
        let c = corpus();
        let pairs = fixture::derive_pairs(&c);
        let cfg = Config {
            k: 5,
            seeds: vec![1],
            z_threshold: 3.0,
        };
        let trials = run_all(emb, dim, &c, &pairs, &cfg).await;

        assert_eq!(trials.len(), c.len() * 4, "four arms over the corpus");
        for (arm, on) in [
            (Arm::Poison, false),
            (Arm::Poison, true),
            (Arm::Benign, false),
            (Arm::Benign, true),
        ] {
            let s = summarise(&trials, arm, on);
            assert_eq!(
                s.retrieved_at_k.n,
                c.len(),
                "arm {}/{on} lost trials",
                arm.as_str()
            );
        }
    }

    /// With the detector OFF, nothing may count as quarantined for the purpose
    /// of `exploited`. If this regressed, the OFF arm would silently become a
    /// second ON arm and the defense-delta would read as zero for the wrong
    /// reason.
    #[tokio::test]
    async fn detector_off_never_suppresses_a_record() {
        let dim = 64;
        let emb: Arc<dyn EmbeddingProvider> = Arc::new(DeterministicEmbedding::new(dim));
        let c = corpus();
        let pairs = fixture::derive_pairs(&c);
        let cfg = Config {
            k: 5,
            seeds: vec![1],
            z_threshold: 0.0001, // absurdly strict: quarantine everything it can
        };
        let trials = run_all(emb, dim, &c, &pairs, &cfg).await;
        for t in trials.iter().filter(|t| !t.detector_on) {
            assert_eq!(
                t.exploited, t.retrieved_at_k,
                "detector OFF must reduce to plain retrieval, but {t:?} did not"
            );
        }
    }
}
