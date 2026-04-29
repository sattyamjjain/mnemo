//! OpenTelemetry + OCSF emitters (v0.4.1 P0-3).
//!
//! The OTel side maps to semconv 1.31's `agent.*` attributes; the
//! OCSF side maps to OCSF 1.4 Application Activity (`category_uid`
//! 6, `class_uid` 6004). Operators wire the JSON straight into
//! their existing SIEM pipeline.
//!
//! Anti-leak invariant: emitted payloads contain only metric
//! aggregates — no memory contents, no raw audit rows. The unit
//! tests sweep with a regex to assert this stays true.

use serde::{Deserialize, Serialize};

use crate::profile::AgentBaseline;

pub trait BaselineExporter: Send + Sync {
    /// Emit one OTel-shape JSON envelope.
    fn emit_otel(&self, b: &AgentBaseline) -> serde_json::Value;

    /// Emit one OCSF Application-Activity JSON envelope.
    fn emit_ocsf(&self, b: &AgentBaseline) -> serde_json::Value;
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonExporter;

impl BaselineExporter for JsonExporter {
    fn emit_otel(&self, b: &AgentBaseline) -> serde_json::Value {
        serde_json::json!({
            "name": "mnemo.baseline",
            "kind": "internal",
            "attributes": {
                "agent.id": b.agent,
                "agent.window_secs": b.window.as_secs(),
                "agent.recall_rate_per_min": b.recall_rate_per_min,
                "agent.write_rate_per_min": b.write_rate_per_min,
                "agent.namespace_fanout": b.namespace_fanout,
                "agent.hmac_continuity": b.hmac_continuity,
                "agent.tool_mix_keys": b.tool_mix.keys().cloned().collect::<Vec<_>>(),
            },
        })
    }

    fn emit_ocsf(&self, b: &AgentBaseline) -> serde_json::Value {
        serde_json::json!({
            "category_uid": 6, // Application Activity
            "class_uid": 6004,
            "type_uid": 600401, // Generic
            "activity_id": 1,
            "severity_id": 1,
            "metadata": {
                "version": "1.4.0",
                "product": {
                    "name": "mnemo-baseline",
                    "vendor_name": "mnemo",
                },
            },
            "actor": {
                "user": {
                    "name": b.agent,
                    "type": "Agent",
                },
            },
            "enrichments": [
                {"name": "recall_rate_per_min", "value": b.recall_rate_per_min},
                {"name": "write_rate_per_min", "value": b.write_rate_per_min},
                {"name": "namespace_fanout", "value": b.namespace_fanout},
                {"name": "hmac_continuity", "value": b.hmac_continuity},
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::profile::AgentBaseline;

    fn fake_baseline() -> AgentBaseline {
        let mut b = AgentBaseline::new("agent-prod-42", Duration::from_secs(300));
        b.recall_rate_per_min = 12.0;
        b.write_rate_per_min = 4.5;
        b.namespace_fanout = 2.0;
        b.hmac_continuity = 1.0;
        b.tool_mix.insert("recall".into(), 0.7);
        b.tool_mix.insert("write".into(), 0.3);
        b
    }

    #[test]
    fn otel_payload_carries_agent_attributes() {
        let exp = JsonExporter;
        let v = exp.emit_otel(&fake_baseline());
        assert_eq!(v["name"], "mnemo.baseline");
        assert_eq!(v["attributes"]["agent.id"], "agent-prod-42");
        assert_eq!(v["attributes"]["agent.recall_rate_per_min"], 12.0);
    }

    #[test]
    fn ocsf_payload_validates_against_class_6004() {
        let exp = JsonExporter;
        let v = exp.emit_ocsf(&fake_baseline());
        assert_eq!(v["category_uid"], 6);
        assert_eq!(v["class_uid"], 6004);
        assert_eq!(v["actor"]["user"]["name"], "agent-prod-42");
    }

    #[test]
    fn no_pii_or_memory_content_in_payloads() {
        // Anti-leak invariant: regex-sweep the payloads for anything
        // resembling memory text. Build a baseline whose tool_mix
        // KEYS are short tool names — never memory content. The
        // regex catches accidental field additions that smuggle text.
        let exp = JsonExporter;
        let b = fake_baseline();
        let otel = exp.emit_otel(&b).to_string();
        let ocsf = exp.emit_ocsf(&b).to_string();
        let leak_re =
            regex::Regex::new(r"(?i)(content|body|text|memory_text|raw|payload_text)").unwrap();
        assert!(
            !leak_re.is_match(&otel),
            "OTel payload contains a banned field: {otel}"
        );
        assert!(
            !leak_re.is_match(&ocsf),
            "OCSF payload contains a banned field: {ocsf}"
        );
    }
}
