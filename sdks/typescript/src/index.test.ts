import {
  MnemoClient,
  MnemoConnectionError,
  MnemoToolError,
  MnemoRpcError,
} from "./index";

import type {
  RememberInput,
  RecallInput,
  ForgetInput,
  ShareInput,
  CheckpointInput,
  BranchInput,
  MergeInput,
  ReplayInput,
  VerifyInput,
  DelegateInput,
  MnemoClientOptions,
  RememberResponse,
  RecallResponse,
  ForgetResponse,
  ShareResponse,
  CheckpointResponse,
  BranchResponse,
  MergeResponse,
  ReplayResponse,
  VerifyResponse,
  DelegateResponse,
  RecalledMemory,
} from "./types";

// ---------------------------------------------------------------------------
// Type-level tests: these compile-time checks verify that the interfaces
// are structurally correct. If any interface is malformed, the test file
// itself will fail to compile.
// ---------------------------------------------------------------------------

describe("MnemoClient", () => {
  it("can be constructed with default options", () => {
    const client = new MnemoClient();
    expect(client).toBeInstanceOf(MnemoClient);
  });

  it("can be constructed with full options", () => {
    const options: MnemoClientOptions = {
      command: "/usr/local/bin/mnemo",
      dbPath: "/tmp/test.db",
      agentId: "agent-1",
      orgId: "org-1",
      openaiApiKey: "sk-test",
      dimensions: 1536,
      embeddingModel: "text-embedding-3-small",
      postgresUrl: "postgres://localhost/mnemo",
      env: { RUST_LOG: "debug" },
    };
    const client = new MnemoClient(options);
    expect(client).toBeInstanceOf(MnemoClient);
  });

  it("throws MnemoConnectionError when calling tools before connect()", async () => {
    const client = new MnemoClient();
    await expect(
      client.remember({ content: "test" }),
    ).rejects.toThrow(MnemoConnectionError);
  });

  it("throws MnemoConnectionError when calling recall before connect()", async () => {
    const client = new MnemoClient();
    await expect(
      client.recall({ query: "test" }),
    ).rejects.toThrow(MnemoConnectionError);
  });

  it("close() is safe to call when not connected", async () => {
    const client = new MnemoClient();
    await expect(client.close()).resolves.toBeUndefined();
  });
});

describe("Type shape checks", () => {
  it("RememberInput requires content and accepts optional fields", () => {
    const minimal: RememberInput = { content: "hello" };
    expect(minimal.content).toBe("hello");

    const full: RememberInput = {
      content: "hello",
      memory_type: "semantic",
      scope: "public",
      importance: 0.8,
      tags: ["test"],
      metadata: { key: "value" },
      ttl_seconds: 3600,
      related_to: ["uuid-1"],
      thread_id: "thread-1",
    };
    expect(full.importance).toBe(0.8);
  });

  it("RecallInput requires query and accepts optional filters", () => {
    const minimal: RecallInput = { query: "search" };
    expect(minimal.query).toBe("search");

    const full: RecallInput = {
      query: "search",
      limit: 20,
      memory_type: "episodic",
      memory_types: ["episodic", "semantic"],
      scope: "shared",
      min_importance: 0.3,
      tags: ["tag1"],
      strategy: "hybrid",
      temporal_range: { after: "2025-01-01T00:00:00Z" },
    };
    expect(full.limit).toBe(20);
  });

  it("ForgetInput requires memory_ids", () => {
    const input: ForgetInput = { memory_ids: ["id-1", "id-2"] };
    expect(input.memory_ids).toHaveLength(2);

    const withCriteria: ForgetInput = {
      memory_ids: [],
      strategy: "decay",
      criteria: {
        max_age_hours: 48,
        min_importance_below: 0.2,
        memory_type: "working",
        tags: ["temp"],
      },
    };
    expect(withCriteria.strategy).toBe("decay");
  });

  it("ShareInput requires memory_id and target_agent_id", () => {
    const input: ShareInput = {
      memory_id: "mem-1",
      target_agent_id: "agent-2",
    };
    expect(input.memory_id).toBe("mem-1");

    const full: ShareInput = {
      memory_id: "mem-1",
      target_agent_id: "agent-2",
      target_agent_ids: ["agent-2", "agent-3"],
      permission: "write",
      expires_in_hours: 24,
    };
    expect(full.permission).toBe("write");
  });

  it("CheckpointInput requires thread_id and state_snapshot", () => {
    const input: CheckpointInput = {
      thread_id: "thread-1",
      state_snapshot: { step: 5, context: "test" },
    };
    expect(input.thread_id).toBe("thread-1");
  });

  it("BranchInput requires thread_id and new_branch_name", () => {
    const input: BranchInput = {
      thread_id: "thread-1",
      new_branch_name: "experiment-a",
    };
    expect(input.new_branch_name).toBe("experiment-a");
  });

  it("MergeInput requires thread_id and source_branch", () => {
    const input: MergeInput = {
      thread_id: "thread-1",
      source_branch: "experiment-a",
    };
    expect(input.source_branch).toBe("experiment-a");
  });

  it("ReplayInput requires thread_id", () => {
    const input: ReplayInput = { thread_id: "thread-1" };
    expect(input.thread_id).toBe("thread-1");
  });

  it("VerifyInput is fully optional", () => {
    const empty: VerifyInput = {};
    expect(empty.agent_id).toBeUndefined();

    const full: VerifyInput = {
      agent_id: "agent-1",
      thread_id: "thread-1",
    };
    expect(full.agent_id).toBe("agent-1");
  });

  it("DelegateInput requires delegate_id and permission", () => {
    const input: DelegateInput = {
      delegate_id: "agent-2",
      permission: "read",
    };
    expect(input.delegate_id).toBe("agent-2");

    const full: DelegateInput = {
      delegate_id: "agent-2",
      permission: "admin",
      memory_ids: ["mem-1"],
      tags: ["important"],
      max_depth: 2,
      expires_in_hours: 72,
    };
    expect(full.max_depth).toBe(2);
  });

  it("Response types have the expected shapes", () => {
    const remember: RememberResponse = {
      id: "uuid",
      content_hash: "sha256",
      status: "remembered",
    };
    expect(remember.status).toBe("remembered");

    const recalled: RecalledMemory = {
      id: "uuid",
      agent_id: "agent-1",
      content: "test",
      memory_type: "episodic",
      scope: "private",
      importance: 0.5,
      tags: [],
      score: 0.95,
      created_at: "2025-01-01T00:00:00Z",
      updated_at: "2025-01-01T00:00:00Z",
    };
    expect(recalled.score).toBe(0.95);

    const recall: RecallResponse = { memories: [recalled], total: 1 };
    expect(recall.total).toBe(1);

    const forget: ForgetResponse = {
      forgotten: ["uuid"],
      errors: [],
      status: "forgotten",
    };
    expect(forget.forgotten).toHaveLength(1);

    const share: ShareResponse = {
      acl_id: "uuid",
      acl_ids: ["uuid"],
      memory_id: "uuid",
      shared_with: ["agent-2"],
      permission: "read",
      status: "shared",
    };
    expect(share.status).toBe("shared");

    const checkpoint: CheckpointResponse = {
      checkpoint_id: "uuid",
      parent_id: null,
      branch_name: "main",
      status: "checkpointed",
    };
    expect(checkpoint.branch_name).toBe("main");

    const branch: BranchResponse = {
      checkpoint_id: "uuid",
      branch_name: "feature",
      source_checkpoint_id: "uuid2",
      status: "branched",
    };
    expect(branch.status).toBe("branched");

    const merge: MergeResponse = {
      checkpoint_id: "uuid",
      target_branch: "main",
      merged_memory_count: 5,
      status: "merged",
    };
    expect(merge.merged_memory_count).toBe(5);

    const replay: ReplayResponse = {
      checkpoint: {
        id: "uuid",
        branch_name: "main",
        state_snapshot: {},
        label: null,
        created_at: "2025-01-01T00:00:00Z",
      },
      memory_count: 3,
      event_count: 10,
      memories: [],
      status: "replayed",
    };
    expect(replay.memory_count).toBe(3);

    const verify: VerifyResponse = {
      valid: true,
      total_records: 100,
      verified_records: 100,
      first_broken_at: null,
      error_message: null,
      status: "verified",
    };
    expect(verify.valid).toBe(true);

    const delegate: DelegateResponse = {
      delegation_id: "uuid",
      delegator: "agent-1",
      delegate: "agent-2",
      permission: "read",
      status: "delegated",
    };
    expect(delegate.status).toBe("delegated");
  });
});

describe("Error classes", () => {
  it("MnemoToolError contains tool name", () => {
    const err = new MnemoToolError("mnemo.remember", "something broke");
    expect(err.name).toBe("MnemoToolError");
    expect(err.toolName).toBe("mnemo.remember");
    expect(err.message).toContain("mnemo.remember");
    expect(err.message).toContain("something broke");
    expect(err).toBeInstanceOf(Error);
  });

  it("MnemoRpcError contains code and optional data", () => {
    const err = new MnemoRpcError(-32600, "Invalid Request", { extra: true });
    expect(err.name).toBe("MnemoRpcError");
    expect(err.code).toBe(-32600);
    expect(err.data).toEqual({ extra: true });
    expect(err.message).toContain("-32600");
    expect(err).toBeInstanceOf(Error);
  });

  it("MnemoConnectionError is an Error", () => {
    const err = new MnemoConnectionError("not connected");
    expect(err.name).toBe("MnemoConnectionError");
    expect(err.message).toBe("not connected");
    expect(err).toBeInstanceOf(Error);
  });
});
