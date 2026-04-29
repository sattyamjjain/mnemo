//! v0.4.1 (P0-1) — runner smoke + judge variance + mock-judge round trip.

use std::time::Instant;

use mnemo_locomo_bench::scoring::SliceTag;
use mnemo_locomo_bench::{
    Dialogue, GoldAnswer, JudgeModel, LoCoMoJudge, LoCoMoResult, LoCoMoRun, MockJudge, RecallMode,
};

fn slice_label(id: &str) -> SliceTag {
    match id.chars().next() {
        Some('t') => SliceTag::Temporal,
        Some('m') => SliceTag::MultiSession,
        Some('o') => SliceTag::OpenDomain,
        _ => SliceTag::Other,
    }
}

#[tokio::test]
async fn runner_smoke_5_dialogues_under_90s() {
    let start = Instant::now();
    let _run = LoCoMoRun::new(JudgeModel::Mock, RecallMode::Default, [0u8; 32]);
    let dialogues = vec![
        Dialogue {
            id: "t1".into(),
            turns: vec!["the patient is allergic to penicillin".into()],
        },
        Dialogue {
            id: "m1".into(),
            turns: vec!["session 1: hemoglobin 11.2".into()],
        },
        Dialogue {
            id: "o1".into(),
            turns: vec!["paris is the capital of france".into()],
        },
        Dialogue {
            id: "t2".into(),
            turns: vec!["the visit was three weeks ago".into()],
        },
        Dialogue {
            id: "o2".into(),
            turns: vec!["the meeting is on tuesday".into()],
        },
    ];
    let golds = vec![
        ("t1".to_string(), "penicillin".to_string()),
        ("m1".to_string(), "hemoglobin".to_string()),
        ("o1".to_string(), "paris".to_string()),
        ("t2".to_string(), "three weeks".to_string()),
        ("o2".to_string(), "tuesday".to_string()),
    ];

    let judge = MockJudge;
    let mut verdicts = Vec::new();
    for d in &dialogues {
        let candidate = d.turns.join(" ");
        let gold = golds.iter().find(|(id, _)| id == &d.id).unwrap();
        let g = GoldAnswer {
            id: gold.0.clone(),
            answer: gold.1.clone(),
        };
        let v = judge.score(d, &g, &candidate).await;
        verdicts.push((d.id.clone(), v));
    }
    let result = LoCoMoResult::from_verdicts(&verdicts, slice_label);
    assert_eq!(result.overall.total, 5);
    assert_eq!(result.overall.correct, 5);
    assert!(start.elapsed().as_secs() < 90);
}

#[tokio::test]
async fn judge_variance_under_2pp_on_mock_dataset() {
    // Two judges that agree on everything → 0 pp variance. The real
    // gate uses the GPT-5.1 + Claude-3.7 pair against a 20-dialogue
    // golden slice; this smoke test wires the variance code path so
    // a regression in `record_variance` can't slip past CI.
    let verdicts_a = vec![
        (
            "t1".to_string(),
            mnemo_locomo_bench::JudgeVerdict {
                correct: true,
                confidence: 1.0,
                rationale: "x".into(),
                judge: JudgeModel::Mock,
            },
        ),
        (
            "t2".to_string(),
            mnemo_locomo_bench::JudgeVerdict {
                correct: false,
                confidence: 1.0,
                rationale: "x".into(),
                judge: JudgeModel::Mock,
            },
        ),
    ];
    let mut a = LoCoMoResult::from_verdicts(&verdicts_a, slice_label);
    let b = LoCoMoResult::from_verdicts(&verdicts_a, slice_label);
    a.record_variance(&b);
    assert_eq!(a.overall.variance_pp, 0.0);
    assert!(a.overall.variance_pp <= 2.0);
}

#[test]
fn parity_table_anchor_present_in_readme() {
    let readme =
        std::fs::read_to_string(env!("CARGO_MANIFEST_DIR").to_string() + "/../../README.md")
            .expect("README.md readable");
    // We commit a row labelled `LoCoMo` into the parity table at
    // P0-1 wrap-up. The runner's own crate test asserts the row is
    // there so a future README rewrite can't accidentally drop it.
    assert!(
        readme.contains("LoCoMo"),
        "README should reference LoCoMo benchmark"
    );
}
