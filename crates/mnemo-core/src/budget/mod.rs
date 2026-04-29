//! v0.4.1 (P1-4) — 1M-context recall budget planner.
//!
//! With DeepSeek V4 1M (2026-04-24), Claude 3.7 Sonnet 1M, and
//! Gemini 2.5 Pro 2M, the question shifts from "how do I retrieve"
//! to "how do I budget". Stuffing 1M tokens of memory into a 1M
//! context window leaves no room for system prompt + history +
//! response — the planner has to allocate.
//!
//! This module ships:
//!
//! 1. [`models::ModelId`] + a per-model context-window table
//!    (`deepseek-v4-1m`, `claude-3.7-sonnet-1m`, `gpt-5.1-400k`,
//!    `gemini-2.5-pro-2m`, plus the older 200k/128k entries).
//! 2. [`planner::ContextBudget`] — total + system + response +
//!    history reserves.
//! 3. [`planner::plan_recall`] — given a budget + history token
//!    count + query, return a [`planner::RecallPlan`] with `k`,
//!    `chunk_tokens`, `dedup_radius`, and a typed
//!    [`planner::FallbackStrategy`].

pub mod models;
pub mod planner;

pub use models::{ContextWindow, MODEL_TABLE, ModelId};
pub use planner::{ContextBudget, FallbackStrategy, RecallPlan, plan_recall};
