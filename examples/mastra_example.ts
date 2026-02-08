/**
 * Example: Mastra (TypeScript) + Mnemo persistent memory.
 *
 * Mastra agents connect to Mnemo via MCPClient with stdio transport,
 * providing persistent memory for TypeScript agents.
 *
 * Requirements:
 *   npm install @mastra/core @mastra/mcp @ai-sdk/openai
 *   cargo build --release -p mnemo-cli
 *   export OPENAI_API_KEY=sk-...
 */

import { Agent } from "@mastra/core/agent";
import { MCPClient } from "@mastra/mcp";
import { openai } from "@ai-sdk/openai";

// Configure Mnemo MCP client
const mnemoClient = new MCPClient({
  servers: {
    mnemo: {
      command: "mnemo",
      args: ["--db-path", "mastra_demo.db", "--agent-id", "mastra-agent"],
    },
  },
  timeout: 30000,
});

async function main() {
  // Get Mnemo tools via MCP
  const tools = await mnemoClient.listTools();

  // Create an agent with persistent memory
  const agent = new Agent({
    name: "MemoryAssistant",
    instructions: [
      "You are a helpful assistant with persistent memory.",
      "Use mnemo.remember to store important facts.",
      "Use mnemo.recall to retrieve relevant context.",
      "Use mnemo.forget to remove outdated information.",
      "Always check memory before answering questions about the user.",
    ].join("\n"),
    model: openai("gpt-4o"),
    tools,
  });

  // Session 1: Store knowledge
  console.log("=== Store Knowledge ===");
  let response = await agent.generate(
    "Remember that I'm Alice, a TypeScript developer who loves React and Next.js."
  );
  console.log(`Agent: ${response.text}\n`);

  // Session 2: Recall context
  console.log("=== Recall Context ===");
  response = await agent.generate(
    "What frameworks do I work with?"
  );
  console.log(`Agent: ${response.text}\n`);

  // Session 3: Update knowledge
  console.log("=== Update Knowledge ===");
  response = await agent.generate(
    "I've also started using Svelte. Update my preferences."
  );
  console.log(`Agent: ${response.text}`);
}

// Alternative: Dynamic tool resolution (lazy loading)
async function withLazyTools() {
  const agent = new Agent({
    name: "LazyMemoryAgent",
    instructions: "You have persistent memory tools.",
    model: openai("gpt-4o"),
    tools: async () => {
      const client = new MCPClient({
        servers: {
          mnemo: {
            command: "mnemo",
            args: ["--db-path", "mastra_demo.db"],
          },
        },
      });
      return await client.getTools();
    },
  });

  const response = await agent.generate("Remember that I prefer dark mode");
  console.log(response.text);
}

main().catch(console.error);
