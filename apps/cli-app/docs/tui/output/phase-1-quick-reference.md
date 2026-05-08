# Phase 1 Quick Reference

## Using FileIOService

```typescript
import { FileIOService } from "../io/file-io-service.js";

// Initialize
const fileIO = new FileIOService({ baseDir: ".wf-agent" });
await fileIO.initialize();

// Get session paths
const paths = fileIO.getSessionPaths("session-test-001");
// Returns: {
//   functional: { humanRelayOutput, humanRelayInput },
//   display: { output }
// }

// Write Human Relay prompt
await fileIO.writeHumanRelayOutput({
  sessionId: "session-test-001",
  content: "Your prompt here..."
});

// Watch for user response
fileIO.watchHumanRelayInput({
  sessionId: "session-test-001",
  timeout: 300000, // 5 minutes
  onResponse: (content) => {
    console.log("User response:", content);
  },
  onTimeout: () => {
    console.log("Timeout!");
  }
});

// Update display output
await fileIO.updateDisplayOutput({
  sessionId: "session-test-001",
  sections: [
    { title: "Status", content: "Running..." }
  ],
  append: true
});

// Cleanup
await fileIO.close();
```

## Creating Message Handlers

```typescript
import type { OutputHandler, BaseComponentMessage } from "@wf-agent/types";
import { OutputTarget } from "@wf-agent/types";

export class MyHandler implements OutputHandler {
  readonly target = OutputTarget.TUI; // or FILE_FUNCTIONAL, FILE_DISPLAY
  readonly name = "my-handler";

  supports(message: BaseComponentMessage): boolean {
    return message.type === "my.custom.message";
  }

  async handle(message: BaseComponentMessage): Promise<void> {
    // Process message
    console.log("Received:", message);
  }
}
```

## Registering Handlers with MessageBus

```typescript
import { MessageBus } from "@wf-agent/sdk";
import { CLI_ROUTING_RULES } from "../config/routing-rules.js";

const bus = new MessageBus(CLI_ROUTING_RULES);

// Register handlers
bus.registerHandler(new TUIHandler(tui));
bus.registerHandler(new FunctionalFileHandler(fileIO));
bus.registerHandler(new DisplayFileHandler(fileIO));

// Publish messages (SDK does this automatically)
bus.publish({
  type: "agent.llm.stream",
  category: MessageCategory.AGENT,
  entity: { id: "agent-123", type: "agent" },
  data: { chunk: "Hello" },
  level: "info"
});
```

## Subscribing to Messages in Screens

```typescript
// In your screen component
private setupSubscriptions() {
  this.subscription = this.messageBus.subscribe(
    {
      categories: [MessageCategory.AGENT],
      entityIds: [this.currentAgentId]
    },
    (message) => this.handleMessage(message)
  );
}

private handleMessage(message: BaseComponentMessage) {
  switch (message.type) {
    case "agent.llm.stream":
      const data = message.data as AgentLLMStreamData;
      this.appendLog(data.chunk);
      break;
  }
}

// Don't forget to unsubscribe!
onDeactivate() {
  this.subscription?.unsubscribe();
}
```

## Directory Structure

```
.wf-agent/
├── function/
│   └── session-{name}-{timestamp}/
│       ├── human-relay-output.txt  (prompt to copy)
│       └── human-relay-input.txt   (user response)
└── display/
    └── session-{name}-{timestamp}/
        ├── output.md               (aggregated output)
        └── execution-log.md        (detailed log)
```

## Session Naming

Sessions follow the pattern: `session-{name}-{timestamp}`

Example: `session-data-processing-1705312345678`

## Key Files

- **FileIOService**: `src/io/file-io-service.ts`
- **Message Handlers**: `src/messaging/handlers/`
- **TUI Human Relay**: `src/tui/handlers/tui-human-relay-handler.ts`
- **Routing Rules**: `src/config/routing-rules.ts`
- **App Integration**: `src/tui/app.ts`

## Common Message Types

| Type | Category | Handler | Description |
|------|----------|---------|-------------|
| `agent.llm.stream` | AGENT | TUI | Streaming LLM output |
| `agent.tool.call_start` | AGENT | TUI | Tool call started |
| `agent.tool.result` | AGENT | FILE_DISPLAY | Tool execution result |
| `agent.human_relay.request` | AGENT | All three | Human Relay needed |
| `workflow-execution.node.start` | WORKFLOW | TUI + FILE_DISPLAY | Node execution started |
| `system.error` | SYSTEM | TUI + FILE_DISPLAY | Error occurred |

## Testing Commands

```bash
# Build the project
cd apps/cli-app
npm run build

# Run TypeScript check
npx tsc --noEmit

# Start TUI mode
npm run cli -- --tui
```
