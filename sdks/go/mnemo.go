package mnemo

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os/exec"
	"sync"
)

// ClientOptions configures the Mnemo MCP client.
type ClientOptions struct {
	// Command is the path or name of the mnemo binary. Defaults to "mnemo".
	Command string

	// DbPath is the path to the database file. Passed as --db-path.
	DbPath string

	// AgentID is the default agent identifier. Passed as --agent-id.
	AgentID string

	// OrgID is the default organization identifier. Passed as --org-id.
	OrgID string

	// OpenAIKey is the OpenAI API key for embeddings. Passed as
	// --openai-api-key.
	OpenAIKey string

	// Dimensions sets the embedding vector dimensions. Passed as --dimensions.
	Dimensions int
}

// Client communicates with a mnemo MCP server process over STDIO.
//
// All exported methods are safe for concurrent use. The client manages the
// lifecycle of the child process; call Close when done.
type Client struct {
	cmd    *exec.Cmd
	stdin  io.WriteCloser
	stdout *bufio.Scanner
	nextID int
	mu     sync.Mutex
}

// NewClient spawns a mnemo MCP server as a child process and performs the MCP
// initialization handshake.
//
// The caller must call Close when finished to terminate the child process and
// release resources.
func NewClient(opts ClientOptions) (*Client, error) {
	command := opts.Command
	if command == "" {
		command = "mnemo"
	}

	args := buildArgs(opts)

	cmd := exec.Command(command, args...)
	cmd.Stderr = nil // let mnemo's stderr go to /dev/null by default

	stdinPipe, err := cmd.StdinPipe()
	if err != nil {
		return nil, fmt.Errorf("mnemo: failed to create stdin pipe: %w", err)
	}

	stdoutPipe, err := cmd.StdoutPipe()
	if err != nil {
		_ = stdinPipe.Close()
		return nil, fmt.Errorf("mnemo: failed to create stdout pipe: %w", err)
	}

	if err := cmd.Start(); err != nil {
		_ = stdinPipe.Close()
		return nil, fmt.Errorf("mnemo: failed to start process: %w", err)
	}

	scanner := bufio.NewScanner(stdoutPipe)
	// Increase buffer for large JSON responses (1 MB).
	scanner.Buffer(make([]byte, 0, 1024*1024), 1024*1024)

	c := &Client{
		cmd:    cmd,
		stdin:  stdinPipe,
		stdout: scanner,
		nextID: 0,
	}

	if err := c.initialize(); err != nil {
		_ = c.Close()
		return nil, fmt.Errorf("mnemo: initialization failed: %w", err)
	}

	return c, nil
}

// Close terminates the child process and releases all resources.
func (c *Client) Close() error {
	c.mu.Lock()
	defer c.mu.Unlock()

	_ = c.stdin.Close()
	return c.cmd.Wait()
}

// Remember stores a new memory and returns its ID and content hash.
func (c *Client) Remember(input RememberInput) (*RememberResponse, error) {
	var resp RememberResponse
	if err := c.callTool("mnemo.remember", input, &resp); err != nil {
		return nil, err
	}
	return &resp, nil
}

// Recall searches memories by semantic similarity and filters.
func (c *Client) Recall(input RecallInput) (*RecallResponse, error) {
	var resp RecallResponse
	if err := c.callTool("mnemo.recall", input, &resp); err != nil {
		return nil, err
	}
	return &resp, nil
}

// Forget deletes or archives memories by ID or criteria.
func (c *Client) Forget(input ForgetInput) (*ForgetResponse, error) {
	var resp ForgetResponse
	if err := c.callTool("mnemo.forget", input, &resp); err != nil {
		return nil, err
	}
	return &resp, nil
}

// Share grants another agent access to a memory.
func (c *Client) Share(input ShareInput) (*ShareResponse, error) {
	var resp ShareResponse
	if err := c.callTool("mnemo.share", input, &resp); err != nil {
		return nil, err
	}
	return &resp, nil
}

// Checkpoint creates a snapshot of the current agent state.
func (c *Client) Checkpoint(input CheckpointInput) (*CheckpointResponse, error) {
	var resp CheckpointResponse
	if err := c.callTool("mnemo.checkpoint", input, &resp); err != nil {
		return nil, err
	}
	return &resp, nil
}

// Branch forks the current state into a new named branch.
func (c *Client) Branch(input BranchInput) (*BranchResponse, error) {
	var resp BranchResponse
	if err := c.callTool("mnemo.branch", input, &resp); err != nil {
		return nil, err
	}
	return &resp, nil
}

// Merge combines a source branch into a target branch.
func (c *Client) Merge(input MergeInput) (*MergeResponse, error) {
	var resp MergeResponse
	if err := c.callTool("mnemo.merge", input, &resp); err != nil {
		return nil, err
	}
	return &resp, nil
}

// Replay reconstructs the agent context at a specific checkpoint.
func (c *Client) Replay(input ReplayInput) (*ReplayResponse, error) {
	var resp ReplayResponse
	if err := c.callTool("mnemo.replay", input, &resp); err != nil {
		return nil, err
	}
	return &resp, nil
}

// Verify checks the hash chain integrity of stored memories.
func (c *Client) Verify(input VerifyInput) (*VerifyResponse, error) {
	var resp VerifyResponse
	if err := c.callTool("mnemo.verify", input, &resp); err != nil {
		return nil, err
	}
	return &resp, nil
}

// Delegate grants scoped, time-bounded permissions to another agent.
func (c *Client) Delegate(input DelegateInput) (*DelegateResponse, error) {
	var resp DelegateResponse
	if err := c.callTool("mnemo.delegate", input, &resp); err != nil {
		return nil, err
	}
	return &resp, nil
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

// buildArgs constructs the CLI arguments from the options.
func buildArgs(opts ClientOptions) []string {
	var args []string
	if opts.DbPath != "" {
		args = append(args, "--db-path", opts.DbPath)
	}
	if opts.AgentID != "" {
		args = append(args, "--agent-id", opts.AgentID)
	}
	if opts.OrgID != "" {
		args = append(args, "--org-id", opts.OrgID)
	}
	if opts.OpenAIKey != "" {
		args = append(args, "--openai-api-key", opts.OpenAIKey)
	}
	if opts.Dimensions > 0 {
		args = append(args, "--dimensions", fmt.Sprintf("%d", opts.Dimensions))
	}
	return args
}

// initialize performs the MCP initialization handshake with the server.
//
// It sends the "initialize" request and the "notifications/initialized"
// notification as required by the MCP specification.
func (c *Client) initialize() error {
	initReq := jsonRPCRequest{
		JSONRPC: "2.0",
		Method:  "initialize",
		Params: map[string]interface{}{
			"protocolVersion": "2024-11-05",
			"capabilities":    map[string]interface{}{},
			"clientInfo": map[string]interface{}{
				"name":    "mnemo-go-sdk",
				"version": "0.1.0",
			},
		},
		ID: intPtr(c.allocID()),
	}

	if err := c.sendRequest(initReq); err != nil {
		return fmt.Errorf("send initialize: %w", err)
	}

	// Read the initialize response.
	if _, err := c.readRawResponse(); err != nil {
		return fmt.Errorf("read initialize response: %w", err)
	}

	// Send the initialized notification (no id, no response expected).
	notif := jsonRPCRequest{
		JSONRPC: "2.0",
		Method:  "notifications/initialized",
	}

	if err := c.sendRequest(notif); err != nil {
		return fmt.Errorf("send initialized notification: %w", err)
	}

	return nil
}

// allocID returns the next request ID and increments the counter.
// Must be called with c.mu held.
func (c *Client) allocID() int {
	id := c.nextID
	c.nextID++
	return id
}

// callTool sends a tools/call JSON-RPC request and unmarshals the text content
// of the first content item into dest.
func (c *Client) callTool(name string, arguments interface{}, dest interface{}) error {
	c.mu.Lock()
	defer c.mu.Unlock()

	id := c.allocID()

	req := jsonRPCRequest{
		JSONRPC: "2.0",
		Method:  "tools/call",
		Params: toolCallParams{
			Name:      name,
			Arguments: arguments,
		},
		ID: intPtr(id),
	}

	if err := c.sendRequest(req); err != nil {
		return fmt.Errorf("mnemo %s: send: %w", name, err)
	}

	raw, err := c.readRawResponse()
	if err != nil {
		return fmt.Errorf("mnemo %s: read: %w", name, err)
	}

	var rpcResp jsonRPCResponse
	if err := json.Unmarshal(raw, &rpcResp); err != nil {
		return fmt.Errorf("mnemo %s: unmarshal response: %w", name, err)
	}

	if rpcResp.Error != nil {
		return fmt.Errorf("mnemo %s: rpc error %d: %s", name, rpcResp.Error.Code, rpcResp.Error.Message)
	}

	if rpcResp.Result == nil {
		return fmt.Errorf("mnemo %s: empty result", name)
	}

	if len(rpcResp.Result.Content) == 0 {
		return fmt.Errorf("mnemo %s: no content in result", name)
	}

	text := rpcResp.Result.Content[0].Text
	if err := json.Unmarshal([]byte(text), dest); err != nil {
		return fmt.Errorf("mnemo %s: unmarshal content: %w", name, err)
	}

	return nil
}

// sendRequest marshals and writes a JSON-RPC request followed by a newline to
// the child process stdin.
func (c *Client) sendRequest(req jsonRPCRequest) error {
	data, err := json.Marshal(req)
	if err != nil {
		return fmt.Errorf("marshal request: %w", err)
	}

	data = append(data, '\n')

	if _, err := c.stdin.Write(data); err != nil {
		return fmt.Errorf("write to stdin: %w", err)
	}

	return nil
}

// readRawResponse reads the next newline-delimited JSON-RPC response from the
// child process stdout.
func (c *Client) readRawResponse() ([]byte, error) {
	if !c.stdout.Scan() {
		if err := c.stdout.Err(); err != nil {
			return nil, fmt.Errorf("scan stdout: %w", err)
		}
		return nil, fmt.Errorf("unexpected EOF from mnemo process")
	}
	return c.stdout.Bytes(), nil
}

// intPtr returns a pointer to the given int value.
func intPtr(v int) *int {
	return &v
}
