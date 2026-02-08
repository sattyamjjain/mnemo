/**
 * Example: Vercel AI SDK + Mnemo persistent memory.
 *
 * Vercel AI SDK agents connect to Mnemo via the MnemoClient
 * TypeScript SDK or directly via MCP.
 *
 * Requirements:
 *   npm install @mnemo/sdk
 *   # Or use MCP directly:
 *   npm install ai @ai-sdk/openai
 *   cargo build --release -p mnemo-cli
 *   export OPENAI_API_KEY=sk-...
 */

// === Option 1: Using Mnemo TypeScript SDK (REST API) ===

import { MnemoClient } from "@mnemo/sdk";

async function withMnemoSDK() {
  const client = new MnemoClient({
    command: "mnemo",
    dbPath: "vercel_demo.db",
    agentId: "vercel-agent",
  });

  await client.connect();

  // Store knowledge
  console.log("=== Store Knowledge ===");
  const stored = await client.remember({
    content: "The user Alice prefers TypeScript and Next.js",
    tags: ["user-preference", "tech-stack"],
    importance: 0.9,
  });
  console.log(`Stored: ${stored.id}`);

  // Recall knowledge
  console.log("\n=== Recall Knowledge ===");
  const recalled = await client.recall({
    query: "user tech preferences",
    limit: 5,
  });
  for (const mem of recalled.memories) {
    console.log(`  [${mem.score.toFixed(2)}] ${mem.content}`);
  }

  // Forget
  console.log("\n=== Forget ===");
  const forgotten = await client.forget({
    memoryIds: [stored.id],
  });
  console.log(`Forgotten: ${forgotten.forgotten}`);

  await client.close();
}

// === Option 2: Using Vercel AI SDK with generateText ===

import { generateText } from "ai";
import { openai } from "@ai-sdk/openai";

async function withGenerateText() {
  const client = new MnemoClient({
    command: "mnemo",
    dbPath: "vercel_demo.db",
    agentId: "vercel-agent",
  });
  await client.connect();

  // Use Mnemo as context for AI generation
  const memories = await client.recall({
    query: "user preferences and background",
    limit: 5,
  });

  const context = memories.memories
    .map((m) => m.content)
    .join("\n");

  const result = await generateText({
    model: openai("gpt-4o"),
    system: `You are a helpful assistant. Here is what you know about the user:\n${context}`,
    prompt: "Suggest a good project for me based on what you know.",
  });

  console.log(result.text);

  // Store the suggestion in memory
  await client.remember({
    content: `Suggested project: ${result.text}`,
    tags: ["suggestion"],
  });

  await client.close();
}

withMnemoSDK().catch(console.error);
