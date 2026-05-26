# AI Agent with AgentFS on Cloudflare

This example demonstrates running an AI agent with persistent filesystem storage on Cloudflare Workers using:

- **AgentFS** for persistent file storage (backed by Durable Objects SQLite)
- **just-bash** for bash command execution in a virtual environment
- **Workers AI** for LLM inference (Llama 3.1 70B)
- **AI SDK** for the agent framework

## Setup

```bash
npm install
```

## Development

```bash
npm run dev
```

## Deploy

```bash
npm run deploy
```

## Usage

Send a message to the agent:

```bash
curl -X POST https://your-worker.workers.dev/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Create a file called hello.txt with Hello World"}'
```

The agent can:
- Create, read, update, and delete files
- Create directories
- List directory contents
- All files persist across sessions in Durable Objects SQLite storage
