/**
 * Tests for WorkflowCheckpointConfigResolver
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  WorkflowCheckpointConfigResolver,
  buildNodeCheckpointLayers,
  resolveCheckpointConfig,
  shouldCreateCheckpoint,
  getCheckpointDescription,
} from "../config-resolver.js";
import type {
  CheckpointConfigContext,
  WorkflowCheckpointConfigLayer,
  CheckpointTriggerType,
} from "@wf-agent/types";
import { CheckpointTrigger } from "@wf-agent/types";

// Mock contextual logger
vi.mock("../../../../utils/contextual-logger.js", () => ({
  createContextualLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

describe("WorkflowCheckpointConfigResolver", () => {
  let resolver: WorkflowCheckpointConfigResolver;

  beforeEach(() => {
    resolver = new WorkflowCheckpointConfigResolver();
  });

  describe("resolveWorkflowConfig", () => {
    it("should skip checkpoint creation for triggered subworkflow without explicit enable", () => {
      const context: CheckpointConfigContext = {
        triggerType: CheckpointTrigger.AFTER_EXECUTE,
        isTriggeredSubworkflow: true,
        explicitEnableCheckpoint: false,
      };

      const result = resolver.resolveWorkflowConfig([], context);

      expect(result.shouldCreate).toBe(false);
      expect(result.effectiveSource).toBe("default");
    });

    it("should create checkpoint for triggered subworkflow with explicit enable", () => {
      const context: CheckpointConfigContext = {
        triggerType: CheckpointTrigger.AFTER_EXECUTE,
        isTriggeredSubworkflow: true,
        explicitEnableCheckpoint: true,
      };

      const layers: WorkflowCheckpointConfigLayer[] = [
        {
          source: "node",
          config: { enabled: true },
        },
      ];

      const result = resolver.resolveWorkflowConfig(layers, context);

      expect(result.shouldCreate).toBe(true);
      expect(result.effectiveSource).toBe("node");
    });

    it("should return shouldCreate false when enabled is false", () => {
      const context: CheckpointConfigContext = {
        triggerType: CheckpointTrigger.AFTER_EXECUTE,
      };

      const layers: WorkflowCheckpointConfigLayer[] = [
        {
          source: "runtime",
          config: { enabled: false },
        },
      ];

      const result = resolver.resolveWorkflowConfig(layers, context);

      expect(result.shouldCreate).toBe(false);
      expect(result.effectiveSource).toBe("runtime");
    });

    it("should respect trigger-specific config for NODE_BEFORE_EXECUTE", () => {
      const context: CheckpointConfigContext = {
        triggerType: CheckpointTrigger.BEFORE_EXECUTE,
      };

      const layers: WorkflowCheckpointConfigLayer[] = [
        {
          source: "workflow",
          config: {
            enabled: true,
            triggers: { nodeBeforeExecute: false },
          },
        },
      ];

      const result = resolver.resolveWorkflowConfig(layers, context);

      expect(result.shouldCreate).toBe(false);
    });

    it("should respect trigger-specific config for NODE_AFTER_EXECUTE", () => {
      const context: CheckpointConfigContext = {
        triggerType: CheckpointTrigger.AFTER_EXECUTE,
      };

      const layers: WorkflowCheckpointConfigLayer[] = [
        {
          source: "workflow",
          config: {
            enabled: true,
            triggers: { nodeAfterExecute: true },
          },
        },
      ];

      const result = resolver.resolveWorkflowConfig(layers, context);

      expect(result.shouldCreate).toBe(true);
    });

    it("should respect trigger-specific config for TOOL_BEFORE", () => {
      const context: CheckpointConfigContext = {
        triggerType: "TOOL_BEFORE",
      };

      const layers: WorkflowCheckpointConfigLayer[] = [
        {
          source: "workflow",
          config: {
            enabled: true,
            triggers: { toolBefore: true },
          },
        },
      ];

      const result = resolver.resolveWorkflowConfig(layers, context);

      expect(result.shouldCreate).toBe(true);
    });

    it("should respect trigger-specific config for TOOL_AFTER", () => {
      const context: CheckpointConfigContext = {
        triggerType: "TOOL_AFTER",
      };

      const layers: WorkflowCheckpointConfigLayer[] = [
        {
          source: "workflow",
          config: {
            enabled: true,
            triggers: { toolAfter: false },
          },
        },
      ];

      const result = resolver.resolveWorkflowConfig(layers, context);

      expect(result.shouldCreate).toBe(false);
    });

    it("should always create checkpoint for HOOK and TRIGGER trigger types", () => {
      const hookContext: CheckpointConfigContext = { triggerType: "HOOK" };
      const triggerContext: CheckpointConfigContext = { triggerType: "TRIGGER" };

      const hookResult = resolver.resolveWorkflowConfig([], hookContext);
      const triggerResult = resolver.resolveWorkflowConfig([], triggerContext);

      expect(hookResult.shouldCreate).toBe(true);
      expect(triggerResult.shouldCreate).toBe(true);
    });

    it("should use first layer with explicit enabled as effective source", () => {
      const context: CheckpointConfigContext = {
        triggerType: CheckpointTrigger.AFTER_EXECUTE,
      };

      const layers: WorkflowCheckpointConfigLayer[] = [
        {
          source: "node",
          config: { description: "Node config without enabled" },
        },
        {
          source: "global",
          config: { enabled: true },
        },
      ];

      const result = resolver.resolveWorkflowConfig(layers, context);

      expect(result.shouldCreate).toBe(true);
      expect(result.effectiveSource).toBe("global");
    });

    it("should use trigger default when no layer has explicit enabled", () => {
      const context: CheckpointConfigContext = {
        triggerType: CheckpointTrigger.AFTER_EXECUTE,
      };

      const layers: WorkflowCheckpointConfigLayer[] = [
        {
          source: "workflow",
          config: { description: "Just a description" },
        },
      ];

      const result = resolver.resolveWorkflowConfig(layers, context);

      // When enabled is not explicitly set, trigger defaults apply
      // For NODE_AFTER_EXECUTE, the default is to create a checkpoint
      expect(result.shouldCreate).toBe(true);
      expect(result.effectiveSource).toBe("default");
    });

    it("should merge triggers from multiple layers", () => {
      const context: CheckpointConfigContext = {
        triggerType: CheckpointTrigger.AFTER_EXECUTE,
      };

      const layers: WorkflowCheckpointConfigLayer[] = [
        {
          source: "node",
          config: {
            enabled: true,
            triggers: { nodeAfterExecute: true },
          },
        },
        {
          source: "global",
          config: {
            triggers: { nodeAfterExecute: false },
          },
        },
      ];

      const result = resolver.resolveWorkflowConfig(layers, context);

      // Higher priority layer wins
      expect(result.shouldCreate).toBe(true);
      expect(result.effectiveSource).toBe("node");
    });

    it("should build description from context when not provided in config", () => {
      const contexts: Array<{ triggerType: GraphCheckpointTriggerType; expectedPrefix: string }> = [
        { triggerType: CheckpointTrigger.BEFORE_EXECUTE, expectedPrefix: "Before node" },
        { triggerType: CheckpointTrigger.AFTER_EXECUTE, expectedPrefix: "After node" },
        { triggerType: "TOOL_BEFORE", expectedPrefix: "Before tool" },
        { triggerType: "TOOL_AFTER", expectedPrefix: "After tool" },
        { triggerType: "HOOK", expectedPrefix: "Hook" },
        { triggerType: "TRIGGER", expectedPrefix: "Trigger" },
      ];

      for (const { triggerType, expectedPrefix } of contexts) {
        const context: CheckpointConfigContext = { triggerType };
        const result = resolver.resolveWorkflowConfig([], context);
        expect(result.description).toBe(`${expectedPrefix} checkpoint`);
      }
    });

    it("should use config description when provided", () => {
      const context: CheckpointConfigContext = {
        triggerType: CheckpointTrigger.AFTER_EXECUTE,
      };

      const layers: WorkflowCheckpointConfigLayer[] = [
        {
          source: "node",
          config: { enabled: true, description: "Custom description" },
        },
      ];

      const result = resolver.resolveWorkflowConfig(layers, context);

      expect(result.description).toBe("Custom description");
    });
  });
});

describe("buildNodeCheckpointLayers", () => {
  it("should return empty layers when no global config and no node", () => {
    const context: CheckpointConfigContext = {
      triggerType: CheckpointTrigger.BEFORE_EXECUTE,
    };

    const layers = buildNodeCheckpointLayers(undefined, undefined, context);

    expect(layers).toEqual([]);
  });

  it("should build layer from node config with higher priority", () => {
    const context: CheckpointConfigContext = {
      triggerType: CheckpointTrigger.BEFORE_EXECUTE,
    };

    const layers = buildNodeCheckpointLayers(
      undefined,
      {
        id: "node-1",
        name: "Test Node",
        type: "LLM",
        checkpointBeforeExecute: true,
      } as any,
      context,
    );

    expect(layers).toHaveLength(1);
    expect(layers[0]!.source).toBe("node");
    expect(layers[0]!.config.enabled).toBe(true);
    expect(layers[0]!.config.description).toBe("Before node: Test Node");
  });

  it("should build layer from node config for NODE_AFTER_EXECUTE", () => {
    const context: CheckpointConfigContext = {
      triggerType: CheckpointTrigger.AFTER_EXECUTE,
    };

    const layers = buildNodeCheckpointLayers(
      undefined,
      {
        id: "node-2",
        name: "After Node",
        type: "SCRIPT",
        checkpointAfterExecute: true,
      } as any,
      context,
    );

    expect(layers).toHaveLength(1);
    expect(layers[0]!.source).toBe("node");
    expect(layers[0]!.config.description).toBe("After node: After Node");
  });

  it("should build layers from both node and global config", () => {
    const context: CheckpointConfigContext = {
      triggerType: CheckpointTrigger.BEFORE_EXECUTE,
    };

    const layers = buildNodeCheckpointLayers(
      { checkpointBeforeNode: false },
      {
        id: "node-3",
        name: "Priority Node",
        type: "LLM",
        checkpointBeforeExecute: true,
      } as any,
      context,
    );

    expect(layers).toHaveLength(2);
    expect(layers[0]!.source).toBe("node");
    expect(layers[0]!.config.enabled).toBe(true);
    expect(layers[1]!.source).toBe("global");
    expect(layers[1]!.config.enabled).toBe(false);
  });

  it("should skip node layer when node checkpoint config is undefined", () => {
    const context: CheckpointConfigContext = {
      triggerType: CheckpointTrigger.BEFORE_EXECUTE,
    };

    const layers = buildNodeCheckpointLayers(
      { checkpointBeforeNode: true },
      {
        id: "node-4",
        name: "No Config",
        type: "LLM",
      } as any,
      context,
    );

    expect(layers).toHaveLength(1);
    expect(layers[0]!.source).toBe("global");
    expect(layers[0]!.config.enabled).toBe(true);
  });

  it("should build global layer for NODE_AFTER_EXECUTE", () => {
    const context: CheckpointConfigContext = {
      triggerType: CheckpointTrigger.AFTER_EXECUTE,
    };

    const layers = buildNodeCheckpointLayers({ checkpointAfterNode: true }, undefined, context);

    expect(layers).toHaveLength(1);
    expect(layers[0]!.source).toBe("global");
    expect(layers[0]!.config.enabled).toBe(true);
    expect(layers[0]!.config.description).toBe("Global checkpoint after node");
  });
});

describe("resolveCheckpointConfig", () => {
  it("should use default resolver to resolve config", () => {
    const context: CheckpointConfigContext = {
      triggerType: CheckpointTrigger.AFTER_EXECUTE,
    };

    const layers: GraphCheckpointConfigLayer[] = [
      {
        source: "node",
        config: { enabled: true },
      },
    ];

    const result = resolveCheckpointConfig(layers, context);

    expect(result.shouldCreate).toBe(true);
    expect(result.effectiveSource).toBe("node");
  });
});

describe("shouldCreateCheckpoint", () => {
  it("should return true when config allows", () => {
    const context: CheckpointConfigContext = {
      triggerType: CheckpointTrigger.AFTER_EXECUTE,
    };

    const layers: GraphCheckpointConfigLayer[] = [
      {
        source: "node",
        config: { enabled: true },
      },
    ];

    expect(shouldCreateCheckpoint(layers, context)).toBe(true);
  });

  it("should return false when config disallows", () => {
    const context: CheckpointConfigContext = {
      triggerType: CheckpointTrigger.AFTER_EXECUTE,
    };

    const layers: GraphCheckpointConfigLayer[] = [
      {
        source: "runtime",
        config: { enabled: false },
      },
    ];

    expect(shouldCreateCheckpoint(layers, context)).toBe(false);
  });
});

describe("getCheckpointDescription", () => {
  it("should return description from config", () => {
    const context: CheckpointConfigContext = {
      triggerType: CheckpointTrigger.AFTER_EXECUTE,
    };

    const layers: GraphCheckpointConfigLayer[] = [
      {
        source: "node",
        config: { enabled: true, description: "My checkpoint" },
      },
    ];

    expect(getCheckpointDescription(layers, context)).toBe("My checkpoint");
  });

  it("should return default description for empty config based on trigger type", () => {
    const context: CheckpointConfigContext = {
      triggerType: CheckpointTrigger.AFTER_EXECUTE,
    };

    expect(getCheckpointDescription([], context)).toBe("After node checkpoint");
  });

  it("should generate correct default description for NODE_BEFORE_EXECUTE", () => {
    const context: CheckpointConfigContext = {
      triggerType: CheckpointTrigger.BEFORE_EXECUTE,
    };

    expect(getCheckpointDescription([], context)).toBe("Before node checkpoint");
  });

  it("should generate correct default description for HOOK", () => {
    const context: CheckpointConfigContext = {
      triggerType: "HOOK",
    };

    expect(getCheckpointDescription([], context)).toBe("Hook checkpoint");
  });
});
