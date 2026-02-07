# Go SDK

The Go SDK communicates with Mnemo via MCP STDIO, spawning the `mnemo` binary as a child process.

## Installation

```bash
go get github.com/mnemo-ai/mnemo-go
```

## Usage

```go
package main

import (
    "fmt"
    "log"

    mnemo "github.com/mnemo-ai/mnemo-go"
)

func main() {
    client, err := mnemo.NewClient(mnemo.ClientOptions{
        DbPath:  "agent.db",
        AgentID: "my-agent",
    })
    if err != nil {
        log.Fatal(err)
    }
    defer client.Close()

    // Store a memory
    importance := float32(0.8)
    result, err := client.Remember(mnemo.RememberInput{
        Content:    "User prefers dark mode",
        Importance: &importance,
    })
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("Stored: %s\n", result.ID)

    // Recall memories
    limit := 5
    memories, err := client.Recall(mnemo.RecallInput{
        Query: "user preferences",
        Limit: &limit,
    })
    if err != nil {
        log.Fatal(err)
    }
    for _, m := range memories.Memories {
        fmt.Printf("  %s (score: %.2f)\n", m.Content, m.Score)
    }
}
```

## API Reference

### ClientOptions

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `Command` | string | `"mnemo"` | Path to mnemo binary |
| `DbPath` | string | `"mnemo.db"` | Database file path |
| `AgentID` | string | `"default"` | Default agent ID |
| `OrgID` | string | - | Organization ID |
| `OpenAIKey` | string | - | OpenAI API key |
| `Dimensions` | int | 1536 | Embedding dimensions |

### Methods

- `Remember(RememberInput)` / `Recall(RecallInput)` / `Forget(ForgetInput)`
- `Share(ShareInput)` / `Checkpoint(CheckpointInput)` / `Branch(BranchInput)`
- `Merge(MergeInput)` / `Replay(ReplayInput)` / `Verify(VerifyInput)` / `Delegate(DelegateInput)`
