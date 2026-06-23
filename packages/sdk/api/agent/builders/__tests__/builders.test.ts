/**
 * Unit tests for Agent builders
 */

import { describe, it, expect } from "vitest";
import {
  AgentDefinitionBuilder,
  AgentToolConfigBuilder,
  AgentHookBuilder,
  AgentTriggerBuilder,
} from "../index.js";

describe("AgentToolConfigBuilder", () => {
  it("should build basic tool config", () => {
    const config = AgentToolConfigBuilder.create()
      .addTools(["read_file", "write_file"])
      .build();

    expect(config.tools).toEqual(["read_file", "write_file"]);
    expect(config.requireApproval).toBeUndefined();
  });

  it("should build tool config with approval", () => {
    const config = AgentToolConfigBuilder.create()
      .addTools(["read_file", "write_file"])
      .requireApproval("write_file")
      .build();

    expect(config.tools).toEqual(["read_file", "write_file"]);
    expect(config.requireApproval).toEqual(["write_file"]);
  });

  it("should reject approval for non-existent tool", () => {
    expect(() => {
      AgentToolConfigBuilder.create()
        .addTools(["read_file"])
        .requireApproval("non_existent")
        .build();
    }).toThrow();
  });

  it("should build tool config with allowed workflows", () => {
    const config = AgentToolConfigBuilder.create()
      .addTools(["execute_workflow"])
      .allowedWorkflows(["workflow-1", "workflow-2"])
      .build();

    expect(config.allowedWorkflows).toEqual(["workflow-1", "workflow-2"]);
  });

  it("should clone builder", () => {
    const original = AgentToolConfigBuilder.create()
      .addTools(["read_file", "write_file"])
      .requireApproval("write_file");

    const cloned = original.clone();
    cloned.addTools(["execute_command"]);

    const originalConfig = original.build();
    const clonedConfig = cloned.build();

    expect(originalConfig.tools).toEqual(["read_file", "write_file"]);
    expect(clonedConfig.tools).toEqual(["read_file", "write_file", "execute_command"]);
  });
});

describe("AgentHookBuilder", () => {
  it("should build hook with hookType via static method", () => {
    const hook = AgentHookBuilder.beforeIteration()
      .eventName("log-iteration")
      .build();

    expect(hook.hookType).toBe("BEFORE_ITERATION");
    expect(hook.eventName).toBe("log-iteration");
    expect(hook.enabled).toBe(true);
  });

  it("should build hook with condition", () => {
    const hook = AgentHookBuilder.afterLlmCall()
      .eventName("check-response")
      .condition("response.length > 100")
      .build();

    expect(hook.hookType).toBe("AFTER_LLM_CALL");
    expect(hook.condition).toBe("response.length > 100");
  });

  it("should build hook with checkpoint", () => {
    const hook = AgentHookBuilder.beforeToolCall()
      .eventName("save-state")
      .checkpointDescription("Before tool execution")
      .build();

    expect(hook.createCheckpoint).toBe(true);
    expect(hook.checkpointDescription).toBe("Before tool execution");
  });

  it("should fail without event name", () => {
    expect(() => {
      AgentHookBuilder.beforeIteration().build();
    }).toThrow("Event name is required");
  });

  it("should build disabled hook", () => {
    const hook = AgentHookBuilder.afterIteration()
      .eventName("cleanup")
      .disable()
      .build();

    expect(hook.enabled).toBe(false);
  });

  it("should clone hook builder", () => {
    const original = AgentHookBuilder.beforeIteration()
      .eventName("log")
      .weight(10);

    const cloned = original.clone();
    cloned.eventName("log-v2");

    const originalHook = original.build();
    const clonedHook = cloned.build();

    expect(originalHook.eventName).toBe("log");
    expect(clonedHook.eventName).toBe("log-v2");
  });
});

describe("AgentTriggerBuilder", () => {
  it("should build condition-based trigger", () => {
    const trigger = AgentTriggerBuilder.onCondition("max-iterations")
      .condition("iteration > 10")
      .pause()
      .build();

    expect(trigger.type).toBe("condition");
    expect(trigger.condition).toBe("iteration > 10");
    expect(trigger.action.type).toBe("pause");
    expect(trigger.enabled).toBe(true);
  });

  it("should build event-based trigger", () => {
    const trigger = AgentTriggerBuilder.onEvent("error-trigger")
      .eventName("error_occurred")
      .stop()
      .build();

    expect(trigger.type).toBe("event");
    expect(trigger.eventName).toBe("error_occurred");
    expect(trigger.action.type).toBe("stop");
  });

  it("should build trigger with checkpoint action", () => {
    const trigger = AgentTriggerBuilder.create("checkpoint-trigger")
      .type("schedule")
      .checkpoint({ description: "Periodic save" })
      .build();

    expect(trigger.action.type).toBe("checkpoint");
    expect(trigger.action.config?.description).toBe("Periodic save");
  });

  it("should build disabled trigger", () => {
    const trigger = AgentTriggerBuilder.onCondition("disabled-trigger")
      .condition("false")
      .pause()
      .disable()
      .build();

    expect(trigger.enabled).toBe(false);
  });

  it("should fail without action", () => {
    expect(() => {
      AgentTriggerBuilder.create("no-action")
        .type("condition")
        .condition("iteration > 5")
        .build();
    }).toThrow("Trigger action is required");
  });

  it("should clone trigger builder", () => {
    const original = AgentTriggerBuilder.onCondition("test")
      .condition("x > 5")
      .pause();

    const cloned = original.clone();
    cloned.stop();

    const originalTrigger = original.build();
    const clonedTrigger = cloned.build();

    expect(originalTrigger.action.type).toBe("pause");
    expect(clonedTrigger.action.type).toBe("stop");
  });
});

describe("AgentDefinitionBuilder", () => {
  it("should build basic agent definition", () => {
    const definition = AgentDefinitionBuilder.create()
      .name("TestAgent")
      .build();

    expect(definition.name).toBe("TestAgent");
    expect(definition.version).toBe("1.0.0");
    expect(definition.id).toBeDefined();
  });

  it("should build agent with all properties", () => {
    const definition = AgentDefinitionBuilder.create()
      .id("agent-001")
      .name("DocumentAnalyzer")
      .version("2.0.0")
      .description("Analyzes documents")
      .profileId("gpt-4")
      .systemPrompt("You are a document analyzer")
      .maxIterations(20)
      .enableStreaming()
      .build();

    expect(definition.id).toBe("agent-001");
    expect(definition.name).toBe("DocumentAnalyzer");
    expect(definition.version).toBe("2.0.0");
    expect(definition.description).toBe("Analyzes documents");
    expect(definition.profileId).toBe("gpt-4");
    expect(definition.systemPrompt).toBe("You are a document analyzer");
    expect(definition.maxIterations).toBe(20);
    expect(definition.stream).toBe(true);
  });

  it("should build agent with tools", () => {
    const toolConfig = AgentToolConfigBuilder.create()
      .addTools(["search", "read"])
      .build();

    const definition = AgentDefinitionBuilder.create()
      .name("SearchAgent")
      .availableTools(toolConfig)
      .build();

    expect(definition.availableTools?.tools).toEqual(["search", "read"]);
  });

  it("should build agent with hooks", () => {
    const hook1 = AgentHookBuilder.beforeIteration()
      .eventName("start-iteration")
      .build();

    const hook2 = AgentHookBuilder.afterIteration()
      .eventName("end-iteration")
      .build();

    const definition = AgentDefinitionBuilder.create()
      .name("HookAgent")
      .addHooks([hook1, hook2])
      .build();

    expect(definition.hooks).toHaveLength(2);
    expect(definition.hooks?.[0].eventName).toBe("start-iteration");
  });

  it("should build agent with triggers", () => {
    const trigger = AgentTriggerBuilder.onCondition("max-iter")
      .condition("iteration > 50")
      .stop()
      .build();

    const definition = AgentDefinitionBuilder.create()
      .name("TriggerAgent")
      .addTrigger(trigger)
      .build();

    expect(definition.triggers).toHaveLength(1);
    expect(definition.triggers?.[0].id).toBe("max-iter");
  });

  it("should build agent with checkpoint config", () => {
    const definition = AgentDefinitionBuilder.create()
      .name("CheckpointAgent")
      .createCheckpointOnEnd()
      .createCheckpointOnError()
      .build();

    expect(definition.checkpoint?.createOnEnd).toBe(true);
    expect(definition.checkpoint?.createOnError).toBe(true);
  });

  it("should fail without name", () => {
    expect(() => {
      AgentDefinitionBuilder.create().build();
    }).toThrow("Agent name is required");
  });

  it("should add metadata", () => {
    const definition = AgentDefinitionBuilder.create()
      .name("MetaAgent")
      .metadata("author", "Alice")
      .metadata({ tags: ["test", "demo"] })
      .build();

    expect(definition.metadata?.author).toBe("Alice");
    expect(definition.metadata?.tags).toEqual(["test", "demo"]);
  });

  it("should clone builder", () => {
    const original = AgentDefinitionBuilder.create()
      .name("OriginalAgent")
      .version("1.0.0")
      .systemPrompt("Original prompt");

    const cloned = original.clone();
    cloned.name("ClonedAgent").version("2.0.0");

    const originalDef = original.build();
    const clonedDef = cloned.build();

    expect(originalDef.name).toBe("OriginalAgent");
    expect(originalDef.version).toBe("1.0.0");
    expect(clonedDef.name).toBe("ClonedAgent");
    expect(clonedDef.version).toBe("2.0.0");
  });
});
