/**
 * AI Agent with AgentFS on Cloudflare Workers
 *
 * This example shows how to run an AI agent with just-bash and AgentFS
 * on Cloudflare Workers using Durable Objects for persistent storage
 * and Workers AI for inference.
 *
 * Deploy:
 *   npm install
 *   npm run deploy
 *
 * Then send prompts:
 *   curl -X POST https://your-worker.workers.dev/chat \
 *     -H "Content-Type: application/json" \
 *     -d '{"message": "List all files in the root directory"}'
 */

import { DurableObject } from "cloudflare:workers";
import { streamText, stepCountIs } from "ai";
import { createWorkersAI } from "workers-ai-provider";
import { createBashTool } from "bash-tool";
import { Bash } from "just-bash";
import { AgentFS, type CloudflareStorage } from "agentfs-sdk/cloudflare";

export interface Env {
  AGENT_FS: DurableObjectNamespace<AgentFSDurableObject>;
  AI: Ai;
}

/**
 * Durable Object that provides an AI agent with persistent filesystem.
 */
export class AgentFSDurableObject extends DurableObject<Env> {
  private fs: AgentFS;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    this.fs = AgentFS.create(ctx.storage as CloudflareStorage);
  }

  async chat(message: string): Promise<ReadableStream<Uint8Array>> {
    // Create a just-bash Bash instance with AgentFS filesystem
    const bash = new Bash({ fs: this.fs });

    // Create the bash tool with the just-bash sandbox
    const bashToolkit = await createBashTool({
      sandbox: bash,
      extraInstructions: `You are an AI agent with a persistent filesystem.
Files you create will persist across sessions.

Use bash commands to interact with the filesystem:
- ls to list files
- cat <file> to read files
- echo "content" > file.txt to write files
- mkdir <dir> to create directories
- rm <file> to remove files`,
    });

    // Create Workers AI provider
    const workersai = createWorkersAI({ binding: this.env.AI });

    // Stream the response
    const result = streamText({
      model: workersai("@cf/meta/llama-3.1-70b-instruct" as Parameters<typeof workersai>[0]),
      tools: bashToolkit.tools,
      messages: [{ role: "user", content: message }],
      stopWhen: stepCountIs(10),
    });

    return result.toTextStreamResponse().body!;
  }
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);

    if (url.pathname === "/chat" && request.method === "POST") {
      const { message } = await request.json<{ message: string }>();

      if (!message) {
        return new Response("Missing message", { status: 400 });
      }

      // Route to the Durable Object
      const id = env.AGENT_FS.idFromName("default");
      const stub = env.AGENT_FS.get(id);
      const stream = await stub.chat(message);

      return new Response(stream, {
        headers: {
          "Content-Type": "text/event-stream",
          "Cache-Control": "no-cache",
        },
      });
    }

    return new Response(
      `AI Agent with AgentFS on Cloudflare

POST /chat - Send a message to the agent

Example:
  curl -X POST ${url.origin}/chat \\
    -H "Content-Type: application/json" \\
    -d '{"message": "Create a file called hello.txt with Hello World"}'
`,
      { headers: { "Content-Type": "text/plain" } }
    );
  },
};
