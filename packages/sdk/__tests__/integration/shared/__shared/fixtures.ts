/**
 * Fixtures for sdk/shared integration tests
 * Provides common test data and mock builders
 */

import type { LLMMessage, Tool, AgentLoopRuntimeConfig } from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";

/**
 * Create mock LLM message
 */
export function createMockMessage(overrides?: Partial<LLMMessage>): LLMMessage {
  return {
    role: "user",
    content: "test message",
    ...overrides,
  };
}

/**
 * Create mock tool
 */
export function createMockTool(overrides?: Partial<Tool>): Tool {
  return {
    id: "tool-test-" + Math.random().toString(36).slice(2),
    name: "test_tool",
    description: "Test tool",
    category: "test",
    inputSchema: {
      type: "object",
      properties: {
        input: { type: "string" },
      },
      required: ["input"],
    },
    ...overrides,
  };
}

/**
 * Create mock agent configuration
 */
export function createMockAgentConfig(
  overrides?: Partial<AgentLoopRuntimeConfig>,
): AgentLoopRuntimeConfig {
  return {
    id: "test-config-" + Math.random().toString(36).slice(2),
    name: "Test Agent",
    description: "Test agent for integration testing",
    tools: [],
    transformContext: async (ctx) => ctx,
    convertToLlm: async (ctx) => ({
      role: "user",
      content: "test",
    }),
    ...overrides,
  };
}

/**
 * Create mock execution context
 */
export function createMockExecutionContext(id: string = "test-exec-1") {
  return {
    id,
    status: AgentLoopStatus.RUNNING,
    createdAt: new Date(),
    state: {
      messageCount: 0,
      messageMetadata: { fingerprint: "" },
      usedTools: [] as string[],
    },
    childExecutionIds: [] as string[],
    parentId: undefined as string | undefined,
  };
}

/**
 * Create mock checkpoint error context
 */
export function createMockCheckpointErrorContext() {
  return {
    checkpointId: "checkpoint-1",
    entityId: "entity-1",
    operation: "save" as const,
    triggerEvent: "auto_save",
  };
}

/**
 * Create mock validation result
 */
export function createMockValidationResult(valid: boolean = true) {
  return {
    valid,
    errors: valid ? [] : [{ type: "TEST_ERROR", message: "Test error" }],
    warnings: [] as string[],
    timestamp: Date.now(),
  };
}

/**
 * Create message sequence for testing
 */
export function createMessageSequence(count: number): LLMMessage[] {
  const messages: LLMMessage[] = [];
  for (let i = 0; i < count; i++) {
    messages.push(
      createMockMessage({
        role: i % 2 === 0 ? "user" : "assistant",
        content: `Message ${i + 1}`,
      }),
    );
  }
  return messages;
}

/**
 * Create tool set for testing
 */
export function createToolSet(count: number): Tool[] {
  const tools: Tool[] = [];
  for (let i = 0; i < count; i++) {
    tools.push(
      createMockTool({
        id: `tool-${i + 1}`,
        name: `tool_${i + 1}`,
      }),
    );
  }
  return tools;
}

/**
 * Create hook trigger definition
 */
export function createMockTriggerDefinition() {
  return {
    id: "trigger-test",
    type: "event" as const,
    eventType: "on_iteration",
    condition: {
      type: "expression" as const,
      value: "true",
    },
  };
}

/**
 * Create hook definition
 */
export function createMockHookDefinition() {
  return {
    id: "hook-test",
    name: "Test Hook",
    description: "Test hook for integration testing",
    events: ["on_iteration"],
    condition: {
      type: "expression" as const,
      value: "true",
    },
    payload: {
      type: "template" as const,
      template: {
        message: "Hook executed",
      },
    },
    handler: {
      type: "http" as const,
      method: "POST",
      url: "http://localhost:8080/webhook",
    },
  };
}

/**
 * Helper class for managing test fixtures
 */
export class SharedTestFixtures {
  private createdEntities: any[] = [];

  add(entity: any) {
    this.createdEntities.push(entity);
    return entity;
  }

  async cleanup() {
    for (const entity of this.createdEntities.reverse()) {
      try {
        if (typeof entity?.cleanup === "function") {
          await entity.cleanup();
        } else if (typeof entity?.dispose === "function") {
          await entity.dispose();
        }
      } catch (error) {
        console.warn("Fixture cleanup error:", error);
      }
    }
    this.createdEntities = [];
  }

  clear() {
    this.createdEntities = [];
  }
}
