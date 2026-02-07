package mnemo

import (
	"encoding/json"
	"testing"
)

// ---------------------------------------------------------------------------
// TestNewClient — verifies client creation with a missing binary.
// ---------------------------------------------------------------------------

func TestNewClientMissingBinary(t *testing.T) {
	_, err := NewClient(ClientOptions{
		Command: "mnemo-nonexistent-binary-for-test",
		DbPath:  "/tmp/test-mnemo.db",
		AgentID: "test-agent",
	})
	if err == nil {
		t.Fatal("expected error when binary does not exist, got nil")
	}
}

// ---------------------------------------------------------------------------
// TestBuildArgs — verifies CLI argument construction.
// ---------------------------------------------------------------------------

func TestBuildArgs(t *testing.T) {
	tests := []struct {
		name string
		opts ClientOptions
		want []string
	}{
		{
			name: "empty options",
			opts: ClientOptions{},
			want: nil,
		},
		{
			name: "all options",
			opts: ClientOptions{
				DbPath:     "/data/agent.db",
				AgentID:    "agent-1",
				OrgID:      "org-42",
				OpenAIKey:  "sk-test-key",
				Dimensions: 768,
			},
			want: []string{
				"--db-path", "/data/agent.db",
				"--agent-id", "agent-1",
				"--org-id", "org-42",
				"--openai-api-key", "sk-test-key",
				"--dimensions", "768",
			},
		},
		{
			name: "partial options",
			opts: ClientOptions{
				DbPath:  "memory.db",
				AgentID: "bot",
			},
			want: []string{
				"--db-path", "memory.db",
				"--agent-id", "bot",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := buildArgs(tt.opts)
			if len(got) != len(tt.want) {
				t.Fatalf("buildArgs() returned %d args, want %d\ngot:  %v\nwant: %v", len(got), len(tt.want), got, tt.want)
			}
			for i := range got {
				if got[i] != tt.want[i] {
					t.Errorf("arg[%d] = %q, want %q", i, got[i], tt.want[i])
				}
			}
		})
	}
}

// ---------------------------------------------------------------------------
// TestRememberInputJSON — verifies JSON marshaling of RememberInput.
// ---------------------------------------------------------------------------

func TestRememberInputJSON(t *testing.T) {
	importance := float32(0.9)
	memType := "semantic"
	scope := "private"
	ttl := uint64(3600)

	input := RememberInput{
		Content:    "Go is a statically typed language",
		MemoryType: &memType,
		Scope:      &scope,
		Importance: &importance,
		Tags:       []string{"golang", "facts"},
		Metadata:   map[string]interface{}{"source": "docs"},
		TTLSeconds: &ttl,
		RelatedTo:  []string{"abc-123"},
	}

	data, err := json.Marshal(input)
	if err != nil {
		t.Fatalf("Marshal RememberInput: %v", err)
	}

	var decoded RememberInput
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("Unmarshal RememberInput: %v", err)
	}

	if decoded.Content != input.Content {
		t.Errorf("Content = %q, want %q", decoded.Content, input.Content)
	}
	if decoded.MemoryType == nil || *decoded.MemoryType != memType {
		t.Errorf("MemoryType = %v, want %q", decoded.MemoryType, memType)
	}
	if decoded.Importance == nil || *decoded.Importance != importance {
		t.Errorf("Importance = %v, want %f", decoded.Importance, importance)
	}
	if len(decoded.Tags) != 2 {
		t.Errorf("Tags length = %d, want 2", len(decoded.Tags))
	}
	if decoded.TTLSeconds == nil || *decoded.TTLSeconds != ttl {
		t.Errorf("TTLSeconds = %v, want %d", decoded.TTLSeconds, ttl)
	}
}

// ---------------------------------------------------------------------------
// TestRememberInputOmitEmpty — verifies omitempty fields are absent.
// ---------------------------------------------------------------------------

func TestRememberInputOmitEmpty(t *testing.T) {
	input := RememberInput{
		Content: "minimal memory",
	}

	data, err := json.Marshal(input)
	if err != nil {
		t.Fatalf("Marshal: %v", err)
	}

	var raw map[string]interface{}
	if err := json.Unmarshal(data, &raw); err != nil {
		t.Fatalf("Unmarshal to map: %v", err)
	}

	if _, ok := raw["content"]; !ok {
		t.Error("expected 'content' key in JSON")
	}

	for _, key := range []string{"agent_id", "memory_type", "scope", "importance", "tags", "metadata", "ttl_seconds", "related_to", "decay_rate", "created_by"} {
		if _, ok := raw[key]; ok {
			t.Errorf("expected key %q to be omitted, but it was present", key)
		}
	}
}

// ---------------------------------------------------------------------------
// TestRecallInputJSON — verifies RecallInput round-trip.
// ---------------------------------------------------------------------------

func TestRecallInputJSON(t *testing.T) {
	limit := 5
	strategy := "hybrid"
	minImp := float32(0.3)
	after := "2024-01-01T00:00:00Z"

	input := RecallInput{
		Query:         "user preferences",
		Limit:         &limit,
		Strategy:      &strategy,
		MinImportance: &minImp,
		Tags:          []string{"prefs"},
		TemporalRange: &TemporalRange{
			After: &after,
		},
	}

	data, err := json.Marshal(input)
	if err != nil {
		t.Fatalf("Marshal RecallInput: %v", err)
	}

	var decoded RecallInput
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("Unmarshal RecallInput: %v", err)
	}

	if decoded.Query != "user preferences" {
		t.Errorf("Query = %q, want %q", decoded.Query, "user preferences")
	}
	if decoded.Limit == nil || *decoded.Limit != limit {
		t.Errorf("Limit = %v, want %d", decoded.Limit, limit)
	}
	if decoded.Strategy == nil || *decoded.Strategy != strategy {
		t.Errorf("Strategy = %v, want %q", decoded.Strategy, strategy)
	}
	if decoded.TemporalRange == nil || decoded.TemporalRange.After == nil {
		t.Error("TemporalRange.After should be set")
	}
}

// ---------------------------------------------------------------------------
// TestRecallResponseJSON — verifies recall response deserialization.
// ---------------------------------------------------------------------------

func TestRecallResponseJSON(t *testing.T) {
	raw := `{
		"memories": [
			{
				"id": "550e8400-e29b-41d4-a716-446655440000",
				"agent_id": "agent-1",
				"content": "User prefers dark mode",
				"memory_type": "semantic",
				"scope": "private",
				"importance": 0.8,
				"tags": ["preferences"],
				"score": 0.95,
				"created_at": "2024-01-15T10:30:00Z",
				"updated_at": "2024-01-15T10:30:00Z"
			}
		],
		"total": 1
	}`

	var resp RecallResponse
	if err := json.Unmarshal([]byte(raw), &resp); err != nil {
		t.Fatalf("Unmarshal RecallResponse: %v", err)
	}

	if resp.Total != 1 {
		t.Errorf("Total = %d, want 1", resp.Total)
	}
	if len(resp.Memories) != 1 {
		t.Fatalf("Memories length = %d, want 1", len(resp.Memories))
	}

	m := resp.Memories[0]
	if m.Content != "User prefers dark mode" {
		t.Errorf("Content = %q, want %q", m.Content, "User prefers dark mode")
	}
	if m.Score != 0.95 {
		t.Errorf("Score = %f, want 0.95", m.Score)
	}
	if m.Importance != 0.8 {
		t.Errorf("Importance = %f, want 0.8", m.Importance)
	}
}

// ---------------------------------------------------------------------------
// TestForgetInputJSON — verifies ForgetInput with criteria.
// ---------------------------------------------------------------------------

func TestForgetInputJSON(t *testing.T) {
	strategy := "decay"
	maxAge := 48.0
	minImp := float32(0.2)

	input := ForgetInput{
		MemoryIDs: []string{},
		Strategy:  &strategy,
		Criteria: &ForgetCriteria{
			MaxAgeHours:        &maxAge,
			MinImportanceBelow: &minImp,
			Tags:               []string{"temp"},
		},
	}

	data, err := json.Marshal(input)
	if err != nil {
		t.Fatalf("Marshal ForgetInput: %v", err)
	}

	var decoded ForgetInput
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("Unmarshal ForgetInput: %v", err)
	}

	if decoded.Strategy == nil || *decoded.Strategy != strategy {
		t.Errorf("Strategy = %v, want %q", decoded.Strategy, strategy)
	}
	if decoded.Criteria == nil {
		t.Fatal("Criteria should not be nil")
	}
	if decoded.Criteria.MaxAgeHours == nil || *decoded.Criteria.MaxAgeHours != maxAge {
		t.Errorf("MaxAgeHours = %v, want %f", decoded.Criteria.MaxAgeHours, maxAge)
	}
}

// ---------------------------------------------------------------------------
// TestForgetResponseJSON — verifies forget response deserialization.
// ---------------------------------------------------------------------------

func TestForgetResponseJSON(t *testing.T) {
	raw := `{
		"forgotten": ["id-1", "id-2"],
		"errors": [{"id": "id-3", "error": "not found"}],
		"status": "forgotten"
	}`

	var resp ForgetResponse
	if err := json.Unmarshal([]byte(raw), &resp); err != nil {
		t.Fatalf("Unmarshal ForgetResponse: %v", err)
	}

	if len(resp.Forgotten) != 2 {
		t.Errorf("Forgotten length = %d, want 2", len(resp.Forgotten))
	}
	if len(resp.Errors) != 1 {
		t.Errorf("Errors length = %d, want 1", len(resp.Errors))
	}
	if resp.Status != "forgotten" {
		t.Errorf("Status = %q, want %q", resp.Status, "forgotten")
	}
}

// ---------------------------------------------------------------------------
// TestShareInputJSON — verifies ShareInput marshaling.
// ---------------------------------------------------------------------------

func TestShareInputJSON(t *testing.T) {
	perm := "write"
	hours := 24.0

	input := ShareInput{
		MemoryID:       "mem-uuid",
		TargetAgentID:  "agent-2",
		TargetAgentIDs: []string{"agent-2", "agent-3"},
		Permission:     &perm,
		ExpiresInHours: &hours,
	}

	data, err := json.Marshal(input)
	if err != nil {
		t.Fatalf("Marshal ShareInput: %v", err)
	}

	var decoded ShareInput
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("Unmarshal ShareInput: %v", err)
	}

	if decoded.MemoryID != "mem-uuid" {
		t.Errorf("MemoryID = %q, want %q", decoded.MemoryID, "mem-uuid")
	}
	if len(decoded.TargetAgentIDs) != 2 {
		t.Errorf("TargetAgentIDs length = %d, want 2", len(decoded.TargetAgentIDs))
	}
}

// ---------------------------------------------------------------------------
// TestCheckpointInputJSON — verifies CheckpointInput marshaling.
// ---------------------------------------------------------------------------

func TestCheckpointInputJSON(t *testing.T) {
	branch := "experiment"
	label := "before-refactor"

	input := CheckpointInput{
		ThreadID:      "thread-1",
		BranchName:    &branch,
		StateSnapshot: map[string]interface{}{"step": 5, "score": 0.87},
		Label:         &label,
	}

	data, err := json.Marshal(input)
	if err != nil {
		t.Fatalf("Marshal CheckpointInput: %v", err)
	}

	var raw map[string]interface{}
	if err := json.Unmarshal(data, &raw); err != nil {
		t.Fatalf("Unmarshal to map: %v", err)
	}

	if raw["thread_id"] != "thread-1" {
		t.Errorf("thread_id = %v, want %q", raw["thread_id"], "thread-1")
	}
	if raw["branch_name"] != "experiment" {
		t.Errorf("branch_name = %v, want %q", raw["branch_name"], "experiment")
	}

	snapshot, ok := raw["state_snapshot"].(map[string]interface{})
	if !ok {
		t.Fatal("state_snapshot should be a map")
	}
	if snapshot["step"] != float64(5) {
		t.Errorf("state_snapshot.step = %v, want 5", snapshot["step"])
	}
}

// ---------------------------------------------------------------------------
// TestBranchInputJSON — verifies BranchInput marshaling.
// ---------------------------------------------------------------------------

func TestBranchInputJSON(t *testing.T) {
	srcBranch := "main"
	srcCP := "cp-123"

	input := BranchInput{
		ThreadID:           "thread-1",
		NewBranchName:      "feature-x",
		SourceCheckpointID: &srcCP,
		SourceBranch:       &srcBranch,
	}

	data, err := json.Marshal(input)
	if err != nil {
		t.Fatalf("Marshal BranchInput: %v", err)
	}

	var decoded BranchInput
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("Unmarshal BranchInput: %v", err)
	}

	if decoded.NewBranchName != "feature-x" {
		t.Errorf("NewBranchName = %q, want %q", decoded.NewBranchName, "feature-x")
	}
	if decoded.SourceCheckpointID == nil || *decoded.SourceCheckpointID != srcCP {
		t.Errorf("SourceCheckpointID = %v, want %q", decoded.SourceCheckpointID, srcCP)
	}
}

// ---------------------------------------------------------------------------
// TestMergeInputJSON — verifies MergeInput marshaling.
// ---------------------------------------------------------------------------

func TestMergeInputJSON(t *testing.T) {
	target := "main"
	strategy := "cherry_pick"

	input := MergeInput{
		ThreadID:      "thread-1",
		SourceBranch:  "feature-x",
		TargetBranch:  &target,
		Strategy:      &strategy,
		CherryPickIDs: []string{"mem-1", "mem-2"},
	}

	data, err := json.Marshal(input)
	if err != nil {
		t.Fatalf("Marshal MergeInput: %v", err)
	}

	var decoded MergeInput
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("Unmarshal MergeInput: %v", err)
	}

	if decoded.SourceBranch != "feature-x" {
		t.Errorf("SourceBranch = %q, want %q", decoded.SourceBranch, "feature-x")
	}
	if len(decoded.CherryPickIDs) != 2 {
		t.Errorf("CherryPickIDs length = %d, want 2", len(decoded.CherryPickIDs))
	}
}

// ---------------------------------------------------------------------------
// TestReplayInputJSON — verifies ReplayInput marshaling.
// ---------------------------------------------------------------------------

func TestReplayInputJSON(t *testing.T) {
	cpID := "cp-456"
	branch := "main"

	input := ReplayInput{
		ThreadID:     "thread-1",
		CheckpointID: &cpID,
		BranchName:   &branch,
	}

	data, err := json.Marshal(input)
	if err != nil {
		t.Fatalf("Marshal ReplayInput: %v", err)
	}

	var decoded ReplayInput
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("Unmarshal ReplayInput: %v", err)
	}

	if decoded.ThreadID != "thread-1" {
		t.Errorf("ThreadID = %q, want %q", decoded.ThreadID, "thread-1")
	}
	if decoded.CheckpointID == nil || *decoded.CheckpointID != cpID {
		t.Errorf("CheckpointID = %v, want %q", decoded.CheckpointID, cpID)
	}
}

// ---------------------------------------------------------------------------
// TestReplayResponseJSON — verifies replay response deserialization.
// ---------------------------------------------------------------------------

func TestReplayResponseJSON(t *testing.T) {
	raw := `{
		"checkpoint": {
			"id": "cp-789",
			"branch_name": "main",
			"state_snapshot": {"step": 3},
			"label": "mid-run",
			"created_at": "2024-06-01T12:00:00Z"
		},
		"memory_count": 2,
		"event_count": 5,
		"memories": [
			{"id": "m1", "content": "first", "memory_type": "episodic", "created_at": "2024-06-01T11:00:00Z"},
			{"id": "m2", "content": "second", "memory_type": "semantic", "created_at": "2024-06-01T11:30:00Z"}
		],
		"status": "replayed"
	}`

	var resp ReplayResponse
	if err := json.Unmarshal([]byte(raw), &resp); err != nil {
		t.Fatalf("Unmarshal ReplayResponse: %v", err)
	}

	if resp.Checkpoint.ID != "cp-789" {
		t.Errorf("Checkpoint.ID = %q, want %q", resp.Checkpoint.ID, "cp-789")
	}
	if resp.MemoryCount != 2 {
		t.Errorf("MemoryCount = %d, want 2", resp.MemoryCount)
	}
	if resp.EventCount != 5 {
		t.Errorf("EventCount = %d, want 5", resp.EventCount)
	}
	if len(resp.Memories) != 2 {
		t.Fatalf("Memories length = %d, want 2", len(resp.Memories))
	}
	if resp.Status != "replayed" {
		t.Errorf("Status = %q, want %q", resp.Status, "replayed")
	}
}

// ---------------------------------------------------------------------------
// TestVerifyInputJSON — verifies VerifyInput marshaling.
// ---------------------------------------------------------------------------

func TestVerifyInputJSON(t *testing.T) {
	agentID := "agent-1"
	threadID := "thread-42"

	input := VerifyInput{
		AgentID:  &agentID,
		ThreadID: &threadID,
	}

	data, err := json.Marshal(input)
	if err != nil {
		t.Fatalf("Marshal VerifyInput: %v", err)
	}

	var decoded VerifyInput
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("Unmarshal VerifyInput: %v", err)
	}

	if decoded.AgentID == nil || *decoded.AgentID != agentID {
		t.Errorf("AgentID = %v, want %q", decoded.AgentID, agentID)
	}
	if decoded.ThreadID == nil || *decoded.ThreadID != threadID {
		t.Errorf("ThreadID = %v, want %q", decoded.ThreadID, threadID)
	}
}

// ---------------------------------------------------------------------------
// TestVerifyResponseJSON — verifies verify response deserialization.
// ---------------------------------------------------------------------------

func TestVerifyResponseJSON(t *testing.T) {
	raw := `{
		"valid": true,
		"total_records": 10,
		"verified_records": 10,
		"first_broken_at": null,
		"error_message": null,
		"status": "verified"
	}`

	var resp VerifyResponse
	if err := json.Unmarshal([]byte(raw), &resp); err != nil {
		t.Fatalf("Unmarshal VerifyResponse: %v", err)
	}

	if !resp.Valid {
		t.Error("Valid should be true")
	}
	if resp.TotalRecords != 10 {
		t.Errorf("TotalRecords = %d, want 10", resp.TotalRecords)
	}
	if resp.VerifiedRecords != 10 {
		t.Errorf("VerifiedRecords = %d, want 10", resp.VerifiedRecords)
	}
	if resp.FirstBrokenAt != nil {
		t.Errorf("FirstBrokenAt should be nil, got %v", resp.FirstBrokenAt)
	}
	if resp.Status != "verified" {
		t.Errorf("Status = %q, want %q", resp.Status, "verified")
	}
}

// ---------------------------------------------------------------------------
// TestDelegateInputJSON — verifies DelegateInput marshaling.
// ---------------------------------------------------------------------------

func TestDelegateInputJSON(t *testing.T) {
	maxDepth := uint32(2)
	hours := 72.0

	input := DelegateInput{
		DelegateID:     "agent-3",
		Permission:     "write",
		MemoryIDs:      []string{"mem-1", "mem-2"},
		Tags:           []string{"important"},
		MaxDepth:       &maxDepth,
		ExpiresInHours: &hours,
	}

	data, err := json.Marshal(input)
	if err != nil {
		t.Fatalf("Marshal DelegateInput: %v", err)
	}

	var decoded DelegateInput
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("Unmarshal DelegateInput: %v", err)
	}

	if decoded.DelegateID != "agent-3" {
		t.Errorf("DelegateID = %q, want %q", decoded.DelegateID, "agent-3")
	}
	if decoded.Permission != "write" {
		t.Errorf("Permission = %q, want %q", decoded.Permission, "write")
	}
	if decoded.MaxDepth == nil || *decoded.MaxDepth != maxDepth {
		t.Errorf("MaxDepth = %v, want %d", decoded.MaxDepth, maxDepth)
	}
	if decoded.ExpiresInHours == nil || *decoded.ExpiresInHours != hours {
		t.Errorf("ExpiresInHours = %v, want %f", decoded.ExpiresInHours, hours)
	}
	if len(decoded.MemoryIDs) != 2 {
		t.Errorf("MemoryIDs length = %d, want 2", len(decoded.MemoryIDs))
	}
}

// ---------------------------------------------------------------------------
// TestDelegateResponseJSON — verifies delegate response deserialization.
// ---------------------------------------------------------------------------

func TestDelegateResponseJSON(t *testing.T) {
	raw := `{
		"delegation_id": "del-uuid-123",
		"delegator": "agent-1",
		"delegate": "agent-3",
		"permission": "write",
		"status": "delegated"
	}`

	var resp DelegateResponse
	if err := json.Unmarshal([]byte(raw), &resp); err != nil {
		t.Fatalf("Unmarshal DelegateResponse: %v", err)
	}

	if resp.DelegationID != "del-uuid-123" {
		t.Errorf("DelegationID = %q, want %q", resp.DelegationID, "del-uuid-123")
	}
	if resp.Delegator != "agent-1" {
		t.Errorf("Delegator = %q, want %q", resp.Delegator, "agent-1")
	}
	if resp.Status != "delegated" {
		t.Errorf("Status = %q, want %q", resp.Status, "delegated")
	}
}

// ---------------------------------------------------------------------------
// TestRememberResponseJSON — verifies remember response deserialization.
// ---------------------------------------------------------------------------

func TestRememberResponseJSON(t *testing.T) {
	raw := `{
		"id": "550e8400-e29b-41d4-a716-446655440000",
		"content_hash": "sha256:abcdef1234567890",
		"status": "remembered"
	}`

	var resp RememberResponse
	if err := json.Unmarshal([]byte(raw), &resp); err != nil {
		t.Fatalf("Unmarshal RememberResponse: %v", err)
	}

	if resp.ID != "550e8400-e29b-41d4-a716-446655440000" {
		t.Errorf("ID = %q, want UUID", resp.ID)
	}
	if resp.ContentHash != "sha256:abcdef1234567890" {
		t.Errorf("ContentHash = %q", resp.ContentHash)
	}
	if resp.Status != "remembered" {
		t.Errorf("Status = %q, want %q", resp.Status, "remembered")
	}
}

// ---------------------------------------------------------------------------
// TestShareResponseJSON — verifies share response deserialization.
// ---------------------------------------------------------------------------

func TestShareResponseJSON(t *testing.T) {
	raw := `{
		"acl_id": "acl-1",
		"acl_ids": ["acl-1", "acl-2"],
		"memory_id": "mem-1",
		"shared_with": ["agent-2", "agent-3"],
		"permission": "read",
		"status": "shared"
	}`

	var resp ShareResponse
	if err := json.Unmarshal([]byte(raw), &resp); err != nil {
		t.Fatalf("Unmarshal ShareResponse: %v", err)
	}

	if resp.ACLID != "acl-1" {
		t.Errorf("ACLID = %q, want %q", resp.ACLID, "acl-1")
	}
	if len(resp.SharedWith) != 2 {
		t.Errorf("SharedWith length = %d, want 2", len(resp.SharedWith))
	}
	if resp.Status != "shared" {
		t.Errorf("Status = %q, want %q", resp.Status, "shared")
	}
}

// ---------------------------------------------------------------------------
// TestCheckpointResponseJSON — verifies checkpoint response deserialization.
// ---------------------------------------------------------------------------

func TestCheckpointResponseJSON(t *testing.T) {
	raw := `{
		"checkpoint_id": "cp-100",
		"parent_id": "cp-99",
		"branch_name": "main",
		"status": "checkpointed"
	}`

	var resp CheckpointResponse
	if err := json.Unmarshal([]byte(raw), &resp); err != nil {
		t.Fatalf("Unmarshal CheckpointResponse: %v", err)
	}

	if resp.CheckpointID != "cp-100" {
		t.Errorf("CheckpointID = %q, want %q", resp.CheckpointID, "cp-100")
	}
	if resp.ParentID == nil || *resp.ParentID != "cp-99" {
		t.Errorf("ParentID = %v, want %q", resp.ParentID, "cp-99")
	}
	if resp.BranchName != "main" {
		t.Errorf("BranchName = %q, want %q", resp.BranchName, "main")
	}
}

// ---------------------------------------------------------------------------
// TestBranchResponseJSON — verifies branch response deserialization.
// ---------------------------------------------------------------------------

func TestBranchResponseJSON(t *testing.T) {
	raw := `{
		"checkpoint_id": "cp-200",
		"branch_name": "feature-x",
		"source_checkpoint_id": "cp-100",
		"status": "branched"
	}`

	var resp BranchResponse
	if err := json.Unmarshal([]byte(raw), &resp); err != nil {
		t.Fatalf("Unmarshal BranchResponse: %v", err)
	}

	if resp.CheckpointID != "cp-200" {
		t.Errorf("CheckpointID = %q, want %q", resp.CheckpointID, "cp-200")
	}
	if resp.BranchName != "feature-x" {
		t.Errorf("BranchName = %q, want %q", resp.BranchName, "feature-x")
	}
}

// ---------------------------------------------------------------------------
// TestMergeResponseJSON — verifies merge response deserialization.
// ---------------------------------------------------------------------------

func TestMergeResponseJSON(t *testing.T) {
	raw := `{
		"checkpoint_id": "cp-300",
		"target_branch": "main",
		"merged_memory_count": 7,
		"status": "merged"
	}`

	var resp MergeResponse
	if err := json.Unmarshal([]byte(raw), &resp); err != nil {
		t.Fatalf("Unmarshal MergeResponse: %v", err)
	}

	if resp.MergedMemoryCount != 7 {
		t.Errorf("MergedMemoryCount = %d, want 7", resp.MergedMemoryCount)
	}
	if resp.Status != "merged" {
		t.Errorf("Status = %q, want %q", resp.Status, "merged")
	}
}

// ---------------------------------------------------------------------------
// TestJSONRPCRequestMarshal — verifies the JSON-RPC request envelope.
// ---------------------------------------------------------------------------

func TestJSONRPCRequestMarshal(t *testing.T) {
	id := 42
	req := jsonRPCRequest{
		JSONRPC: "2.0",
		Method:  "tools/call",
		Params: toolCallParams{
			Name: "mnemo.remember",
			Arguments: RememberInput{
				Content: "test content",
			},
		},
		ID: &id,
	}

	data, err := json.Marshal(req)
	if err != nil {
		t.Fatalf("Marshal jsonRPCRequest: %v", err)
	}

	var raw map[string]interface{}
	if err := json.Unmarshal(data, &raw); err != nil {
		t.Fatalf("Unmarshal to map: %v", err)
	}

	if raw["jsonrpc"] != "2.0" {
		t.Errorf("jsonrpc = %v, want %q", raw["jsonrpc"], "2.0")
	}
	if raw["method"] != "tools/call" {
		t.Errorf("method = %v, want %q", raw["method"], "tools/call")
	}
	if raw["id"] != float64(42) {
		t.Errorf("id = %v, want 42", raw["id"])
	}

	params, ok := raw["params"].(map[string]interface{})
	if !ok {
		t.Fatal("params should be a map")
	}
	if params["name"] != "mnemo.remember" {
		t.Errorf("params.name = %v, want %q", params["name"], "mnemo.remember")
	}
}

// ---------------------------------------------------------------------------
// TestJSONRPCNotificationMarshal — verifies notification has no id.
// ---------------------------------------------------------------------------

func TestJSONRPCNotificationMarshal(t *testing.T) {
	notif := jsonRPCRequest{
		JSONRPC: "2.0",
		Method:  "notifications/initialized",
	}

	data, err := json.Marshal(notif)
	if err != nil {
		t.Fatalf("Marshal notification: %v", err)
	}

	var raw map[string]interface{}
	if err := json.Unmarshal(data, &raw); err != nil {
		t.Fatalf("Unmarshal to map: %v", err)
	}

	if _, ok := raw["id"]; ok {
		t.Error("notification should not have 'id' field")
	}
	if _, ok := raw["params"]; ok {
		t.Error("notification should not have 'params' field when nil")
	}
}

// ---------------------------------------------------------------------------
// TestJSONRPCResponseUnmarshal — verifies response with content parsing.
// ---------------------------------------------------------------------------

func TestJSONRPCResponseUnmarshal(t *testing.T) {
	raw := `{
		"jsonrpc": "2.0",
		"result": {
			"content": [
				{
					"type": "text",
					"text": "{\"id\":\"test-id\",\"content_hash\":\"hash\",\"status\":\"remembered\"}"
				}
			]
		},
		"id": 1
	}`

	var resp jsonRPCResponse
	if err := json.Unmarshal([]byte(raw), &resp); err != nil {
		t.Fatalf("Unmarshal jsonRPCResponse: %v", err)
	}

	if resp.Error != nil {
		t.Error("Error should be nil")
	}
	if resp.Result == nil {
		t.Fatal("Result should not be nil")
	}
	if len(resp.Result.Content) != 1 {
		t.Fatalf("Content length = %d, want 1", len(resp.Result.Content))
	}
	if resp.Result.Content[0].Type != "text" {
		t.Errorf("Content[0].Type = %q, want %q", resp.Result.Content[0].Type, "text")
	}

	// Parse the inner JSON.
	var remember RememberResponse
	if err := json.Unmarshal([]byte(resp.Result.Content[0].Text), &remember); err != nil {
		t.Fatalf("Unmarshal inner content: %v", err)
	}
	if remember.ID != "test-id" {
		t.Errorf("ID = %q, want %q", remember.ID, "test-id")
	}
}

// ---------------------------------------------------------------------------
// TestJSONRPCErrorResponseUnmarshal — verifies error response parsing.
// ---------------------------------------------------------------------------

func TestJSONRPCErrorResponseUnmarshal(t *testing.T) {
	raw := `{
		"jsonrpc": "2.0",
		"error": {
			"code": -32600,
			"message": "Invalid Request"
		},
		"id": 1
	}`

	var resp jsonRPCResponse
	if err := json.Unmarshal([]byte(raw), &resp); err != nil {
		t.Fatalf("Unmarshal error response: %v", err)
	}

	if resp.Error == nil {
		t.Fatal("Error should not be nil")
	}
	if resp.Error.Code != -32600 {
		t.Errorf("Error.Code = %d, want -32600", resp.Error.Code)
	}
	if resp.Error.Message != "Invalid Request" {
		t.Errorf("Error.Message = %q, want %q", resp.Error.Message, "Invalid Request")
	}
}

// ---------------------------------------------------------------------------
// TestVerifyBrokenChainJSON — verifies integrity violation response.
// ---------------------------------------------------------------------------

func TestVerifyBrokenChainJSON(t *testing.T) {
	brokenID := "550e8400-e29b-41d4-a716-446655440099"
	errMsg := "hash mismatch at record 5"

	raw := `{
		"valid": false,
		"total_records": 10,
		"verified_records": 4,
		"first_broken_at": "` + brokenID + `",
		"error_message": "` + errMsg + `",
		"status": "integrity_violation"
	}`

	var resp VerifyResponse
	if err := json.Unmarshal([]byte(raw), &resp); err != nil {
		t.Fatalf("Unmarshal VerifyResponse: %v", err)
	}

	if resp.Valid {
		t.Error("Valid should be false")
	}
	if resp.VerifiedRecords != 4 {
		t.Errorf("VerifiedRecords = %d, want 4", resp.VerifiedRecords)
	}
	if resp.FirstBrokenAt == nil || *resp.FirstBrokenAt != brokenID {
		t.Errorf("FirstBrokenAt = %v, want %q", resp.FirstBrokenAt, brokenID)
	}
	if resp.ErrorMessage == nil || *resp.ErrorMessage != errMsg {
		t.Errorf("ErrorMessage = %v, want %q", resp.ErrorMessage, errMsg)
	}
	if resp.Status != "integrity_violation" {
		t.Errorf("Status = %q, want %q", resp.Status, "integrity_violation")
	}
}
