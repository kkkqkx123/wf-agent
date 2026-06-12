import { describe, it, expect, vi, beforeEach } from "vitest";
import type { TriggerAction } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import { executeTriggeredSubworkflowHandler } from "../execute-triggered-subworkflow-handler.js";
import type { WorkflowExecutionRegistry } from "../../../../stores/workflow-execution-registry.js";
import type { GlobalContext } from "../../../../../core/global-context.js";
import type { WorkflowGraphRegistry } from "../../../../stores/workflow-graph-registry.js";
import type { TriggeredSubworkflowHandler } from "../../triggered-subworkflow-handler.js";
import type { AgentLoopEntity } from "../../../../../agent/entities/agent-loop-entity.js";
import type { Container } from "@wf-agent/common-utils";
import * as Identifiers from "../../../../../core/di/service-identifiers.js";

const mockMainEntity = {
  id: "main-exec-1",
} as unknown as WorkflowExecutionEntity;

const mockRegistry = {
  get: vi.fn(),
} as unknown as WorkflowExecutionRegistry;

const mockGraphRegistry = {
  get: vi.fn(),
} as unknown as WorkflowGraphRegistry;

const mockTriggeredSubworkflowHandler = {
  executeTriggeredSubgraph: vi.fn(),
} as unknown as TriggeredSubworkflowHandler;

const mockContainer = {
  get: vi.fn(),
} as unknown as Container;

const mockGlobalContext = {
  container: mockContainer,
} as unknown as GlobalContext;

const mockAgentLoopEntity = {
  id: "agent-loop-1",
} as unknown as AgentLoopEntity;

describe("execute-triggered-subworkflow-handler", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (mockRegistry.get as any).mockReturnValue(mockMainEntity);
    (mockGraphRegistry.get as any).mockReturnValue({ id: "sub-wf-1" });
    (mockContainer.get as any).mockImplementation((id: symbol) => {
      if (id === Identifiers.WorkflowGraphRegistry) return mockGraphRegistry;
      if (id === Identifiers.TriggeredSubworkflowHandler) return mockTriggeredSubworkflowHandler;
      return undefined;
    });
  });

  it("should execute triggered subworkflow synchronously and return success", async () => {
    (mockTriggeredSubworkflowHandler.executeTriggeredSubgraph as any).mockResolvedValue({
      subworkflowEntity: { getOutput: () => ({ result: "success" }) },
      executionTime: 100,
    });

    const action: TriggerAction = {
      type: "execute_triggered_subworkflow",
      parameters: { triggeredWorkflowId: "sub-wf-1", waitForCompletion: true },
    };

    const result = await executeTriggeredSubworkflowHandler(
      action,
      "trigger-1",
      mockRegistry,
      "main-exec-1",
      mockGlobalContext,
    );

    expect(mockTriggeredSubworkflowHandler.executeTriggeredSubgraph).toHaveBeenCalled();
    expect(result.success).toBe(true);
    expect(result.result).toMatchObject({
      message: "Triggered subworkflow execution completed: sub-wf-1",
      triggeredWorkflowId: "sub-wf-1",
      waitForCompletion: true,
      executed: true,
      completed: true,
      output: { result: "success" },
    });
  });

  it("should execute triggered subworkflow asynchronously and return task info", async () => {
    (mockTriggeredSubworkflowHandler.executeTriggeredSubgraph as any).mockResolvedValue({
      taskId: "task-1",
      status: "PENDING",
    });

    const action: TriggerAction = {
      type: "execute_triggered_subworkflow",
      parameters: { triggeredWorkflowId: "sub-wf-1", waitForCompletion: false },
    };

    const result = await executeTriggeredSubworkflowHandler(
      action,
      "trigger-2",
      mockRegistry,
      "main-exec-1",
      mockGlobalContext,
    );

    expect(result.success).toBe(true);
    expect(result.result).toMatchObject({
      message: "Triggered subworkflow submitted: sub-wf-1",
      triggeredWorkflowId: "sub-wf-1",
      taskId: "task-1",
      status: "PENDING",
      waitForCompletion: false,
      executed: true,
      completed: false,
    });
  });

  it("should execute with agentLoopEntity source", async () => {
    (mockTriggeredSubworkflowHandler.executeTriggeredSubgraph as any).mockResolvedValue({
      subworkflowEntity: { getOutput: () => ({}) },
      executionTime: 50,
    });

    const action: TriggerAction = {
      type: "execute_triggered_subworkflow",
      parameters: { triggeredWorkflowId: "sub-wf-1", waitForCompletion: true },
    };

    const result = await executeTriggeredSubworkflowHandler(
      action,
      "trigger-3",
      mockRegistry,
      "main-exec-1",
      mockGlobalContext,
      mockAgentLoopEntity,
    );

    expect(result.success).toBe(true);
    expect(mockTriggeredSubworkflowHandler.executeTriggeredSubgraph).toHaveBeenCalledWith(
      expect.objectContaining({ sourceType: "agent", sourceEntityId: "agent-loop-1" }),
    );
  });

  it("should use default waitForCompletion=true", async () => {
    (mockTriggeredSubworkflowHandler.executeTriggeredSubgraph as any).mockResolvedValue({
      subworkflowEntity: { getOutput: () => ({}) },
      executionTime: 50,
    });

    const action: TriggerAction = {
      type: "execute_triggered_subworkflow",
      parameters: { triggeredWorkflowId: "sub-wf-1" },
    };

    const result = await executeTriggeredSubworkflowHandler(
      action,
      "trigger-4",
      mockRegistry,
      "main-exec-1",
      mockGlobalContext,
    );

    expect(result.success).toBe(true);
    expect(result.result).toMatchObject({ waitForCompletion: true, completed: true });
  });

  it("should fail when triggeredWorkflowId is missing", async () => {
    const action = {
      type: "execute_triggered_subworkflow",
      parameters: {},
    } as unknown as TriggerAction;

    const result = await executeTriggeredSubworkflowHandler(
      action,
      "trigger-5",
      mockRegistry,
      "main-exec-1",
      mockGlobalContext,
    );

    expect(result.success).toBe(false);
    expect(result.error).toContain("triggeredWorkflowId");
  });

  it("should fail when currentExecutionId is missing", async () => {
    const action: TriggerAction = {
      type: "execute_triggered_subworkflow",
      parameters: { triggeredWorkflowId: "sub-wf-1" },
    };

    const result = await executeTriggeredSubworkflowHandler(
      action,
      "trigger-6",
      mockRegistry,
      undefined,
      mockGlobalContext,
    );

    expect(result.success).toBe(false);
    expect(result.error).toContain("execution ID not provided");
  });

  it("should fail when main workflow execution not found", async () => {
    (mockRegistry.get as any).mockReturnValue(null);
    const action: TriggerAction = {
      type: "execute_triggered_subworkflow",
      parameters: { triggeredWorkflowId: "sub-wf-1" },
    };

    const result = await executeTriggeredSubworkflowHandler(
      action,
      "trigger-7",
      mockRegistry,
      "main-exec-not-found",
      mockGlobalContext,
    );

    expect(result.success).toBe(false);
    expect(result.error).toContain("not found");
  });

  it("should fail when globalContext is missing", async () => {
    const action: TriggerAction = {
      type: "execute_triggered_subworkflow",
      parameters: { triggeredWorkflowId: "sub-wf-1" },
    };

    const result = await executeTriggeredSubworkflowHandler(
      action,
      "trigger-8",
      mockRegistry,
      "main-exec-1",
      undefined,
    );

    expect(result.success).toBe(false);
    expect(result.error).toContain("GlobalContext is required");
  });

  it("should fail when triggered workflow not found", async () => {
    (mockGraphRegistry.get as any).mockReturnValue(undefined);
    const action: TriggerAction = {
      type: "execute_triggered_subworkflow",
      parameters: { triggeredWorkflowId: "missing-wf" },
    };

    const result = await executeTriggeredSubworkflowHandler(
      action,
      "trigger-9",
      mockRegistry,
      "main-exec-1",
      mockGlobalContext,
    );

    expect(result.success).toBe(false);
    expect(result.error).toContain("not found");
  });
});
