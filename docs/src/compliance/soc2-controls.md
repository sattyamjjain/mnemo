# SOC 2 Trust Service Criteria -- Control Mapping

This document maps the AICPA SOC 2 Trust Service Criteria (2017 revision) to
Mnemo's security architecture. Each section covers one Common Criteria (CC)
category, lists the relevant controls, describes how Mnemo addresses them, and
identifies any gaps or recommendations.

For background on Mnemo's security features, see the
[Security](../security.md) page and the [Compliance Overview](./README.md).

---

## CC1 -- Control Environment

The control environment sets the tone for the organization, influencing the
control consciousness of its people. It is the foundation for all other
components of internal control.

### CC1.1 -- Commitment to Integrity and Ethical Values

| Field | Detail |
|---|---|
| **Control ID** | CC1.1 |
| **Description** | The entity demonstrates a commitment to integrity and ethical values. |
| **Mnemo Implementation** | Mnemo is an open-source project with public code review. All contributions go through pull request review before merge. The project enforces Rust compiler warnings as errors (`#[deny(warnings)]`) and maintains a comprehensive test suite (67 tests across unit, integration, and MCP layers). |
| **Status** | **Operational** |
| **Gaps / Recommendations** | Formalize a written code of conduct and contributor ethics policy. Document the review and approval process for security-sensitive changes. |

### CC1.2 -- Board Oversight

| Field | Detail |
|---|---|
| **Control ID** | CC1.2 |
| **Description** | The board of directors demonstrates independence from management and exercises oversight. |
| **Mnemo Implementation** | As a software component rather than a service organization, Mnemo defers board-level governance to the deploying organization. The project provides tools (audit logs, hash chain verification) that enable oversight. |
| **Status** | **Operational** |
| **Gaps / Recommendations** | Deploying organizations should establish governance committees with visibility into Mnemo audit logs and verification reports. |

### CC1.3 -- Management Structure and Authority

| Field | Detail |
|---|---|
| **Control ID** | CC1.3 |
| **Description** | Management establishes structures, reporting lines, and appropriate authorities and responsibilities. |
| **Mnemo Implementation** | Mnemo's RBAC model (`crates/mnemo-core/src/model/acl.rs`) implements a six-level permission hierarchy: Read, Write, Delete, Share, Delegate, Admin. Each permission level satisfies all lower levels. Principal types include Agent, User, Org, Role, and Public. The delegation model (`crates/mnemo-core/src/model/delegation.rs`) enforces maximum transitive depth and scoped authority. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | None. The hierarchical permission model maps well to organizational authority structures. |

### CC1.4 -- Competence Commitment

| Field | Detail |
|---|---|
| **Control ID** | CC1.4 |
| **Description** | The entity demonstrates a commitment to attract, develop, and retain competent individuals. |
| **Mnemo Implementation** | Mnemo is written in Rust, which enforces memory safety at compile time. The project uses type-safe error handling (`crate::error::Error`), preventing entire categories of runtime bugs. CI/CD pipelines run the full test suite on every commit. |
| **Status** | **Operational** |
| **Gaps / Recommendations** | Document onboarding procedures for new contributors, including security review training. |

### CC1.5 -- Accountability

| Field | Detail |
|---|---|
| **Control ID** | CC1.5 |
| **Description** | The entity holds individuals accountable for their internal control responsibilities. |
| **Mnemo Implementation** | Every memory operation is attributed to an `agent_id`. The `AgentEvent` log (`crates/mnemo-core/src/model/event.rs`) records the agent, thread, timestamp, and event type for every action. The `created_by` field on `MemoryRecord` tracks who created each memory. Delegation records track both `delegator_id` and `delegate_id`. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | None. Attribution is comprehensive across all data operations. |

---

## CC2 -- Communication and Information

The entity uses relevant, quality information to support the functioning of
internal control and communicates information internally and externally.

### CC2.1 -- Information Quality

| Field | Detail |
|---|---|
| **Control ID** | CC2.1 |
| **Description** | The entity obtains or generates and uses relevant, quality information to support the functioning of internal control. |
| **Mnemo Implementation** | The `AgentEvent` model captures 15 distinct event types covering all data lifecycle operations: `MemoryWrite`, `MemoryRead`, `MemoryDelete`, `MemoryShare`, `Checkpoint`, `Branch`, `Merge`, `UserMessage`, `AssistantMessage`, `ToolCall`, `ToolResult`, `Error`, `RetrievalQuery`, `RetrievalResult`, `Decision`. Each event includes OpenTelemetry fields (`trace_id`, `span_id`, `model`, `tokens_input`, `tokens_output`, `latency_ms`, `cost_usd`) for observability. Events are hash-chained (`content_hash`, `prev_hash`) for integrity. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | Consider adding structured log export (e.g., to SIEM systems) for centralized monitoring. |

### CC2.2 -- Internal Communication

| Field | Detail |
|---|---|
| **Control ID** | CC2.2 |
| **Description** | The entity internally communicates information necessary to support the functioning of internal control. |
| **Mnemo Implementation** | The event log is queryable via `list_events()`, `get_events_by_thread()`, and `list_child_events()` on the `StorageBackend` trait. The `mnemo.verify` MCP tool allows any authorized agent to verify hash chain integrity and report anomalies. Memory poisoning detection results include detailed `reasons` arrays explaining each anomaly factor. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | Add webhook or notification support for critical events (quarantine triggers, chain verification failures). |

### CC2.3 -- External Communication

| Field | Detail |
|---|---|
| **Control ID** | CC2.3 |
| **Description** | The entity communicates with external parties regarding matters affecting the functioning of internal control. |
| **Mnemo Implementation** | Mnemo provides a REST API and MCP protocol interface for external integration. Audit events can be retrieved programmatically. The Python SDK, TypeScript SDK, and Go SDK enable external systems to consume compliance-relevant data. |
| **Status** | **Partially Implemented** |
| **Gaps / Recommendations** | Implement dedicated compliance reporting endpoints that export audit data in standard formats (e.g., CEF, OCSF). Add support for external audit log forwarding. |

---

## CC3 -- Risk Assessment

The entity identifies and assesses risks to the achievement of its objectives,
including risks related to fraud.

### CC3.1 -- Objective Specification

| Field | Detail |
|---|---|
| **Control ID** | CC3.1 |
| **Description** | The entity specifies objectives with sufficient clarity to enable the identification and assessment of risks. |
| **Mnemo Implementation** | Mnemo defines clear security objectives through its data model: memory confidentiality (encryption, scoping), integrity (hash chains, content hashes), availability (TTL management, checkpoint/restore). Each memory has explicit `scope` (Private, Shared, Public, Global) and `importance` scoring. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | None. |

### CC3.2 -- Risk Identification and Analysis

| Field | Detail |
|---|---|
| **Control ID** | CC3.2 |
| **Description** | The entity identifies risks to the achievement of its objectives and analyzes risks as a basis for determining how the risks should be managed. |
| **Mnemo Implementation** | The memory poisoning detection system (`crates/mnemo-core/src/query/poisoning.rs`) implements multi-factor anomaly scoring. Three risk indicators are evaluated for every memory write: (1) importance deviation from agent baseline (>0.4 deviation = +0.3 score), (2) content length deviation from agent average (>5x or <0.1x = +0.3 score), (3) high-frequency burst detection (rapid writes = +0.4 score). A composite score >= 0.5 triggers anomaly classification. Agent behavioral baselines are maintained in `AgentProfile` records (`crates/mnemo-core/src/model/agent_profile.rs`) with running averages of importance, content length, and total memory count. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | Consider adding configurable thresholds per agent or organization. Add support for custom anomaly detection rules. |

### CC3.3 -- Fraud Risk Assessment

| Field | Detail |
|---|---|
| **Control ID** | CC3.3 |
| **Description** | The entity considers the potential for fraud in assessing risks. |
| **Mnemo Implementation** | Memory poisoning detection directly addresses the risk of agents injecting malicious or misleading memories. The quarantine mechanism (`MemoryRecord.quarantined`, `MemoryRecord.quarantine_reason`) isolates suspicious memories from recall results. The hash chain prevents retrospective tampering with the historical record. The delegation model prevents privilege escalation through `max_depth` limits and time bounds on delegated permissions. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | Add alerting on repeated quarantine events from a single agent (potential coordinated attack). Consider implementing agent reputation scoring. |

### CC3.4 -- Change-Related Risk Assessment

| Field | Detail |
|---|---|
| **Control ID** | CC3.4 |
| **Description** | The entity identifies and assesses changes that could significantly impact the system of internal controls. |
| **Mnemo Implementation** | The checkpoint/branch/merge system (`crates/mnemo-core/src/model/checkpoint.rs`) provides git-like versioning for agent state. Every checkpoint captures a `state_snapshot`, optional `state_diff`, `memory_refs`, and `event_cursor`. The `version` and `prev_version_id` fields on `MemoryRecord` track all changes to individual memories. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | None. Change tracking is comprehensive. |

---

## CC5 -- Control Activities

The entity selects and develops control activities that contribute to the
mitigation of risks to the achievement of objectives to acceptable levels.

### CC5.1 -- Selection of Control Activities

| Field | Detail |
|---|---|
| **Control ID** | CC5.1 |
| **Description** | The entity selects and develops control activities that contribute to the mitigation of risks. |
| **Mnemo Implementation** | Mnemo implements defense in depth through multiple layered controls: encryption at rest, hash chain integrity, RBAC with hierarchical permissions, ACL-based sharing, scoped delegation, anomaly detection, quarantine, and TTL-based expiration. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | None. Multiple overlapping controls provide robust risk mitigation. |

### CC5.2 -- Technology-Based Control Activities

| Field | Detail |
|---|---|
| **Control ID** | CC5.2 |
| **Description** | The entity selects and develops general control activities over technology. |
| **Mnemo Implementation** | Access control is enforced at the storage layer through the `StorageBackend` trait. Key methods include: `check_permission(memory_id, principal_id, required_permission)` for ACL enforcement, `check_delegation(delegate_id, memory_id, required_permission)` for delegation enforcement, and `list_accessible_memory_ids(agent_id, limit)` for permission-safe vector search. The permission hierarchy (`Permission::satisfies()`) ensures that higher-level permissions automatically grant lower-level access. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | None. |

### CC5.3 -- Deployment of Control Activities Through Policies

| Field | Detail |
|---|---|
| **Control ID** | CC5.3 |
| **Description** | The entity deploys control activities through policies that establish what is expected and in procedures that put policies into action. |
| **Mnemo Implementation** | Access policies are encoded in the data model: each `Acl` record specifies `principal_type` (Agent, Org, Public, User, Role), `principal_id`, `permission` level, `granted_by`, and optional `expires_at`. Delegation policies specify `scope` (AllMemories, ByTag, ByMemoryId), `max_depth`, `current_depth`, and `expires_at`. Memory scope (Private, Shared, Public, Global) sets the default visibility policy. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | Add organization-level default policies that apply to all agents within an org. |

---

## CC6 -- Logical and Physical Access Controls

The entity implements logical access security software, infrastructure, and
architectures over protected information assets.

### CC6.1 -- Logical Access Security

| Field | Detail |
|---|---|
| **Control ID** | CC6.1 |
| **Description** | The entity implements logical access security over protected information assets. |
| **Mnemo Implementation** | Three-tier access control: (1) Owner access -- the creating agent has full control. (2) ACL-based sharing -- explicit grants with specified permission levels and optional expiration. (3) Delegation -- transitive permission chains with depth limits and time bounds. All access checks are performed at the storage layer before data is returned. The `list_accessible_memory_ids()` method ensures that vector similarity search only returns memories the requesting agent is authorized to see. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | None. |

### CC6.2 -- Authentication and Authorization

| Field | Detail |
|---|---|
| **Control ID** | CC6.2 |
| **Description** | Prior to issuing system credentials and granting system access, the entity registers and authorizes new users. |
| **Mnemo Implementation** | Agent identity is established through the `agent_id` field present on all operations. In MCP mode, the agent identity is bound to the STDIO transport session. The permission system supports five principal types (Agent, User, Org, Role, Public) with hierarchical authorization. |
| **Status** | **Partially Implemented** |
| **Gaps / Recommendations** | Implement formal agent registration and credential management. Add support for authentication tokens or API keys. Consider integration with external identity providers (OIDC, SAML). |

### CC6.3 -- Data Encryption

| Field | Detail |
|---|---|
| **Control ID** | CC6.3 |
| **Description** | The entity protects data in transit and at rest using encryption. |
| **Mnemo Implementation** | **At rest:** The `ContentEncryption` module (`crates/mnemo-core/src/encryption.rs`) provides AES-256-based content encryption. Keys are 256-bit (32 bytes), loaded from the `MNEMO_ENCRYPTION_KEY` environment variable or provided directly as hex-encoded strings. Each encryption operation produces `nonce \|\| ciphertext \|\| tag` with a 12-byte nonce and 16-byte HMAC integrity tag. Decryption verifies the tag before returning plaintext, detecting any tampering. **In transit:** When deployed with PostgreSQL mode, TLS is recommended. The Docker deployment guide recommends reverse proxy with TLS termination. |
| **Status** | **Partially Implemented** |
| **Gaps / Recommendations** | Upgrade the encryption implementation from the current simplified XOR-based cipher to the `aes-gcm` crate for production-grade AES-256-GCM (the code contains a comment noting this: "In production, use `aes-gcm` crate"). Implement key rotation support. Add envelope encryption for per-record keys. Enforce TLS for all network transports. |

### CC6.4 -- Restriction of Physical Access

| Field | Detail |
|---|---|
| **Control ID** | CC6.4 |
| **Description** | The entity restricts physical access to facilities and protected information assets. |
| **Mnemo Implementation** | As a software component, Mnemo defers physical access controls to the deployment environment. The Docker deployment (`Dockerfile`, `docker-compose.yml`) uses a non-root container image based on `debian:bookworm-slim`. The data volume (`/data`) can be mounted with appropriate filesystem permissions. |
| **Status** | **Operational** |
| **Gaps / Recommendations** | Document recommended filesystem permissions for the data volume. Provide Kubernetes deployment guidance with pod security policies and network policies. |

### CC6.5 -- Disposal of Information Assets

| Field | Detail |
|---|---|
| **Control ID** | CC6.5 |
| **Description** | The entity disposes of protected information assets in a secure manner. |
| **Mnemo Implementation** | Mnemo implements both soft delete (`soft_delete_memory`) and hard delete (`hard_delete_memory`) operations. Soft delete sets `deleted_at` timestamp, preserving the record for audit purposes. Hard delete permanently removes the record from storage. The `cleanup_expired()` method removes memories past their TTL. Cognitive forgetting (`lifecycle.rs`) provides decay-based archival and forgetting with configurable thresholds. Consolidation states track the full lifecycle: Raw, Active, Pending, Consolidated, Archived, Forgotten. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | Add secure wipe (zeroing) for hard-deleted records to prevent forensic recovery. Document data retention policies and destruction schedules. |

### CC6.6 -- Protection Against External Threats

| Field | Detail |
|---|---|
| **Control ID** | CC6.6 |
| **Description** | The entity implements controls to prevent or detect and act upon the introduction of unauthorized or malicious software. |
| **Mnemo Implementation** | Memory poisoning detection (`crates/mnemo-core/src/query/poisoning.rs`) monitors all incoming memories against agent behavioral baselines. Anomalous memories are automatically quarantined. The hash chain prevents injection of fabricated historical records. Content hashing detects any post-insertion modification. Source type tracking (`SourceType` enum with 9 variants) identifies the provenance of each memory. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | Add content validation rules (e.g., maximum content length, prohibited patterns). Consider integrating with external threat intelligence feeds. |

---

## CC7 -- System Operations

The entity uses detection and monitoring procedures to identify changes to
configurations and system components that may indicate an attack.

### CC7.1 -- Detection of System Changes

| Field | Detail |
|---|---|
| **Control ID** | CC7.1 |
| **Description** | The entity detects changes to system components and configurations. |
| **Mnemo Implementation** | The hash chain verification system (`crates/mnemo-core/src/hash.rs`) enables detection of any tampering with stored memories. `verify_chain()` iterates through all records, verifying both content hashes and chain linkage. The `ChainVerificationResult` reports: `valid` (boolean), `total_records`, `verified_records`, `first_broken_at` (UUID of first tampered record), and `error_message`. The `mnemo.verify` MCP tool exposes this capability to agents. The checkpoint system tracks state changes with `state_diff` fields. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | Add automated periodic verification (cron-based or event-triggered). Implement alerting on verification failures. |

### CC7.2 -- Monitoring for Anomalies

| Field | Detail |
|---|---|
| **Control ID** | CC7.2 |
| **Description** | The entity monitors system components and operations for anomalies indicative of malicious acts, natural disasters, or errors. |
| **Mnemo Implementation** | Anomaly detection runs on every memory write via `check_for_anomaly()`. Three indicators are scored: importance deviation (+0.3), content length deviation (+0.3), and burst frequency (+0.4). The `AnomalyCheckResult` struct provides `is_anomalous`, `score`, and detailed `reasons`. Agent profiles (`AgentProfile`) track running averages to establish baselines. The event log captures all operations with timestamps and OpenTelemetry correlation IDs for distributed tracing. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | Add configurable anomaly thresholds. Implement time-series anomaly detection for longer-term behavioral drift. Export metrics to Prometheus or similar monitoring systems. |

### CC7.3 -- Evaluation and Response

| Field | Detail |
|---|---|
| **Control ID** | CC7.3 |
| **Description** | The entity evaluates anomalies to determine whether they represent security events and responds accordingly. |
| **Mnemo Implementation** | When a memory scores >= 0.5 on the anomaly scale, it is automatically quarantined via `quarantine_memory()`. Quarantined memories have `quarantined = true` and `quarantine_reason` set with the specific anomaly details. Quarantined memories are excluded from recall results, preventing poisoned data from affecting agent behavior. The agent profile is updated after each write via `update_agent_profile()` to refine baselines. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | Add a quarantine review workflow allowing administrators to release or permanently delete quarantined memories. Implement escalation procedures for repeated anomalies from the same agent. |

### CC7.4 -- Incident Response

| Field | Detail |
|---|---|
| **Control ID** | CC7.4 |
| **Description** | The entity responds to identified security incidents. |
| **Mnemo Implementation** | The delegation revocation mechanism (`revoke_delegation()`) enables immediate access termination. The event log provides a complete forensic trail. Hash chain verification can identify the exact point of any data tampering. Checkpoint restore enables rollback to a known-good state. |
| **Status** | **Partially Implemented** |
| **Gaps / Recommendations** | Implement a formal incident response runbook. Add bulk quarantine and bulk revocation capabilities. Create forensic export tools for incident investigation. |

---

## CC8 -- Change Management

The entity authorizes, designs, develops, configures, documents, tests,
approves, and implements changes to infrastructure and software.

### CC8.1 -- Change Authorization

| Field | Detail |
|---|---|
| **Control ID** | CC8.1 |
| **Description** | The entity authorizes, designs, develops, tests, and implements changes to meet its objectives. |
| **Mnemo Implementation** | The checkpoint/branch/merge system provides version control for agent state. Key features: `checkpoint` -- captures a point-in-time snapshot with `state_snapshot`, `state_diff`, `memory_refs`, `event_cursor`, and optional `label`. `branch` -- creates a named branch from a checkpoint (`branch_name` field, `parent_id` linking). `merge` -- combines branch state back into the main line. `replay` -- replays events from a checkpoint forward. Every `MemoryRecord` tracks `version` (incrementing integer) and `prev_version_id` (UUID linking to the prior version). The `mnemo.checkpoint`, `mnemo.branch`, `mnemo.merge`, and `mnemo.replay` MCP tools expose these capabilities. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | Add merge conflict detection and resolution strategies. Implement branch protection rules. |

### CC8.2 -- Testing of Changes

| Field | Detail |
|---|---|
| **Control ID** | CC8.2 |
| **Description** | The entity tests changes before implementation. |
| **Mnemo Implementation** | The project maintains 67 tests across three layers: 46 unit tests, 16 integration tests, and 5 MCP protocol tests. Criterion benchmarks (`benches/engine_bench.rs`) track performance regressions. The branching system allows agents to test changes on a branch before merging into the main line. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | Add security-specific test suites (fuzzing, property-based testing). Implement CI gates that block merges on test failures. |

### CC8.3 -- Change Documentation

| Field | Detail |
|---|---|
| **Control ID** | CC8.3 |
| **Description** | The entity documents changes to meet its objectives. |
| **Mnemo Implementation** | Every state change is documented through the event log (`AgentEvent`). The checkpoint system captures `state_diff` fields showing what changed between checkpoints. Memory versioning (`version`, `prev_version_id`) creates a complete change history for every record. The consolidation state machine (Raw -> Active -> Pending -> Consolidated -> Archived -> Forgotten) tracks lifecycle transitions. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | None. Change documentation is thorough and machine-readable. |

---

## CC9 -- Risk Mitigation

The entity identifies, selects, and develops risk mitigation activities.

### CC9.1 -- Risk Mitigation Selection

| Field | Detail |
|---|---|
| **Control ID** | CC9.1 |
| **Description** | The entity identifies, selects, and develops risk mitigation activities. |
| **Mnemo Implementation** | Mnemo provides a comprehensive set of risk mitigation controls: **TTL enforcement** -- memories with `expires_at` are automatically excluded from recall results and cleaned up by `cleanup_expired()`. **Quarantine** -- anomalous memories are isolated from the data pool. **Cognitive forgetting** -- the Ebbinghaus-inspired decay model (`lifecycle.rs`) automatically reduces the importance of aging memories through configurable functions (Exponential, Linear, StepFunction, PowerLaw). `run_decay_pass()` archives or forgets memories below configurable thresholds. **Delegation bounds** -- `max_depth` prevents infinite permission chains, `expires_at` ensures time-limited grants, `DelegationScope` restricts access to specific memories or tags. |
| **Status** | **Implemented** |
| **Gaps / Recommendations** | None. Multiple complementary mitigation strategies are available. |

### CC9.2 -- Vendor and Business Partner Risk

| Field | Detail |
|---|---|
| **Control ID** | CC9.2 |
| **Description** | The entity assesses and manages risks associated with vendors and business partners. |
| **Mnemo Implementation** | Mnemo tracks the source of every memory via `SourceType` (Agent, Human, System, UserInput, ToolOutput, ModelResponse, Retrieval, Consolidation, Import) and `source_id`. The `created_by` field identifies the creating entity. The poisoning detection system applies equally to memories from all sources, including external imports. |
| **Status** | **Partially Implemented** |
| **Gaps / Recommendations** | Add vendor/source trust levels with different anomaly thresholds. Implement source allowlisting for import operations. |

---

## Summary Matrix

| CC Category | Status | Key Modules |
|---|---|---|
| CC1 -- Control Environment | Implemented / Operational | `acl.rs`, `delegation.rs`, `event.rs` |
| CC2 -- Communication and Information | Implemented | `event.rs`, `hash.rs`, StorageBackend |
| CC3 -- Risk Assessment | Implemented | `poisoning.rs`, `agent_profile.rs`, `checkpoint.rs` |
| CC5 -- Control Activities | Implemented | `acl.rs`, `delegation.rs`, StorageBackend |
| CC6 -- Logical and Physical Access | Partially Implemented | `encryption.rs`, `acl.rs`, `delegation.rs` |
| CC7 -- System Operations | Implemented | `hash.rs`, `poisoning.rs`, `event.rs` |
| CC8 -- Change Management | Implemented | `checkpoint.rs`, `event.rs`, MemoryRecord versioning |
| CC9 -- Risk Mitigation | Implemented | `lifecycle.rs`, `poisoning.rs`, `delegation.rs` |

## Priority Gaps

The following items represent the highest-priority gaps for achieving full SOC 2
compliance. They are listed in recommended order of implementation:

1. **Upgrade encryption to production-grade AES-256-GCM** (CC6.3) -- Replace
   the simplified XOR cipher with the `aes-gcm` crate. This is the most
   critical gap.

2. **Implement formal authentication** (CC6.2) -- Add agent registration,
   API key management, and external identity provider integration.

3. **Add automated hash chain verification** (CC7.1) -- Schedule periodic
   verification runs with alerting on failures.

4. **Implement incident response tooling** (CC7.4) -- Build forensic export,
   bulk quarantine, and bulk revocation capabilities.

5. **Add compliance reporting endpoints** (CC2.3) -- Export audit data in
   standard formats for external consumption.
