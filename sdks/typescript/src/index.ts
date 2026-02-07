import { spawn, type ChildProcess } from "node:child_process";

import type {
  MnemoClientOptions,
  JsonRpcRequest,
  JsonRpcResponse,
  RememberInput,
  RememberResponse,
  RecallInput,
  RecallResponse,
  ForgetInput,
  ForgetResponse,
  ShareInput,
  ShareResponse,
  CheckpointInput,
  CheckpointResponse,
  BranchInput,
  BranchResponse,
  MergeInput,
  MergeResponse,
  ReplayInput,
  ReplayResponse,
  VerifyInput,
  VerifyResponse,
  DelegateInput,
  DelegateResponse,
} from "./types.js";

import {
  MnemoToolError,
  MnemoRpcError,
  MnemoConnectionError,
} from "./types.js";

// Re-export all types for consumers
export * from "./types.js";

/** MCP protocol version used by the SDK. */
const PROTOCOL_VERSION = "2024-11-05";

/** SDK identification sent during the MCP initialize handshake. */
const CLIENT_INFO = {
  name: "mnemo-ts-sdk",
  version: "0.1.0",
} as const;

/**
 * Pending request tracker for correlating JSON-RPC responses with their
 * originating call.
 */
interface PendingRequest {
  resolve: (value: JsonRpcResponse) => void;
  reject: (reason: Error) => void;
}

/**
 * MnemoClient wraps MCP STDIO communication with the `mnemo` CLI binary,
 * providing a typed TypeScript interface to all 10 MCP tools.
 *
 * @example
 * ```ts
 * const client = new MnemoClient({ dbPath: "./my.db" });
 * await client.connect();
 *
 * const { id } = await client.remember({ content: "TypeScript is great" });
 * const { memories } = await client.recall({ query: "TypeScript" });
 *
 * await client.close();
 * ```
 */
export class MnemoClient {
  private process: ChildProcess | null = null;
  private requestId = 0;
  private pendingRequests = new Map<number, PendingRequest>();
  private buffer = "";
  private connected = false;
  private readonly options: Required<
    Pick<MnemoClientOptions, "command" | "dbPath">
  > &
    MnemoClientOptions;

  constructor(options: MnemoClientOptions = {}) {
    this.options = {
      command: "mnemo",
      dbPath: "mnemo.db",
      ...options,
    };
  }

  // -------------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------------

  /**
   * Spawn the `mnemo` child process and perform the MCP initialize handshake.
   *
   * @throws {MnemoConnectionError} If the process fails to start or the
   *   handshake does not complete.
   */
  async connect(): Promise<void> {
    if (this.connected) {
      return;
    }

    const args = this.buildArgs();
    const env = this.buildEnv();

    this.process = spawn(this.options.command, args, {
      stdio: ["pipe", "pipe", "pipe"],
      env,
    });

    if (!this.process.stdout || !this.process.stdin) {
      throw new MnemoConnectionError(
        "Failed to open stdio pipes on the child process",
      );
    }

    // Wire up stdout line parser
    this.process.stdout.on("data", (chunk: Buffer) => {
      this.onData(chunk);
    });

    // Handle unexpected exit
    this.process.on("error", (err) => {
      this.rejectAllPending(
        new MnemoConnectionError(`Child process error: ${err.message}`),
      );
    });

    this.process.on("exit", (code) => {
      this.connected = false;
      this.rejectAllPending(
        new MnemoConnectionError(
          `Child process exited with code ${String(code)}`,
        ),
      );
    });

    // Perform MCP initialize handshake
    await this.initialize();
    this.connected = true;
  }

  /**
   * Gracefully shut down the child process and clean up resources.
   */
  async close(): Promise<void> {
    if (!this.process) {
      return;
    }
    this.connected = false;
    this.process.kill("SIGTERM");
    this.process = null;
    this.rejectAllPending(
      new MnemoConnectionError("Client closed"),
    );
    this.pendingRequests.clear();
    this.buffer = "";
  }

  // -------------------------------------------------------------------------
  // MCP tool methods
  // -------------------------------------------------------------------------

  /**
   * Store a new memory.
   *
   * @param input - The memory content and optional metadata.
   * @returns The ID and content hash of the stored memory.
   */
  async remember(input: RememberInput): Promise<RememberResponse> {
    return this.callTool<RememberResponse>("mnemo.remember", input);
  }

  /**
   * Search and retrieve memories by semantic similarity, keyword matching,
   * or hybrid strategies.
   *
   * @param input - The query string and optional filters.
   * @returns Ranked list of matching memories.
   */
  async recall(input: RecallInput): Promise<RecallResponse> {
    return this.callTool<RecallResponse>("mnemo.recall", input);
  }

  /**
   * Delete one or more memories by ID, optionally using criteria-based
   * selection and various deletion strategies.
   *
   * @param input - Memory IDs and/or criteria for deletion.
   * @returns Lists of successfully forgotten and failed memory IDs.
   */
  async forget(input: ForgetInput): Promise<ForgetResponse> {
    return this.callTool<ForgetResponse>("mnemo.forget", input);
  }

  /**
   * Share a memory with another agent by granting access permissions.
   *
   * @param input - The memory ID, target agent(s), and permission level.
   * @returns ACL entry details and sharing status.
   */
  async share(input: ShareInput): Promise<ShareResponse> {
    return this.callTool<ShareResponse>("mnemo.share", input);
  }

  /**
   * Create a checkpoint to snapshot the current agent state.
   *
   * @param input - Thread ID, state snapshot, and optional label.
   * @returns The checkpoint ID and branch information.
   */
  async checkpoint(input: CheckpointInput): Promise<CheckpointResponse> {
    return this.callTool<CheckpointResponse>("mnemo.checkpoint", input);
  }

  /**
   * Fork the current state into a new branch for exploration.
   *
   * @param input - Thread ID, new branch name, and source checkpoint/branch.
   * @returns The new branch checkpoint details.
   */
  async branch(input: BranchInput): Promise<BranchResponse> {
    return this.callTool<BranchResponse>("mnemo.branch", input);
  }

  /**
   * Merge a branch back into another branch.
   *
   * @param input - Thread ID, source/target branches, and merge strategy.
   * @returns Merge result with the created checkpoint and memory count.
   */
  async merge(input: MergeInput): Promise<MergeResponse> {
    return this.callTool<MergeResponse>("mnemo.merge", input);
  }

  /**
   * Reconstruct the agent context at a specific checkpoint.
   *
   * @param input - Thread ID and optional checkpoint/branch specifiers.
   * @returns The checkpoint state, referenced memories, and events.
   */
  async replay(input: ReplayInput): Promise<ReplayResponse> {
    return this.callTool<ReplayResponse>("mnemo.replay", input);
  }

  /**
   * Verify the hash chain integrity of stored memories.
   *
   * @param input - Optional agent ID and thread ID to scope verification.
   * @returns Verification result with record counts and validity status.
   */
  async verify(input: VerifyInput = {}): Promise<VerifyResponse> {
    return this.callTool<VerifyResponse>("mnemo.verify", input);
  }

  /**
   * Delegate permissions to another agent with optional scoping and
   * time bounds.
   *
   * @param input - Delegate agent, permission, scope, and expiry settings.
   * @returns The delegation record details.
   */
  async delegate(input: DelegateInput): Promise<DelegateResponse> {
    return this.callTool<DelegateResponse>("mnemo.delegate", input);
  }

  // -------------------------------------------------------------------------
  // Internal: MCP handshake
  // -------------------------------------------------------------------------

  /**
   * Perform the MCP initialize/initialized handshake sequence.
   */
  private async initialize(): Promise<void> {
    // Step 1: Send `initialize` request and await the response
    const initResponse = await this.sendRequest("initialize", {
      protocolVersion: PROTOCOL_VERSION,
      capabilities: {},
      clientInfo: CLIENT_INFO,
    });

    if (initResponse.error) {
      throw new MnemoConnectionError(
        `MCP initialize failed: ${initResponse.error.message}`,
      );
    }

    // Step 2: Send `notifications/initialized` (no response expected)
    this.sendNotification("notifications/initialized");
  }

  // -------------------------------------------------------------------------
  // Internal: JSON-RPC transport
  // -------------------------------------------------------------------------

  /**
   * Send a JSON-RPC request and return a promise that resolves when the
   * matching response arrives.
   */
  private sendRequest(
    method: string,
    params?: unknown,
  ): Promise<JsonRpcResponse> {
    return new Promise<JsonRpcResponse>((resolve, reject) => {
      if (!this.process?.stdin) {
        reject(
          new MnemoConnectionError("Not connected -- call connect() first"),
        );
        return;
      }

      const id = this.requestId++;
      const request: JsonRpcRequest = {
        jsonrpc: "2.0",
        method,
        params,
        id,
      };

      this.pendingRequests.set(id, { resolve, reject });

      const payload = JSON.stringify(request) + "\n";
      this.process.stdin.write(payload, (err) => {
        if (err) {
          this.pendingRequests.delete(id);
          reject(
            new MnemoConnectionError(`Failed to write to stdin: ${err.message}`),
          );
        }
      });
    });
  }

  /**
   * Send a JSON-RPC notification (no id, no response expected).
   */
  private sendNotification(method: string): void {
    if (!this.process?.stdin) {
      throw new MnemoConnectionError("Not connected -- call connect() first");
    }

    const notification: JsonRpcRequest = {
      jsonrpc: "2.0",
      method,
    };

    const payload = JSON.stringify(notification) + "\n";
    this.process.stdin.write(payload);
  }

  /**
   * Call an MCP tool via `tools/call` and parse the typed response.
   */
  private async callTool<T>(
    toolName: string,
    args: unknown,
  ): Promise<T> {
    this.ensureConnected();

    const response = await this.sendRequest("tools/call", {
      name: toolName,
      arguments: args,
    });

    if (response.error) {
      throw new MnemoRpcError(
        response.error.code,
        response.error.message,
        response.error.data,
      );
    }

    const result = response.result;
    if (!result?.content || result.content.length === 0) {
      throw new MnemoToolError(
        toolName,
        "Empty response from tool",
      );
    }

    // MCP tool results come as content[0].text containing JSON
    const textContent = result.content[0];
    if (!textContent || textContent.type !== "text") {
      throw new MnemoToolError(
        toolName,
        `Unexpected content type: ${textContent?.type ?? "undefined"}`,
      );
    }

    // Check for isError flag on the tool result
    if (result.isError) {
      throw new MnemoToolError(toolName, textContent.text);
    }

    try {
      return JSON.parse(textContent.text) as T;
    } catch {
      throw new MnemoToolError(
        toolName,
        `Failed to parse response JSON: ${textContent.text}`,
      );
    }
  }

  // -------------------------------------------------------------------------
  // Internal: STDIO data parsing
  // -------------------------------------------------------------------------

  /**
   * Handle incoming data from the child process stdout. Buffers partial
   * lines and dispatches complete JSON-RPC messages.
   */
  private onData(chunk: Buffer): void {
    this.buffer += chunk.toString("utf-8");

    // Process complete lines (JSONL -- one JSON object per line)
    let newlineIndex: number;
    while ((newlineIndex = this.buffer.indexOf("\n")) !== -1) {
      const line = this.buffer.slice(0, newlineIndex).trim();
      this.buffer = this.buffer.slice(newlineIndex + 1);

      if (line.length === 0) {
        continue;
      }

      this.processLine(line);
    }
  }

  /**
   * Parse a single JSON-RPC line and resolve the matching pending request.
   */
  private processLine(line: string): void {
    let message: JsonRpcResponse;
    try {
      message = JSON.parse(line) as JsonRpcResponse;
    } catch {
      // Non-JSON output (e.g. logging to stdout) -- ignore
      return;
    }

    // Only process responses (messages with an `id`)
    if (message.id === undefined || message.id === null) {
      // This is a server-initiated notification -- ignore
      return;
    }

    const pending = this.pendingRequests.get(message.id);
    if (pending) {
      this.pendingRequests.delete(message.id);
      pending.resolve(message);
    }
  }

  // -------------------------------------------------------------------------
  // Internal: helpers
  // -------------------------------------------------------------------------

  /**
   * Build the CLI argument array from the configured options.
   */
  private buildArgs(): string[] {
    const args: string[] = [];

    if (this.options.dbPath) {
      args.push("--db-path", this.options.dbPath);
    }
    if (this.options.agentId) {
      args.push("--agent-id", this.options.agentId);
    }
    if (this.options.orgId) {
      args.push("--org-id", this.options.orgId);
    }
    if (this.options.dimensions !== undefined) {
      args.push("--dimensions", String(this.options.dimensions));
    }
    if (this.options.embeddingModel) {
      args.push("--embedding-model", this.options.embeddingModel);
    }
    if (this.options.postgresUrl) {
      args.push("--postgres-url", this.options.postgresUrl);
    }

    return args;
  }

  /**
   * Build the environment variables for the child process, merging the
   * current process env with any SDK-specific overrides.
   */
  private buildEnv(): NodeJS.ProcessEnv {
    const env: NodeJS.ProcessEnv = { ...process.env };

    if (this.options.openaiApiKey) {
      env["OPENAI_API_KEY"] = this.options.openaiApiKey;
    }

    if (this.options.env) {
      Object.assign(env, this.options.env);
    }

    return env;
  }

  /**
   * Assert that the client is connected, throwing if not.
   */
  private ensureConnected(): void {
    if (!this.connected || !this.process) {
      throw new MnemoConnectionError("Not connected -- call connect() first");
    }
  }

  /**
   * Reject all pending requests with the given error.
   */
  private rejectAllPending(error: Error): void {
    for (const [, pending] of this.pendingRequests) {
      pending.reject(error);
    }
    this.pendingRequests.clear();
  }
}
