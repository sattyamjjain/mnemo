//! Host-side execution path for code-mode recall.
//!
//! The "guest program" is a pre-built sequence of host import calls
//! the LLM-generated wasm would have made. Today the binary in
//! mnemo-cli builds the program from CLI args; tomorrow the wasmtime
//! runner (gated under the `wasm` feature) compiles + executes a
//! real WIT guest. Either way the host-side contract is the same:
//! a [`GuestProgram`] is consumed against the [`MemStore`]-shaped
//! callable, producing a [`RecallBundle`] with the cited memories
//! plus token-cost accounting.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Resource limits the wasm sandbox is parameterised by. Defaults
/// chosen so a runaway guest cannot DOS the host: 10M fuel, 64
/// pages (4 MiB), 50 ms wall.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceBudget {
    pub fuel: u64,
    pub mem_pages: u32,
    pub wall: Duration,
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self {
            fuel: 10_000_000,
            mem_pages: 64,
            wall: Duration::from_millis(50),
        }
    }
}

/// One step a guest program asks the host to run. Mirrors the WIT
/// world's `store` interface (`recall`, `score`, `cite`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RecallStep {
    Recall { query: String, k: u32 },
    Score { memory_id: String },
    Cite { memory_id: String },
}

/// Bundle of host-import calls a guest program asks for. The host
/// runs them in order and records what it returned in
/// [`RecallBundle`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuestProgram {
    pub steps: Vec<RecallStep>,
}

/// What the guest program produces. The CLI hands `final_answer`
/// back to the LLM; the bundle's other fields land in the audit
/// trail so an offline auditor can replay the recall.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallBundle {
    pub recalled: Vec<RecallHit>,
    pub final_answer: String,
    /// Estimated token cost the guest paid talking to the host.
    /// Compare this to [`json_mode_token_estimate`] to show the
    /// savings.
    pub guest_token_cost: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallHit {
    pub id: String,
    pub content: String,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeModeRecall {
    pub program: GuestProgram,
    pub budget: ResourceBudget,
}

#[derive(Debug, Error, PartialEq)]
pub enum CodeModeError {
    #[error("guest fuel exhausted ({budget} units consumed)")]
    Halted { budget: u64 },
    #[error("guest exceeded wall-time budget {budget:?}")]
    WallTimeExceeded { budget: Duration },
    #[error("guest tried to access {capability} which is stripped from the sandbox")]
    SandboxViolation { capability: &'static str },
    #[error("guest emitted no recall steps — refusing an empty bundle")]
    EmptyProgram,
}

/// Trait the host exposes to the guest. Mirrors the WIT
/// `store` interface so swapping in the wasmtime path keeps the
/// same contract.
pub trait HostStore: Send + Sync {
    fn recall(&self, query: &str, k: u32) -> Vec<RecallHit>;
    fn score(&self, memory_id: &str) -> f32;
    fn cite(&self, memory_id: &str) -> String;
}

/// Run a guest program against the host store. The wall-time and
/// fuel budgets are enforced cooperatively on every step; the wasm
/// sandbox enforces them preemptively under the `wasm` feature.
pub fn run_code_mode_host(
    program: &CodeModeRecall,
    store: &dyn HostStore,
) -> Result<RecallBundle, CodeModeError> {
    if program.program.steps.is_empty() {
        return Err(CodeModeError::EmptyProgram);
    }
    let start = std::time::Instant::now();
    let mut fuel_used = 0u64;
    let mut recalled = Vec::new();
    let mut answer_parts = Vec::new();
    for step in &program.program.steps {
        // Each host import costs a fixed fuel quantum. The wasm
        // path will additionally meter wasm instructions; for the
        // host-only path this is enough to catch runaway programs.
        fuel_used = fuel_used.saturating_add(1_000_000);
        if fuel_used > program.budget.fuel {
            return Err(CodeModeError::Halted {
                budget: program.budget.fuel,
            });
        }
        if start.elapsed() > program.budget.wall {
            return Err(CodeModeError::WallTimeExceeded {
                budget: program.budget.wall,
            });
        }
        match step {
            RecallStep::Recall { query, k } => {
                let hits = store.recall(query, *k);
                for h in &hits {
                    answer_parts.push(format!("- {}", h.content));
                }
                recalled.extend(hits);
            }
            RecallStep::Score { memory_id } => {
                let _ = store.score(memory_id);
            }
            RecallStep::Cite { memory_id } => {
                let _ = store.cite(memory_id);
            }
        }
    }
    let final_answer = if answer_parts.is_empty() {
        "(no relevant memories)".to_string()
    } else {
        answer_parts.join("\n")
    };
    let guest_token_cost =
        crate::token::estimate_tokens(&final_answer) + program.program.steps.len() * 4; // ~4 tokens per host call
    Ok(RecallBundle {
        recalled,
        final_answer,
        guest_token_cost,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubStore;
    impl HostStore for StubStore {
        fn recall(&self, q: &str, k: u32) -> Vec<RecallHit> {
            (0..k.min(3))
                .map(|i| RecallHit {
                    id: format!("m{i}"),
                    content: format!("answer to '{q}' #{i}"),
                    score: 1.0 - (i as f32) * 0.1,
                })
                .collect()
        }
        fn score(&self, _: &str) -> f32 {
            0.5
        }
        fn cite(&self, mid: &str) -> String {
            format!("receipt-for-{mid}")
        }
    }

    #[test]
    fn empty_program_is_rejected() {
        let req = CodeModeRecall {
            program: GuestProgram { steps: vec![] },
            budget: ResourceBudget::default(),
        };
        let err = run_code_mode_host(&req, &StubStore).unwrap_err();
        assert_eq!(err, CodeModeError::EmptyProgram);
    }

    #[test]
    fn fuel_exhaust_halts() {
        // Default budget = 10M fuel, each step burns 1M; 12 steps
        // exceeds the budget on step 11 (after fuel_used > 10M).
        let req = CodeModeRecall {
            program: GuestProgram {
                steps: vec![
                    RecallStep::Recall {
                        query: "x".into(),
                        k: 1,
                    };
                    12
                ],
            },
            budget: ResourceBudget::default(),
        };
        let err = run_code_mode_host(&req, &StubStore).unwrap_err();
        assert!(matches!(err, CodeModeError::Halted { .. }));
    }

    #[test]
    fn happy_path_returns_bundle() {
        let req = CodeModeRecall {
            program: GuestProgram {
                steps: vec![RecallStep::Recall {
                    query: "patient fatigue".into(),
                    k: 3,
                }],
            },
            budget: ResourceBudget::default(),
        };
        let bundle = run_code_mode_host(&req, &StubStore).unwrap();
        assert_eq!(bundle.recalled.len(), 3);
        assert!(bundle.final_answer.contains("answer to"));
    }

    #[test]
    fn wall_time_budget_can_be_exceeded() {
        // Budget zero forces an immediate wall-time violation on
        // step 2 (step 1 always completes before the elapsed check).
        let req = CodeModeRecall {
            program: GuestProgram {
                steps: vec![
                    RecallStep::Recall {
                        query: "x".into(),
                        k: 1,
                    };
                    2
                ],
            },
            budget: ResourceBudget {
                wall: Duration::from_nanos(0),
                ..ResourceBudget::default()
            },
        };
        // Sleep a hair so step 2's elapsed > 0.
        std::thread::sleep(Duration::from_millis(1));
        let err = run_code_mode_host(&req, &StubStore).unwrap_err();
        assert!(matches!(err, CodeModeError::WallTimeExceeded { .. }));
    }
}
