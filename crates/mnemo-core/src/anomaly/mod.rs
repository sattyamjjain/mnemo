//! Embedding-space anomaly detection.
//!
//! The MINJA-class indirect-injection detector in `query::poisoning` catches
//! self-referential instruction markers via lexical rules. That covers the
//! explicit attack surface from arXiv:2503.03704 but misses adversarial
//! rewrites that preserve semantics while drifting the embedding away from
//! the agent's normal distribution. This module adds a z-score outlier
//! gate over the embedding space as a complement — not a replacement —
//! scoped per agent and per source tier.
//!
//! The gate is off by default and only runs when a trained
//! [`crate::model::embedding_baseline::EmbeddingBaseline`] exists for the
//! agent and [`crate::query::poisoning::PoisoningPolicy::outlier_threshold`]
//! is set.

pub mod outlier;
