# HIPAA Safeguards -- Control Mapping

This document maps the HIPAA Security Rule safeguards (45 CFR Part 164, Subpart
C) to Mnemo's security architecture. It is intended for organizations that
deploy Mnemo in environments where Protected Health Information (PHI) may be
stored as agent memories.

HIPAA compliance is a shared responsibility between Mnemo (as the software
component) and the deploying organization (as the covered entity or business
associate). This document identifies which safeguards Mnemo addresses through
its architecture and which require operational controls from the deploying
organization.

For background on Mnemo's security features, see the
[Security](../security.md) page and the [Compliance Overview](./README.md).

---

## Administrative Safeguards (Section 164.308)

Administrative safeguards are administrative actions, policies, and procedures
to manage the selection, development, implementation, and maintenance of
security measures to protect ePHI.

### 164.308(a)(1) -- Security Management Process

**Requirement:** Implement policies and procedures to prevent, detect, contain,
and correct security violations.

#### (i) Risk Analysis (Required)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.308(a)(1)(ii)(A) |
| **Requirement** | Conduct an accurate and thorough assessment of the potential risks and vulnerabilities to the confidentiality, integrity, and availability of ePHI. |
| **Mnemo Implementation** | Mnemo provides built-in risk analysis capabilities through the memory poisoning detection system (`crates/mnemo-core/src/query/poisoning.rs`). The `check_for_anomaly()` function evaluates three risk vectors for every memory write: importance deviation from agent baseline, content length anomalies, and high-frequency burst detection. Agent behavioral profiles (`AgentProfile`) are maintained with running averages to establish baselines. The anomaly scoring system produces quantified risk assessments (`AnomalyCheckResult` with `score` and `reasons`). |
| **Status** | **Partially Implemented** |
| **Gaps** | Mnemo provides automated risk detection for data integrity threats but does not replace a comprehensive organizational risk analysis. Deploying organizations must conduct their own risk assessment covering infrastructure, personnel, and operational risks. |

#### (ii) Risk Management (Required)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.308(a)(1)(ii)(B) |
| **Requirement** | Implement security measures sufficient to reduce risks and vulnerabilities to a reasonable and appropriate level. |
| **Mnemo Implementation** | Mnemo implements multiple security measures: AES-256-based encryption at rest (`encryption.rs`), SHA-256 hash chain integrity verification (`hash.rs`), six-level RBAC (`acl.rs`), scoped delegation with depth limits (`delegation.rs`), automatic quarantine of anomalous memories, TTL enforcement for data retention, and cognitive forgetting for automatic data lifecycle management (`lifecycle.rs`). |
| **Status** | **Implemented** |
| **Gaps** | Encryption implementation should be upgraded to production-grade `aes-gcm` crate. See SOC 2 CC6.3 for details. |

#### (iii) Sanction Policy (Required)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.308(a)(1)(ii)(C) |
| **Requirement** | Apply appropriate sanctions against workforce members who fail to comply with security policies. |
| **Mnemo Implementation** | The delegation model supports revocation (`revoke_delegation()`) to immediately terminate an agent's delegated access. Quarantine isolates suspect agent activity. The event log provides evidence for sanction decisions. |
| **Status** | **Partially Implemented** |
| **Gaps** | Sanction policies are organizational responsibilities. Mnemo provides the enforcement mechanisms but does not define the policies themselves. |

#### (iv) Information System Activity Review (Required)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.308(a)(1)(ii)(D) |
| **Requirement** | Implement procedures to regularly review records of information system activity, such as audit logs, access reports, and security incident tracking reports. |
| **Mnemo Implementation** | The `AgentEvent` log (`crates/mnemo-core/src/model/event.rs`) provides an immutable, hash-chained audit trail. It captures 15 event types covering all data operations. Events include OpenTelemetry fields for correlation. The `StorageBackend` trait provides query methods: `list_events(agent_id, limit, offset)`, `get_events_by_thread(thread_id, limit)`, `list_child_events(parent_event_id, limit)`. The `mnemo.verify` MCP tool enables integrity verification of the event chain. |
| **Status** | **Implemented** |
| **Gaps** | Add scheduled activity review reports and dashboards. Implement automated alerting for suspicious activity patterns. |

---

### 164.308(a)(2) -- Assigned Security Responsibility

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.308(a)(2) |
| **Requirement** | Identify the security official responsible for developing and implementing security policies. |
| **Mnemo Implementation** | Mnemo's permission model supports `Admin`-level principals who have full control over all operations. The `PrincipalType::Role` type enables mapping organizational security roles to Mnemo permissions. |
| **Status** | **Operational** |
| **Gaps** | This is an organizational requirement. Mnemo provides the RBAC infrastructure to support it. The deploying organization must designate a security official and map their role to Mnemo's Admin permission. |

---

### 164.308(a)(3) -- Workforce Security

**Requirement:** Implement policies and procedures to ensure that all members
of the workforce have appropriate access to ePHI.

#### (i) Authorization and/or Supervision (Addressable)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.308(a)(3)(ii)(A) |
| **Requirement** | Implement procedures for the authorization and/or supervision of workforce members who work with ePHI. |
| **Mnemo Implementation** | The three-tier access control model (Owner, ACL, Delegation) ensures that agents only access memories they are authorized for. The `list_accessible_memory_ids()` method on `StorageBackend` enforces this during vector search. Every ACL entry records `granted_by` to track authorization chains. Delegation records track both `delegator_id` and `delegate_id` with `max_depth` and `current_depth` for oversight. |
| **Status** | **Implemented** |
| **Gaps** | None at the application level. |

#### (ii) Workforce Clearance Procedure (Addressable)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.308(a)(3)(ii)(B) |
| **Requirement** | Implement procedures to determine that the access of a workforce member to ePHI is appropriate. |
| **Mnemo Implementation** | The permission hierarchy (`Permission::satisfies()`) enforces that each agent has only the minimum required permission level. Delegation scope (`DelegationScope::AllMemories`, `ByTag`, `ByMemoryId`) restricts access to relevant data subsets. Time-bounded ACLs and delegations (`expires_at`) ensure access is reviewed and renewed. |
| **Status** | **Implemented** |
| **Gaps** | Add periodic access review reports listing all active permissions and delegations per agent. |

#### (iii) Termination Procedures (Addressable)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.308(a)(3)(ii)(C) |
| **Requirement** | Implement procedures for terminating access to ePHI when employment or access is no longer required. |
| **Mnemo Implementation** | Delegation revocation (`revoke_delegation()`) sets `revoked_at` timestamp to immediately terminate delegated access. ACL entries support `expires_at` for automatic expiration. Soft delete (`soft_delete_memory()`) preserves audit history while removing access to the content. |
| **Status** | **Implemented** |
| **Gaps** | Add a bulk access termination API that revokes all permissions for a given agent in a single operation. |

---

### 164.308(a)(4) -- Information Access Management

**Requirement:** Implement policies and procedures for authorizing access to
ePHI.

#### (i) Isolating Health Care Clearinghouse Functions (Required)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.308(a)(4)(ii)(A) |
| **Requirement** | If a health care clearinghouse is part of a larger organization, isolate its functions. |
| **Mnemo Implementation** | Memory scoping (Private, Shared, Public, Global) combined with `org_id` field enables organizational isolation. Multi-tenant deployments can use `org_id` to enforce data separation at the storage layer. |
| **Status** | **Partially Implemented** |
| **Gaps** | Implement strict tenant isolation enforcement at the database level. Add cross-org access prevention in all query paths. |

#### (ii) Access Authorization (Addressable)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.308(a)(4)(ii)(B) |
| **Requirement** | Implement policies and procedures for granting access to ePHI. |
| **Mnemo Implementation** | The `mnemo.share` MCP tool provides explicit access granting. The `mnemo.delegate` MCP tool enables controlled permission delegation. Both record the granting agent and support time bounds. |
| **Status** | **Implemented** |
| **Gaps** | None. |

#### (iii) Access Establishment and Modification (Addressable)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.308(a)(4)(ii)(C) |
| **Requirement** | Implement policies and procedures that establish, document, review, and modify access. |
| **Mnemo Implementation** | All access changes are logged as `AgentEvent` records (`MemoryShare` event type). ACL entries include `created_at` and `expires_at` for temporal tracking. Delegation records include creation time, expiration, and revocation timestamps. |
| **Status** | **Implemented** |
| **Gaps** | None. |

---

### 164.308(a)(5) -- Security Awareness and Training

**Requirement:** Implement a security awareness and training program for all
members of the workforce.

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.308(a)(5) |
| **Requirement** | Security reminders, malicious software protection, log-in monitoring, password management. |
| **Mnemo Implementation** | Mnemo provides documentation on security best practices (see `docs/src/security.md`). The memory poisoning detection system protects against malicious data injection. The event log enables monitoring of all access attempts. |
| **Status** | **Partially Implemented** |
| **Gaps** | This is primarily an organizational requirement. Create deployment-specific security guides for teams handling PHI. Add security warning messages for operations involving high-sensitivity memories. |

---

### 164.308(a)(6) -- Security Incident Procedures

**Requirement:** Implement policies and procedures to address security incidents.

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.308(a)(6)(ii) |
| **Requirement** | Identify and respond to suspected or known security incidents; mitigate harmful effects; document incidents and outcomes. |
| **Mnemo Implementation** | Quarantine mechanism automatically responds to detected anomalies. Hash chain verification (`mnemo.verify`) identifies data tampering incidents. The event log provides a forensic trail for incident investigation. Checkpoint restore enables rollback to pre-incident state. Delegation revocation enables immediate access termination. |
| **Status** | **Partially Implemented** |
| **Gaps** | Implement a formal incident tracking system within Mnemo (incident records, severity levels, resolution status). Add automated incident notification capabilities. |

---

### 164.308(a)(7) -- Contingency Plan

**Requirement:** Establish policies and procedures for responding to an
emergency or other occurrence that damages systems containing ePHI.

#### (i) Data Backup Plan (Required)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.308(a)(7)(ii)(A) |
| **Requirement** | Establish and implement procedures to create and maintain retrievable exact copies of ePHI. |
| **Mnemo Implementation** | The checkpoint system (`crates/mnemo-core/src/model/checkpoint.rs`) creates point-in-time snapshots with `state_snapshot`, `memory_refs`, and `event_cursor`. Checkpoints include `parent_id` for history linking. The `mnemo.checkpoint` MCP tool enables programmatic backup creation. DuckDB storage supports file-level backups of the database file. |
| **Status** | **Partially Implemented** |
| **Gaps** | Implement automated scheduled backups. Add backup verification (restore testing). Implement offsite backup replication. |

#### (ii) Disaster Recovery Plan (Required)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.308(a)(7)(ii)(B) |
| **Requirement** | Establish procedures to restore any loss of data. |
| **Mnemo Implementation** | The `mnemo.replay` MCP tool replays events from a checkpoint to restore state. Branch and merge operations enable state recovery from alternative timelines. The checkpoint system captures sufficient state for full reconstruction. |
| **Status** | **Partially Implemented** |
| **Gaps** | Document formal disaster recovery procedures. Define Recovery Time Objective (RTO) and Recovery Point Objective (RPO). Implement automated recovery testing. |

#### (iii) Emergency Mode Operation Plan (Required)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.308(a)(7)(ii)(C) |
| **Requirement** | Establish procedures to enable continuation of critical business processes during an emergency. |
| **Mnemo Implementation** | Mnemo can operate with a local DuckDB file, enabling standalone operation without network dependencies. The `NoopEmbedding` provider allows operation without external API access. |
| **Status** | **Partially Implemented** |
| **Gaps** | Document emergency operating procedures. Define minimum viable configuration for emergency operation. |

---

### 164.308(a)(8) -- Evaluation

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.308(a)(8) |
| **Requirement** | Perform periodic technical and nontechnical evaluation of security controls. |
| **Mnemo Implementation** | The `mnemo.verify` MCP tool enables on-demand integrity verification. Criterion benchmarks track performance characteristics. The test suite (67 tests) validates security controls. |
| **Status** | **Partially Implemented** |
| **Gaps** | Implement scheduled security evaluations. Add compliance assessment tooling. Create security metrics dashboards. |

---

## Physical Safeguards (Section 164.310)

Physical safeguards are physical measures, policies, and procedures to protect
electronic information systems and related buildings and equipment from natural
and environmental hazards and unauthorized intrusion.

### 164.310(a)(1) -- Facility Access Controls

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.310(a)(1) |
| **Requirement** | Implement policies and procedures to limit physical access to electronic information systems while ensuring that properly authorized access is allowed. |
| **Mnemo Implementation** | As a software component, Mnemo defers facility-level controls to the deployment environment. The Docker deployment (`Dockerfile`) uses a minimal `debian:bookworm-slim` base image, reducing the attack surface. The Kubernetes deployment guide provides pod security recommendations. |
| **Status** | **Operational** |
| **Gaps** | This is entirely an operational requirement. Document recommended deployment environments with facility access controls. |

### 164.310(b) -- Workstation Use

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.310(b) |
| **Requirement** | Implement policies and procedures that specify the proper functions to be performed and the physical attributes of the surroundings of workstations that access ePHI. |
| **Mnemo Implementation** | Not directly applicable to Mnemo as a server-side component. The MCP STDIO transport binds sessions to individual agent processes. |
| **Status** | **Operational** |
| **Gaps** | Document workstation security requirements for operators who administer Mnemo deployments. |

### 164.310(c) -- Workstation Security

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.310(c) |
| **Requirement** | Implement physical safeguards for all workstations that access ePHI. |
| **Mnemo Implementation** | Not directly applicable. See workstation use above. |
| **Status** | **Operational** |
| **Gaps** | Document workstation security requirements in the deployment guide. |

### 164.310(d)(1) -- Device and Media Controls

**Requirement:** Implement policies and procedures that govern the receipt and
removal of hardware and electronic media containing ePHI.

#### (i) Disposal (Required)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.310(d)(2)(i) |
| **Requirement** | Implement policies for the final disposition of ePHI and/or the hardware or electronic media on which it is stored. |
| **Mnemo Implementation** | `hard_delete_memory()` permanently removes records from DuckDB storage. `cleanup_expired()` removes expired memories. Cognitive forgetting (`run_decay_pass()`) automatically transitions aging memories through the Archived and Forgotten states. Encrypted content requires the encryption key for meaningful access. |
| **Status** | **Partially Implemented** |
| **Gaps** | Implement secure wipe (zero-fill) for hard-deleted records. Add cryptographic erasure support (destroying the encryption key to render stored ciphertext unrecoverable). Document media disposal procedures. |

#### (ii) Media Re-use (Required)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.310(d)(2)(ii) |
| **Requirement** | Implement procedures for removal of ePHI from electronic media before re-use. |
| **Mnemo Implementation** | DuckDB file storage can be wiped by deleting the database file. Encrypted content is not recoverable without the encryption key. |
| **Status** | **Operational** |
| **Gaps** | Document media re-use procedures. Implement database purge utilities. |

---

## Technical Safeguards (Section 164.312)

Technical safeguards are the technology, and the policy and procedures for its
use, that protect ePHI and control access to it.

### 164.312(a)(1) -- Access Control

**Requirement:** Implement technical policies and procedures for electronic
information systems that maintain ePHI to allow access only to those persons or
software programs that have been granted access rights.

#### (i) Unique User Identification (Required)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.312(a)(2)(i) |
| **Requirement** | Assign a unique name and/or number for identifying and tracking user identity. |
| **Mnemo Implementation** | Every agent is identified by a unique `agent_id` string. All operations (memory CRUD, events, delegations) are attributed to the performing agent. The `PrincipalType` enum supports five identity types: Agent, User, Org, Role, Public. Memory records track `created_by` for creator attribution. Event records include `agent_id`, `thread_id`, and `run_id` for operation attribution. |
| **Status** | **Implemented** |
| **Gaps** | None. Unique identification is comprehensive. |

#### (ii) Emergency Access Procedure (Required)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.312(a)(2)(ii) |
| **Requirement** | Establish procedures for obtaining necessary ePHI during an emergency. |
| **Mnemo Implementation** | `Admin`-level permissions provide unrestricted access. Mnemo can operate locally with DuckDB without network dependencies. Checkpoint restore enables recovery of specific state snapshots. |
| **Status** | **Partially Implemented** |
| **Gaps** | Document emergency access procedures. Implement break-glass access mechanism with enhanced audit logging. |

#### (iii) Automatic Logoff (Addressable)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.312(a)(2)(iii) |
| **Requirement** | Implement electronic procedures that terminate an electronic session after a predetermined time of inactivity. |
| **Mnemo Implementation** | MCP STDIO sessions are bound to process lifetime. ACL entries and delegations support `expires_at` for time-based access termination. |
| **Status** | **Partially Implemented** |
| **Gaps** | Implement session timeout for REST API connections. Add configurable inactivity timeout for MCP sessions. |

#### (iv) Encryption and Decryption (Addressable)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.312(a)(2)(iv) |
| **Requirement** | Implement a mechanism to encrypt and decrypt ePHI. |
| **Mnemo Implementation** | The `ContentEncryption` module (`crates/mnemo-core/src/encryption.rs`) provides encryption/decryption of memory content. Keys are 256-bit, loaded from environment variables. The encryption produces `nonce \|\| ciphertext \|\| tag` format with integrity verification on decryption. |
| **Status** | **Partially Implemented** |
| **Gaps** | Upgrade to production-grade AES-256-GCM using the `aes-gcm` crate (currently uses a simplified XOR cipher). Implement key rotation. Add per-field encryption for metadata. |

---

### 164.312(b) -- Audit Controls

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.312(b) |
| **Requirement** | Implement hardware, software, and/or procedural mechanisms that record and examine activity in information systems that contain or use ePHI. |
| **Mnemo Implementation** | The `AgentEvent` system provides comprehensive audit logging. Every data access operation generates an event record with: unique `id` (UUID v7, time-ordered), `agent_id`, `thread_id`, `run_id`, `event_type` (15 types covering all operations), `payload` (JSON with operation details), `timestamp`, `logical_clock` (monotonic ordering), `content_hash` and `prev_hash` (hash chain integrity). OpenTelemetry fields (`trace_id`, `span_id`) enable correlation with external observability systems. Query methods support review by agent, thread, or event hierarchy. |
| **Status** | **Implemented** |
| **Gaps** | Add tamper-evident log export to external storage. Implement log retention policies. Add real-time audit stream for SIEM integration. |

---

### 164.312(c)(1) -- Integrity

**Requirement:** Implement policies and procedures to protect ePHI from
improper alteration or destruction.

#### (i) Mechanism to Authenticate ePHI (Addressable)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.312(c)(2) |
| **Requirement** | Implement electronic mechanisms to corroborate that ePHI has not been altered or destroyed in an unauthorized manner. |
| **Mnemo Implementation** | The hash chain system (`crates/mnemo-core/src/hash.rs`) provides two levels of integrity verification: (1) **Content hash** -- `compute_content_hash(content, agent_id, timestamp)` produces a SHA-256 hash of each memory's content, agent, and timestamp. (2) **Chain hash** -- `compute_chain_hash(content_hash, prev_hash)` links each record to its predecessor, creating a tamper-evident chain. `verify_chain()` validates both content hashes and chain linkage, reporting `ChainVerificationResult` with the exact record where tampering is detected. The encryption module adds a 16-byte HMAC tag to ciphertext, verified on decryption. Memory versioning (`version`, `prev_version_id`) tracks all modifications. |
| **Status** | **Implemented** |
| **Gaps** | Add automated periodic integrity verification. Consider adding digital signatures for non-repudiation. |

---

### 164.312(d) -- Person or Entity Authentication

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.312(d) |
| **Requirement** | Implement procedures to verify that a person or entity seeking access to ePHI is the one claimed. |
| **Mnemo Implementation** | Agent identity is established through the `agent_id` field on all operations. The MCP STDIO transport binds sessions to OS-level processes. The permission system verifies that the requesting agent has appropriate authorization before returning data. |
| **Status** | **Partially Implemented** |
| **Gaps** | Implement cryptographic authentication (API keys, mTLS, JWT). Add support for multi-factor authentication for administrative operations. Integrate with external identity providers (OIDC, SAML, LDAP). |

---

### 164.312(e)(1) -- Transmission Security

**Requirement:** Implement technical security measures to guard against
unauthorized access to ePHI that is being transmitted over an electronic
communications network.

#### (i) Integrity Controls (Addressable)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.312(e)(2)(i) |
| **Requirement** | Implement security measures to ensure that electronically transmitted ePHI is not improperly modified without detection. |
| **Mnemo Implementation** | Content hashes travel with memory records, enabling integrity verification at the receiving end. The hash chain provides ordering integrity across sequences of records. |
| **Status** | **Partially Implemented** |
| **Gaps** | Implement message-level signatures for MCP protocol messages. Add integrity verification for REST API responses. |

#### (ii) Encryption (Addressable)

| Field | Detail |
|---|---|
| **HIPAA Reference** | 164.312(e)(2)(ii) |
| **Requirement** | Implement a mechanism to encrypt ePHI whenever deemed appropriate during transmission. |
| **Mnemo Implementation** | The MCP STDIO transport operates over local Unix pipes, which are not exposed to network transmission. For network deployments, the documentation recommends TLS. The PostgreSQL mode supports TLS connections. The Docker deployment guide recommends reverse proxy with TLS termination. |
| **Status** | **Partially Implemented** |
| **Gaps** | Enforce TLS for all network transports (reject non-TLS connections). Implement TLS certificate pinning for PostgreSQL connections. Add MCP-over-TLS support for remote agent connections. |

---

## Summary Matrix

| Safeguard Category | Section | Status | Key Modules |
|---|---|---|---|
| Security Management | 164.308(a)(1) | Partially Implemented | `poisoning.rs`, `encryption.rs`, `hash.rs`, `acl.rs` |
| Assigned Security Responsibility | 164.308(a)(2) | Operational | `acl.rs` (Admin role) |
| Workforce Security | 164.308(a)(3) | Implemented | `acl.rs`, `delegation.rs` |
| Information Access Management | 164.308(a)(4) | Implemented | `acl.rs`, `delegation.rs`, MCP tools |
| Security Awareness | 164.308(a)(5) | Partially Implemented | `poisoning.rs`, documentation |
| Security Incident Procedures | 164.308(a)(6) | Partially Implemented | Quarantine, `hash.rs`, `event.rs` |
| Contingency Plan | 164.308(a)(7) | Partially Implemented | `checkpoint.rs`, MCP tools |
| Evaluation | 164.308(a)(8) | Partially Implemented | `hash.rs`, test suite |
| Facility Access | 164.310(a)(1) | Operational | Docker, Kubernetes |
| Workstation Use/Security | 164.310(b-c) | Operational | N/A |
| Device and Media Controls | 164.310(d)(1) | Partially Implemented | Delete operations, `encryption.rs` |
| Access Control | 164.312(a)(1) | Partially Implemented | `acl.rs`, `delegation.rs`, `encryption.rs` |
| Audit Controls | 164.312(b) | Implemented | `event.rs` |
| Integrity | 164.312(c)(1) | Implemented | `hash.rs`, `encryption.rs` |
| Authentication | 164.312(d) | Partially Implemented | `agent_id`, MCP session binding |
| Transmission Security | 164.312(e)(1) | Partially Implemented | TLS recommendations, `hash.rs` |

---

## Priority Gaps for HIPAA Compliance

The following items represent the highest-priority gaps for organizations
deploying Mnemo in HIPAA-regulated environments. They are listed in
recommended order of implementation:

1. **Upgrade encryption to production-grade AES-256-GCM** (164.312(a)(2)(iv))
   -- Replace the simplified XOR cipher with the `aes-gcm` crate. This is
   the single most critical gap for HIPAA compliance.

2. **Implement cryptographic authentication** (164.312(d)) -- Add API key
   management, mTLS, or JWT-based authentication. Agent identity must be
   cryptographically verified, not just asserted.

3. **Enforce TLS for all network transports** (164.312(e)(2)(ii)) -- Reject
   non-TLS connections in network deployment modes. Implement certificate
   validation.

4. **Add key rotation and management** (164.312(a)(2)(iv)) -- Implement
   encryption key rotation without downtime. Add envelope encryption for
   per-record key management.

5. **Implement automated backup and recovery** (164.308(a)(7)) -- Add
   scheduled checkpoint creation, backup verification, and documented
   recovery procedures with defined RTO/RPO.

6. **Add session timeout** (164.312(a)(2)(iii)) -- Implement configurable
   inactivity timeout for REST API and MCP sessions.

7. **Implement tenant isolation** (164.308(a)(4)) -- Enforce strict data
   separation by `org_id` at the database query level to prevent cross-tenant
   data leakage.

8. **Implement break-glass access** (164.312(a)(2)(ii)) -- Add an emergency
   access mechanism with enhanced audit logging for HIPAA-mandated emergency
   access procedures.

---

## Deployment Recommendations for Covered Entities

Organizations subject to HIPAA that deploy Mnemo should implement the following
operational controls in addition to Mnemo's built-in safeguards:

### Infrastructure

- Deploy Mnemo behind a TLS-terminating reverse proxy (e.g., nginx, Envoy).
- Use PostgreSQL mode with TLS-encrypted connections for production.
- Store the encryption key (`MNEMO_ENCRYPTION_KEY`) in a secrets manager (e.g.,
  HashiCorp Vault, AWS Secrets Manager), not in environment files.
- Run Mnemo containers with read-only root filesystems and non-root users.
- Implement network policies restricting Mnemo's inbound and outbound traffic.

### Operations

- Assign a security official responsible for Mnemo deployment and configuration.
- Conduct a risk assessment specific to your PHI data flows through Mnemo.
- Establish backup schedules using the checkpoint system with offsite replication.
- Document and test disaster recovery procedures quarterly.
- Implement log forwarding from Mnemo's event log to your SIEM system.
- Schedule periodic hash chain verification using the `mnemo.verify` tool.

### Access Management

- Map organizational roles to Mnemo's permission hierarchy.
- Use time-bounded delegations with the minimum required permission level.
- Review active ACLs and delegations quarterly.
- Implement agent deprovisioning procedures that revoke all permissions.
- Maintain an access authorization matrix mapping agents to data categories.

### Incident Response

- Define incident severity levels for Mnemo security events.
- Establish escalation procedures for quarantine events and verification failures.
- Document breach notification procedures per HIPAA requirements (60-day
  notification timeline).
- Conduct tabletop exercises simulating data integrity incidents.

### Training

- Train operators on Mnemo's security features and compliance controls.
- Include Mnemo-specific content in HIPAA security awareness training.
- Document procedures for handling PHI within agent memory workflows.
