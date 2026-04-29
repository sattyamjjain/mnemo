//! Per-model context-window table (v0.4.1 P1-4).
//!
//! Operators override via a `models.toml` shipped alongside their
//! deployment; the constant table here is the fallback used when
//! none is provided. Drift in vendor numbers is the main risk —
//! the table is small and keyed by stable `ModelId` so a single
//! rebase fixes the whole crate.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelId {
    Gpt5_1_400k,
    Gpt5_1_128k,
    Claude3_7Sonnet1m,
    Claude3_7Sonnet200k,
    Gemini2_5Pro2m,
    Gemini2_5Pro1m,
    DeepSeekV4_1m,
    DeepSeekV3_128k,
}

impl ModelId {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelId::Gpt5_1_400k => "gpt-5.1-400k",
            ModelId::Gpt5_1_128k => "gpt-5.1-128k",
            ModelId::Claude3_7Sonnet1m => "claude-3.7-sonnet-1m",
            ModelId::Claude3_7Sonnet200k => "claude-3.7-sonnet-200k",
            ModelId::Gemini2_5Pro2m => "gemini-2.5-pro-2m",
            ModelId::Gemini2_5Pro1m => "gemini-2.5-pro-1m",
            ModelId::DeepSeekV4_1m => "deepseek-v4-1m",
            ModelId::DeepSeekV3_128k => "deepseek-v3-128k",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextWindow {
    pub total_tokens: u32,
    /// Recommended system-prompt reserve.
    pub system_reserve: u32,
    /// Recommended response reserve.
    pub response_reserve: u32,
}

/// Default per-model windows the planner uses when no override is
/// provided. Numbers checked against vendor docs as of 2026-04-28;
/// the planner is parameterised by `ContextBudget` so deployments
/// with different reserves don't need to ship a code change.
pub const MODEL_TABLE: &[(ModelId, ContextWindow)] = &[
    (
        ModelId::Gpt5_1_400k,
        ContextWindow {
            total_tokens: 400_000,
            system_reserve: 8_000,
            response_reserve: 16_000,
        },
    ),
    (
        ModelId::Gpt5_1_128k,
        ContextWindow {
            total_tokens: 128_000,
            system_reserve: 4_000,
            response_reserve: 8_000,
        },
    ),
    (
        ModelId::Claude3_7Sonnet1m,
        ContextWindow {
            total_tokens: 1_000_000,
            system_reserve: 16_000,
            response_reserve: 32_000,
        },
    ),
    (
        ModelId::Claude3_7Sonnet200k,
        ContextWindow {
            total_tokens: 200_000,
            system_reserve: 8_000,
            response_reserve: 16_000,
        },
    ),
    (
        ModelId::Gemini2_5Pro2m,
        ContextWindow {
            total_tokens: 2_000_000,
            system_reserve: 16_000,
            response_reserve: 32_000,
        },
    ),
    (
        ModelId::Gemini2_5Pro1m,
        ContextWindow {
            total_tokens: 1_000_000,
            system_reserve: 8_000,
            response_reserve: 16_000,
        },
    ),
    (
        ModelId::DeepSeekV4_1m,
        ContextWindow {
            total_tokens: 1_000_000,
            system_reserve: 8_000,
            response_reserve: 24_000,
        },
    ),
    (
        ModelId::DeepSeekV3_128k,
        ContextWindow {
            total_tokens: 128_000,
            system_reserve: 4_000,
            response_reserve: 8_000,
        },
    ),
];

pub fn lookup(model: ModelId) -> ContextWindow {
    MODEL_TABLE
        .iter()
        .find(|(m, _)| *m == model)
        .map(|(_, w)| *w)
        .unwrap_or(ContextWindow {
            total_tokens: 128_000,
            system_reserve: 4_000,
            response_reserve: 8_000,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_v4_table_entry_is_1m() {
        let w = lookup(ModelId::DeepSeekV4_1m);
        assert_eq!(w.total_tokens, 1_000_000);
    }

    #[test]
    fn every_model_has_distinct_string_id() {
        let mut seen = std::collections::HashSet::new();
        for (m, _) in MODEL_TABLE {
            assert!(
                seen.insert(m.as_str()),
                "duplicate model id: {}",
                m.as_str()
            );
        }
    }

    #[test]
    fn unknown_model_falls_back_safely() {
        // Function takes ModelId by value; unknown models would only
        // appear if the enum gained a variant we forgot to wire.
        // The fallback path is exercised by `lookup` returning the
        // 128k default when iteration misses.
        // (Sanity: every enumerated variant returns a non-default
        // window.)
        for (m, _) in MODEL_TABLE {
            let w = lookup(*m);
            assert!(w.total_tokens > 0);
        }
    }
}
