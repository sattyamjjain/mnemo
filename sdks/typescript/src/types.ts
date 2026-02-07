// ---------------------------------------------------------------------------
// Mnemo MCP Tool Input Types
// ---------------------------------------------------------------------------

/** Memory type classification. */
export type MemoryType = "episodic" | "semantic" | "procedural" | "working";

/** Visibility scope for a memory. */
export type Scope = "private" | "shared" | "public" | "global";

/** Retrieval strategy for recall. */
export type RecallStrategy =
  | "semantic"
  | "lexical"
  | "hybrid"
  | "graph"
  | "exact"
  | "auto";

/** Forget strategy. */
export type ForgetStrategy =
  | "soft_delete"
  | "hard_delete"
  | "decay"
  | "consolidate"
  | "archive";

/** Permission level. */
export type Permission =
  | "read"
  | "write"
  | "delete"
  | "share"
  | "delegate"
  | "admin";

/** Merge strategy. */
export type MergeStrategy = "full_merge" | "cherry_pick" | "squash";

// ---------------------------------------------------------------------------
// mnemo.remember
// ---------------------------------------------------------------------------

/** Input for the `mnemo.remember` tool. */
export interface RememberInput {
  /** The content to remember. Can be a fact, preference, instruction, or any text. */
  content: string;
  /** The type of memory. Defaults to "episodic". */
  memory_type?: MemoryType;
  /** Visibility scope. Defaults to "private". */
  scope?: Scope;
  /** Importance score from 0.0 to 1.0. Defaults to 0.5. */
  importance?: number;
  /** Tags for categorizing and filtering memories. */
  tags?: string[];
  /** Additional metadata as key-value pairs. */
  metadata?: Record<string, unknown>;
  /** Time-to-live in seconds. The memory will expire after this duration. */
  ttl_seconds?: number;
  /** List of memory IDs that this memory is related to. */
  related_to?: string[];
  /** Thread ID for grouping related memories in a conversation thread. */
  thread_id?: string;
}

/** Response from the `mnemo.remember` tool. */
export interface RememberResponse {
  /** The UUID of the newly stored memory. */
  id: string;
  /** SHA-256 content hash of the stored memory. */
  content_hash: string;
  /** Status indicator -- always "remembered" on success. */
  status: string;
}

// ---------------------------------------------------------------------------
// mnemo.recall
// ---------------------------------------------------------------------------

/** Temporal range filter for recall queries. */
export interface TemporalRange {
  /** Only return memories created after this timestamp (RFC 3339 format). */
  after?: string;
  /** Only return memories created before this timestamp (RFC 3339 format). */
  before?: string;
}

/** Input for the `mnemo.recall` tool. */
export interface RecallInput {
  /** Natural language query to search memories semantically. */
  query: string;
  /** Maximum number of memories to return. Defaults to 10, max 100. */
  limit?: number;
  /** Filter by memory type. */
  memory_type?: MemoryType;
  /** Filter by multiple memory types. Takes precedence over memory_type if both are set. */
  memory_types?: MemoryType[];
  /** Filter by scope. */
  scope?: Scope;
  /** Filter by minimum importance score (0.0 to 1.0). */
  min_importance?: number;
  /** Filter by tags. Returns memories matching any of the specified tags. */
  tags?: string[];
  /** Retrieval strategy. Defaults to "auto". */
  strategy?: RecallStrategy;
  /** Filter by time range. */
  temporal_range?: TemporalRange;
}

/** A single recalled memory in the result set. */
export interface RecalledMemory {
  /** UUID of the memory. */
  id: string;
  /** Agent ID that created this memory. */
  agent_id: string;
  /** The memory content text. */
  content: string;
  /** The type of memory. */
  memory_type: string;
  /** Visibility scope. */
  scope: string;
  /** Importance score. */
  importance: number;
  /** Tags attached to the memory. */
  tags: string[];
  /** Relevance score from the retrieval process. */
  score: number;
  /** ISO 8601 creation timestamp. */
  created_at: string;
  /** ISO 8601 last-update timestamp. */
  updated_at: string;
}

/** Response from the `mnemo.recall` tool. */
export interface RecallResponse {
  /** List of recalled memories, ranked by relevance score. */
  memories: RecalledMemory[];
  /** Total number of matching memories (may exceed the returned count). */
  total: number;
}

// ---------------------------------------------------------------------------
// mnemo.forget
// ---------------------------------------------------------------------------

/** Criteria for criteria-based forget operations. */
export interface ForgetCriteria {
  /** Maximum age in hours. Memories older than this will be affected. */
  max_age_hours?: number;
  /** Importance threshold. Memories with importance below this will be affected. */
  min_importance_below?: number;
  /** Filter by memory type. */
  memory_type?: MemoryType;
  /** Filter by tags. */
  tags?: string[];
}

/** Input for the `mnemo.forget` tool. */
export interface ForgetInput {
  /** List of memory IDs to forget/delete. Can be empty if criteria is specified. */
  memory_ids: string[];
  /** Delete strategy. Defaults to "soft_delete". */
  strategy?: ForgetStrategy;
  /** Criteria-based forget: find and apply strategy to memories matching these filters. */
  criteria?: ForgetCriteria;
}

/** A single error entry from the forget operation. */
export interface ForgetError {
  /** The memory ID that failed. */
  id: string;
  /** Description of the error. */
  error: string;
}

/** Response from the `mnemo.forget` tool. */
export interface ForgetResponse {
  /** List of successfully forgotten memory IDs. */
  forgotten: string[];
  /** List of errors for memories that could not be forgotten. */
  errors: ForgetError[];
  /** Status indicator -- always "forgotten" on success. */
  status: string;
}

// ---------------------------------------------------------------------------
// mnemo.share
// ---------------------------------------------------------------------------

/** Input for the `mnemo.share` tool. */
export interface ShareInput {
  /** The ID of the memory to share. */
  memory_id: string;
  /** The agent ID to share the memory with. */
  target_agent_id: string;
  /** Share with multiple agents at once. Takes precedence over target_agent_id if set. */
  target_agent_ids?: string[];
  /** Permission level to grant. Defaults to "read". */
  permission?: Permission;
  /** Number of hours until the share expires. If not set, the share does not expire. */
  expires_in_hours?: number;
}

/** Response from the `mnemo.share` tool. */
export interface ShareResponse {
  /** UUID of the created ACL entry. */
  acl_id: string;
  /** List of all ACL IDs created (one per target agent). */
  acl_ids: string[];
  /** UUID of the shared memory. */
  memory_id: string;
  /** List of agent IDs the memory was shared with. */
  shared_with: string[];
  /** The permission level granted. */
  permission: string;
  /** Status indicator -- always "shared" on success. */
  status: string;
}

// ---------------------------------------------------------------------------
// mnemo.checkpoint
// ---------------------------------------------------------------------------

/** Input for the `mnemo.checkpoint` tool. */
export interface CheckpointInput {
  /** The thread ID to create a checkpoint for. */
  thread_id: string;
  /** The branch name. Defaults to "main". */
  branch_name?: string;
  /** A JSON snapshot of the current agent state. */
  state_snapshot: unknown;
  /** An optional label for this checkpoint. */
  label?: string;
  /** Optional metadata as key-value pairs. */
  metadata?: Record<string, unknown>;
}

/** Response from the `mnemo.checkpoint` tool. */
export interface CheckpointResponse {
  /** UUID of the created checkpoint. */
  checkpoint_id: string;
  /** UUID of the parent checkpoint, if any. */
  parent_id: string | null;
  /** Branch name the checkpoint was created on. */
  branch_name: string;
  /** Status indicator -- always "checkpointed" on success. */
  status: string;
}

// ---------------------------------------------------------------------------
// mnemo.branch
// ---------------------------------------------------------------------------

/** Input for the `mnemo.branch` tool. */
export interface BranchInput {
  /** The thread ID to branch from. */
  thread_id: string;
  /** The name for the new branch. */
  new_branch_name: string;
  /** The checkpoint ID to branch from. If not specified, uses the latest checkpoint. */
  source_checkpoint_id?: string;
  /** The source branch to branch from. Defaults to "main". */
  source_branch?: string;
}

/** Response from the `mnemo.branch` tool. */
export interface BranchResponse {
  /** UUID of the checkpoint created for the new branch. */
  checkpoint_id: string;
  /** Name of the newly created branch. */
  branch_name: string;
  /** UUID of the source checkpoint that was branched from. */
  source_checkpoint_id: string;
  /** Status indicator -- always "branched" on success. */
  status: string;
}

// ---------------------------------------------------------------------------
// mnemo.merge
// ---------------------------------------------------------------------------

/** Input for the `mnemo.merge` tool. */
export interface MergeInput {
  /** The thread ID containing the branches to merge. */
  thread_id: string;
  /** The branch to merge from. */
  source_branch: string;
  /** The branch to merge into. Defaults to "main". */
  target_branch?: string;
  /** Merge strategy. Defaults to "full_merge". */
  strategy?: MergeStrategy;
  /** Memory IDs to cherry-pick (only used with "cherry_pick" strategy). */
  cherry_pick_ids?: string[];
}

/** Response from the `mnemo.merge` tool. */
export interface MergeResponse {
  /** UUID of the merge checkpoint created on the target branch. */
  checkpoint_id: string;
  /** The branch that was merged into. */
  target_branch: string;
  /** Number of memories that were merged. */
  merged_memory_count: number;
  /** Status indicator -- always "merged" on success. */
  status: string;
}

// ---------------------------------------------------------------------------
// mnemo.replay
// ---------------------------------------------------------------------------

/** Input for the `mnemo.replay` tool. */
export interface ReplayInput {
  /** The thread ID to replay. */
  thread_id: string;
  /** Specific checkpoint ID to replay. If not specified, uses the latest checkpoint. */
  checkpoint_id?: string;
  /** The branch name. Defaults to "main". */
  branch_name?: string;
}

/** A checkpoint snapshot returned by replay. */
export interface ReplayCheckpoint {
  /** UUID of the checkpoint. */
  id: string;
  /** Branch name. */
  branch_name: string;
  /** The state snapshot at this checkpoint. */
  state_snapshot: unknown;
  /** Optional label. */
  label: string | null;
  /** ISO 8601 creation timestamp. */
  created_at: string;
}

/** A memory snapshot returned by replay. */
export interface ReplayMemory {
  /** UUID of the memory. */
  id: string;
  /** The memory content text. */
  content: string;
  /** The type of memory. */
  memory_type: string;
  /** ISO 8601 creation timestamp. */
  created_at: string;
}

/** Response from the `mnemo.replay` tool. */
export interface ReplayResponse {
  /** The checkpoint that was replayed. */
  checkpoint: ReplayCheckpoint;
  /** Number of memories referenced at this checkpoint. */
  memory_count: number;
  /** Number of events up to this checkpoint. */
  event_count: number;
  /** The memories active at this checkpoint. */
  memories: ReplayMemory[];
  /** Status indicator -- always "replayed" on success. */
  status: string;
}

// ---------------------------------------------------------------------------
// mnemo.verify
// ---------------------------------------------------------------------------

/** Input for the `mnemo.verify` tool. */
export interface VerifyInput {
  /** Agent ID to verify chain integrity for. Uses default if not specified. */
  agent_id?: string;
  /** Optional thread ID to limit verification to a specific thread. */
  thread_id?: string;
}

/** Response from the `mnemo.verify` tool. */
export interface VerifyResponse {
  /** Whether the hash chain is valid. */
  valid: boolean;
  /** Total number of records checked. */
  total_records: number;
  /** Number of records that passed verification. */
  verified_records: number;
  /** UUID of the first record where the chain broke, if any. */
  first_broken_at: string | null;
  /** Error message describing the integrity violation, if any. */
  error_message: string | null;
  /** Status indicator -- "verified" or "integrity_violation". */
  status: string;
}

// ---------------------------------------------------------------------------
// mnemo.delegate
// ---------------------------------------------------------------------------

/** Input for the `mnemo.delegate` tool. */
export interface DelegateInput {
  /** Agent ID to delegate permissions to. */
  delegate_id: string;
  /** Permission to delegate. */
  permission: Permission;
  /** Specific memory IDs to scope the delegation to. */
  memory_ids?: string[];
  /** Tags to scope the delegation to. */
  tags?: string[];
  /** Maximum re-delegation depth. 0 means the delegate cannot further delegate. */
  max_depth?: number;
  /** Hours until this delegation expires. If not set, delegation is permanent. */
  expires_in_hours?: number;
}

/** Response from the `mnemo.delegate` tool. */
export interface DelegateResponse {
  /** UUID of the created delegation record. */
  delegation_id: string;
  /** Agent ID of the delegator. */
  delegator: string;
  /** Agent ID of the delegate. */
  delegate: string;
  /** The permission that was delegated. */
  permission: string;
  /** Status indicator -- always "delegated" on success. */
  status: string;
}

// ---------------------------------------------------------------------------
// Client options
// ---------------------------------------------------------------------------

/** Options for configuring the MnemoClient. */
export interface MnemoClientOptions {
  /** Path to the `mnemo` binary. Defaults to "mnemo". */
  command?: string;
  /** Path to the database file. Defaults to "mnemo.db". */
  dbPath?: string;
  /** Default agent ID. */
  agentId?: string;
  /** Default organization ID. */
  orgId?: string;
  /** OpenAI API key for embeddings. */
  openaiApiKey?: string;
  /** Embedding dimensions. */
  dimensions?: number;
  /** Embedding model name. */
  embeddingModel?: string;
  /** PostgreSQL connection URL (enables PostgreSQL backend). */
  postgresUrl?: string;
  /** Additional environment variables to pass to the child process. */
  env?: Record<string, string>;
}

// ---------------------------------------------------------------------------
// JSON-RPC protocol types (internal)
// ---------------------------------------------------------------------------

/** A JSON-RPC 2.0 request message. */
export interface JsonRpcRequest {
  jsonrpc: "2.0";
  method: string;
  params?: unknown;
  id?: number;
}

/** A JSON-RPC 2.0 response message. */
export interface JsonRpcResponse {
  jsonrpc: "2.0";
  id: number;
  result?: {
    content?: Array<{ type: string; text: string }>;
    [key: string]: unknown;
  };
  error?: {
    code: number;
    message: string;
    data?: unknown;
  };
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/** Error thrown when the MCP server returns a tool-level error. */
export class MnemoToolError extends Error {
  constructor(
    public readonly toolName: string,
    message: string,
  ) {
    super(`${toolName}: ${message}`);
    this.name = "MnemoToolError";
  }
}

/** Error thrown when JSON-RPC communication fails. */
export class MnemoRpcError extends Error {
  constructor(
    public readonly code: number,
    message: string,
    public readonly data?: unknown,
  ) {
    super(`JSON-RPC error (${code}): ${message}`);
    this.name = "MnemoRpcError";
  }
}

/** Error thrown when the client is not connected or connection failed. */
export class MnemoConnectionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "MnemoConnectionError";
  }
}
