// Package mnemo provides a Go SDK for the Mnemo MCP-native memory database.
//
// Mnemo is a memory database for AI agents that supports semantic search, graph
// relations, hash chain verification, git-like state management (checkpoint,
// branch, merge, replay), and scoped permission delegation.
//
// This SDK communicates with the mnemo CLI binary over MCP STDIO transport using
// JSON-RPC 2.0 messages.
package mnemo

// ---------------------------------------------------------------------------
// Remember
// ---------------------------------------------------------------------------

// RememberInput contains parameters for storing a new memory.
type RememberInput struct {
	// Content is the text to remember. Required.
	Content string `json:"content"`

	// AgentID overrides the default agent identifier for this memory.
	AgentID *string `json:"agent_id,omitempty"`

	// MemoryType classifies the memory: "episodic", "semantic", "procedural",
	// or "working". Defaults to "episodic".
	MemoryType *string `json:"memory_type,omitempty"`

	// Scope controls visibility: "private", "shared", "public", or "global".
	// Defaults to "private".
	Scope *string `json:"scope,omitempty"`

	// Importance is a score from 0.0 to 1.0. Higher means more important.
	// Defaults to 0.5.
	Importance *float32 `json:"importance,omitempty"`

	// Tags categorize and filter memories.
	Tags []string `json:"tags,omitempty"`

	// Metadata holds additional key-value pairs.
	Metadata map[string]interface{} `json:"metadata,omitempty"`

	// SourceType indicates where the memory originated (e.g. "agent", "user").
	SourceType *string `json:"source_type,omitempty"`

	// SourceID is the identifier of the originating source.
	SourceID *string `json:"source_id,omitempty"`

	// RelatedTo lists memory IDs that this memory is related to.
	RelatedTo []string `json:"related_to,omitempty"`

	// OrgID overrides the default organization identifier.
	OrgID *string `json:"org_id,omitempty"`

	// ThreadID groups related memories in a conversation thread.
	ThreadID *string `json:"thread_id,omitempty"`

	// TTLSeconds is the time-to-live in seconds. The memory expires after this
	// duration.
	TTLSeconds *uint64 `json:"ttl_seconds,omitempty"`

	// DecayRate controls the Ebbinghaus decay curve speed for cognitive
	// forgetting.
	DecayRate *float32 `json:"decay_rate,omitempty"`

	// CreatedBy records which agent originally created this memory.
	CreatedBy *string `json:"created_by,omitempty"`
}

// RememberResponse is returned after successfully storing a memory.
type RememberResponse struct {
	ID          string `json:"id"`
	ContentHash string `json:"content_hash"`
	Status      string `json:"status"`
}

// ---------------------------------------------------------------------------
// Recall
// ---------------------------------------------------------------------------

// TemporalRange constrains recall results by creation time.
type TemporalRange struct {
	// After returns only memories created after this RFC 3339 timestamp.
	After *string `json:"after,omitempty"`

	// Before returns only memories created before this RFC 3339 timestamp.
	Before *string `json:"before,omitempty"`
}

// RecallInput contains parameters for searching and retrieving memories.
type RecallInput struct {
	// Query is a natural language search string. Required.
	Query string `json:"query"`

	// AgentID overrides the default agent identifier for the search.
	AgentID *string `json:"agent_id,omitempty"`

	// Limit caps the number of returned memories. Defaults to 10, max 100.
	Limit *int `json:"limit,omitempty"`

	// MemoryType filters by a single memory type.
	MemoryType *string `json:"memory_type,omitempty"`

	// MemoryTypes filters by multiple memory types simultaneously. Takes
	// precedence over MemoryType if both are set.
	MemoryTypes []string `json:"memory_types,omitempty"`

	// Scope filters by visibility scope.
	Scope *string `json:"scope,omitempty"`

	// MinImportance filters by minimum importance score (0.0 to 1.0).
	MinImportance *float32 `json:"min_importance,omitempty"`

	// Tags filters by tag, returning memories matching any specified tag.
	Tags []string `json:"tags,omitempty"`

	// OrgID overrides the default organization identifier.
	OrgID *string `json:"org_id,omitempty"`

	// Strategy selects the retrieval algorithm: "semantic", "lexical",
	// "hybrid", "graph", "exact", or "auto". Defaults to "auto".
	Strategy *string `json:"strategy,omitempty"`

	// TemporalRange constrains results by creation time.
	TemporalRange *TemporalRange `json:"temporal_range,omitempty"`
}

// RecalledMemory represents a single memory returned by a recall query.
type RecalledMemory struct {
	ID         string   `json:"id"`
	AgentID    string   `json:"agent_id"`
	Content    string   `json:"content"`
	MemoryType string   `json:"memory_type"`
	Scope      string   `json:"scope"`
	Importance float32  `json:"importance"`
	Tags       []string `json:"tags"`
	Score      float32  `json:"score"`
	CreatedAt  string   `json:"created_at"`
	UpdatedAt  string   `json:"updated_at"`
}

// RecallResponse is returned after searching for memories.
type RecallResponse struct {
	Memories []RecalledMemory `json:"memories"`
	Total    int              `json:"total"`
}

// ---------------------------------------------------------------------------
// Forget
// ---------------------------------------------------------------------------

// ForgetCriteria specifies filter conditions for criteria-based forget
// operations.
type ForgetCriteria struct {
	// MaxAgeHours removes memories older than this many hours.
	MaxAgeHours *float64 `json:"max_age_hours,omitempty"`

	// MinImportanceBelow removes memories with importance below this threshold.
	MinImportanceBelow *float32 `json:"min_importance_below,omitempty"`

	// MemoryType restricts the forget operation to this memory type.
	MemoryType *string `json:"memory_type,omitempty"`

	// Tags restricts the forget operation to memories with these tags.
	Tags []string `json:"tags,omitempty"`
}

// ForgetInput contains parameters for deleting or archiving memories.
type ForgetInput struct {
	// MemoryIDs lists the memory UUIDs to forget. May be empty when using
	// Criteria.
	MemoryIDs []string `json:"memory_ids"`

	// AgentID overrides the default agent identifier.
	AgentID *string `json:"agent_id,omitempty"`

	// Strategy selects the deletion method: "soft_delete", "hard_delete",
	// "decay", "archive", or "consolidate". Defaults to "soft_delete".
	Strategy *string `json:"strategy,omitempty"`

	// Criteria enables filter-based forget when MemoryIDs is empty.
	Criteria *ForgetCriteria `json:"criteria,omitempty"`
}

// ForgetError describes a failure to forget a specific memory.
type ForgetError struct {
	ID    string `json:"id"`
	Error string `json:"error"`
}

// ForgetResponse is returned after a forget operation.
type ForgetResponse struct {
	Forgotten []string      `json:"forgotten"`
	Errors    []ForgetError `json:"errors"`
	Status    string        `json:"status"`
}

// ---------------------------------------------------------------------------
// Share
// ---------------------------------------------------------------------------

// ShareInput contains parameters for sharing a memory with other agents.
type ShareInput struct {
	// MemoryID is the UUID of the memory to share. Required.
	MemoryID string `json:"memory_id"`

	// TargetAgentID is the agent to share with. Required unless
	// TargetAgentIDs is set.
	TargetAgentID string `json:"target_agent_id"`

	// TargetAgentIDs shares with multiple agents at once. Takes precedence
	// over TargetAgentID.
	TargetAgentIDs []string `json:"target_agent_ids,omitempty"`

	// AgentID overrides the default agent identifier (the sharer).
	AgentID *string `json:"agent_id,omitempty"`

	// Permission is the access level to grant: "read", "write", or "admin".
	// Defaults to "read".
	Permission *string `json:"permission,omitempty"`

	// ExpiresInHours sets a TTL on the share. Nil means no expiration.
	ExpiresInHours *float64 `json:"expires_in_hours,omitempty"`
}

// ShareResponse is returned after sharing a memory.
type ShareResponse struct {
	ACLID      string   `json:"acl_id"`
	ACLIDs     []string `json:"acl_ids"`
	MemoryID   string   `json:"memory_id"`
	SharedWith []string `json:"shared_with"`
	Permission string   `json:"permission"`
	Status     string   `json:"status"`
}

// ---------------------------------------------------------------------------
// Checkpoint
// ---------------------------------------------------------------------------

// CheckpointInput contains parameters for creating a state checkpoint.
type CheckpointInput struct {
	// ThreadID identifies the conversation thread. Required.
	ThreadID string `json:"thread_id"`

	// BranchName is the branch to checkpoint on. Defaults to "main".
	BranchName *string `json:"branch_name,omitempty"`

	// StateSnapshot is a JSON representation of the current agent state.
	// Required.
	StateSnapshot interface{} `json:"state_snapshot"`

	// Label is a human-readable label for this checkpoint.
	Label *string `json:"label,omitempty"`

	// Metadata holds additional key-value pairs.
	Metadata map[string]interface{} `json:"metadata,omitempty"`
}

// CheckpointResponse is returned after creating a checkpoint.
type CheckpointResponse struct {
	CheckpointID string  `json:"checkpoint_id"`
	ParentID     *string `json:"parent_id"`
	BranchName   string  `json:"branch_name"`
	Status       string  `json:"status"`
}

// ---------------------------------------------------------------------------
// Branch
// ---------------------------------------------------------------------------

// BranchInput contains parameters for forking a new branch.
type BranchInput struct {
	// ThreadID identifies the conversation thread. Required.
	ThreadID string `json:"thread_id"`

	// NewBranchName is the name for the new branch. Required.
	NewBranchName string `json:"new_branch_name"`

	// SourceCheckpointID is the checkpoint to branch from. Uses the latest if
	// not set.
	SourceCheckpointID *string `json:"source_checkpoint_id,omitempty"`

	// SourceBranch is the branch to fork from. Defaults to "main".
	SourceBranch *string `json:"source_branch,omitempty"`
}

// BranchResponse is returned after creating a branch.
type BranchResponse struct {
	CheckpointID       string `json:"checkpoint_id"`
	BranchName         string `json:"branch_name"`
	SourceCheckpointID string `json:"source_checkpoint_id"`
	Status             string `json:"status"`
}

// ---------------------------------------------------------------------------
// Merge
// ---------------------------------------------------------------------------

// MergeInput contains parameters for merging branches.
type MergeInput struct {
	// ThreadID identifies the conversation thread. Required.
	ThreadID string `json:"thread_id"`

	// SourceBranch is the branch to merge from. Required.
	SourceBranch string `json:"source_branch"`

	// TargetBranch is the branch to merge into. Defaults to "main".
	TargetBranch *string `json:"target_branch,omitempty"`

	// Strategy selects the merge method: "full_merge", "cherry_pick", or
	// "squash". Defaults to "full_merge".
	Strategy *string `json:"strategy,omitempty"`

	// CherryPickIDs lists specific memory UUIDs for the "cherry_pick"
	// strategy.
	CherryPickIDs []string `json:"cherry_pick_ids,omitempty"`
}

// MergeResponse is returned after merging branches.
type MergeResponse struct {
	CheckpointID     string `json:"checkpoint_id"`
	TargetBranch     string `json:"target_branch"`
	MergedMemoryCount int   `json:"merged_memory_count"`
	Status           string `json:"status"`
}

// ---------------------------------------------------------------------------
// Replay
// ---------------------------------------------------------------------------

// ReplayInput contains parameters for replaying state from a checkpoint.
type ReplayInput struct {
	// ThreadID identifies the conversation thread. Required.
	ThreadID string `json:"thread_id"`

	// CheckpointID is the specific checkpoint to replay. Uses the latest if
	// not set.
	CheckpointID *string `json:"checkpoint_id,omitempty"`

	// BranchName is the branch to replay from. Defaults to "main".
	BranchName *string `json:"branch_name,omitempty"`
}

// ReplayCheckpoint holds checkpoint details within a replay response.
type ReplayCheckpoint struct {
	ID            string      `json:"id"`
	BranchName    string      `json:"branch_name"`
	StateSnapshot interface{} `json:"state_snapshot"`
	Label         *string     `json:"label"`
	CreatedAt     string      `json:"created_at"`
}

// ReplayMemory holds a summarized memory within a replay response.
type ReplayMemory struct {
	ID         string `json:"id"`
	Content    string `json:"content"`
	MemoryType string `json:"memory_type"`
	CreatedAt  string `json:"created_at"`
}

// ReplayResponse is returned after replaying a checkpoint.
type ReplayResponse struct {
	Checkpoint  ReplayCheckpoint `json:"checkpoint"`
	MemoryCount int              `json:"memory_count"`
	EventCount  int              `json:"event_count"`
	Memories    []ReplayMemory   `json:"memories"`
	Status      string           `json:"status"`
}

// ---------------------------------------------------------------------------
// Verify
// ---------------------------------------------------------------------------

// VerifyInput contains parameters for verifying hash chain integrity.
type VerifyInput struct {
	// AgentID limits verification to a specific agent. Uses default if nil.
	AgentID *string `json:"agent_id,omitempty"`

	// ThreadID limits verification to a specific conversation thread.
	ThreadID *string `json:"thread_id,omitempty"`
}

// VerifyResponse is returned after verifying hash chain integrity.
type VerifyResponse struct {
	Valid           bool    `json:"valid"`
	TotalRecords    int     `json:"total_records"`
	VerifiedRecords int     `json:"verified_records"`
	FirstBrokenAt   *string `json:"first_broken_at"`
	ErrorMessage    *string `json:"error_message"`
	Status          string  `json:"status"`
}

// ---------------------------------------------------------------------------
// Delegate
// ---------------------------------------------------------------------------

// DelegateInput contains parameters for delegating permissions to another
// agent.
type DelegateInput struct {
	// DelegateID is the agent to receive the delegation. Required.
	DelegateID string `json:"delegate_id"`

	// Permission is the access level to delegate: "read", "write", "delete",
	// "share", "delegate", or "admin". Required.
	Permission string `json:"permission"`

	// MemoryIDs scopes the delegation to specific memories. If both MemoryIDs
	// and Tags are empty, the delegation applies to all memories.
	MemoryIDs []string `json:"memory_ids,omitempty"`

	// Tags scopes the delegation to memories with these tags.
	Tags []string `json:"tags,omitempty"`

	// MaxDepth limits re-delegation depth. 0 means the delegate cannot further
	// delegate.
	MaxDepth *uint32 `json:"max_depth,omitempty"`

	// ExpiresInHours sets a TTL on the delegation. Nil means permanent.
	ExpiresInHours *float64 `json:"expires_in_hours,omitempty"`
}

// DelegateResponse is returned after creating a delegation.
type DelegateResponse struct {
	DelegationID string `json:"delegation_id"`
	Delegator    string `json:"delegator"`
	Delegate     string `json:"delegate"`
	Permission   string `json:"permission"`
	Status       string `json:"status"`
}

// ---------------------------------------------------------------------------
// JSON-RPC internal types
// ---------------------------------------------------------------------------

// jsonRPCRequest is the JSON-RPC 2.0 request envelope.
type jsonRPCRequest struct {
	JSONRPC string      `json:"jsonrpc"`
	Method  string      `json:"method"`
	Params  interface{} `json:"params,omitempty"`
	ID      *int        `json:"id,omitempty"`
}

// jsonRPCResponse is the JSON-RPC 2.0 response envelope.
type jsonRPCResponse struct {
	JSONRPC string           `json:"jsonrpc"`
	Result  *jsonRPCResult   `json:"result,omitempty"`
	Error   *jsonRPCError    `json:"error,omitempty"`
	ID      *int             `json:"id,omitempty"`
}

// jsonRPCResult holds the result field of a successful JSON-RPC response.
type jsonRPCResult struct {
	Content []jsonRPCContent `json:"content,omitempty"`

	// Raw captures any other fields for non-tool-call responses (e.g.
	// initialize).
	Raw map[string]interface{} `json:"-"`
}

// jsonRPCContent represents a single content item in the MCP response.
type jsonRPCContent struct {
	Type string `json:"type"`
	Text string `json:"text"`
}

// jsonRPCError represents the error field of a JSON-RPC error response.
type jsonRPCError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

// toolCallParams is the params envelope for a tools/call request.
type toolCallParams struct {
	Name      string      `json:"name"`
	Arguments interface{} `json:"arguments"`
}
