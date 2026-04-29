//! v0.4.1 (P0-3) — agent behavioural-baseline exporter.
//!
//! RSAC 2026 (2026-04-26) called out the agentic-SOC telemetry gap:
//! agents emit logs but no normalised behavioural baseline an SOC
//! can alert on. This crate ships the missing surface — a per-agent
//! rolling profile (recall rate, write rate, namespace fanout, tool
//! mix, HMAC continuity) emitted in two canonical schemas:
//!
//! 1. **OpenTelemetry semconv 1.31** — `agent.*` attributes on a
//!    span the operator's existing OTel collector already ingests.
//! 2. **OCSF 1.4 Application Activity** — JSON the SOC's SIEM
//!    pipeline already understands.
//!
//! A z-score + EWMA detector flags drift; the exporter is
//! deliberately **signal, not enforcement** — it does not refuse
//! ops. The README's threat-model section spells that out so a
//! reader doesn't mistake the surface for a policy gate.
//!
//! Anti-leak invariant: emitted payloads never contain memory
//! contents. Only metric aggregates. The unit tests sweep the
//! payload with a regex to assert this.

pub mod anomaly;
pub mod exporter;
pub mod profile;

pub use anomaly::{BaselineDelta, BaselineMetric, Severity};
pub use exporter::{BaselineExporter, JsonExporter};
pub use profile::{AgentBaseline, ToolId};
