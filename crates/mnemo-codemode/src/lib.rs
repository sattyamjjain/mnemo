//! v0.4.0 (P0-3) — Code-mode recall.
//!
//! Cloudflare's Code Mode MCP (announced 2026-04-24) showed that
//! agents calling tools through generated host-side code instead of
//! JSON tool envelopes can drop per-turn token cost by ~99.9%. The
//! same shape applies to recall: instead of the LLM paying for
//! `tool_call(recall, {query: "..."})` plus `tool_result([memory,
//! memory, ...])` JSON each turn, the host hands the LLM a
//! sandboxed wasm host whose imports it can call as plain functions.
//!
//! This crate ships:
//!
//! 1. The host-side data shapes [`CodeModeRecall`], [`RecallBundle`],
//!    [`ResourceBudget`].
//! 2. A pure host-side runner [`run_code_mode_host`] that accepts a
//!    pre-built guest "program" (a list of recall calls) and produces
//!    a [`RecallBundle`] — used by the token-budget tests + the
//!    `mnemo recall --code-mode` CLI in the binary crate.
//! 3. The WIT world definition under `wit/mnemo-memory.wit` describing
//!    the import/export surface a real wasm guest must implement.
//!
//! The wasmtime+wasi-stripping path lives behind the `wasm` feature
//! and is wired in a follow-up; the host-side contract is fully
//! tested today and is what mnemo-cli's `recall --code-mode`
//! currently dispatches to.

pub mod runner;
pub mod token;

pub use runner::{
    CodeModeError, CodeModeRecall, GuestProgram, RecallBundle, RecallStep, ResourceBudget,
    run_code_mode_host,
};
pub use token::estimate_tokens;
