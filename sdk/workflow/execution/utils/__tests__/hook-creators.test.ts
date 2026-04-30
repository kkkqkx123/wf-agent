/**
 * Unit Tests for HookCreators
 * Testing the utility functions of the Hook Creator tool
 */

import { describe, it, expect, vi } from "vitest";
import {
  createWorkflowExecutionStateCheckHook,
  createCustomValidationHook,
  createPermissionCheckHook,
  createAuditLoggingHook,
} from "../hook-creators.js";
import { HookType } from "@wf-agent/types";
import { ExecutionError } from "@wf-agent/types";
import type { HookExecutionContext } from "../../handlers/hook-handlers/hook-handler.js";
import type { WorkflowExecution } from "@wf-agent/types";

/**
 * Create a simulated HookExecutionContext
 */
function createMockExecutionContext(
  overrides?: Partial<HookExecutionContext>,
): HookExecutionContext {
  const mockWorkflowExecution: WorkflowExecution = {
    id: "test-workflow-execution",
    workflowId: "test-workflow",
    workflowVersion: "1.0.0.0",
    currentNodeId: "test-node",
    input: {},
    output: {},
    nodeResults: [],
    errors: [],
    graph: {} as any,
    variables: [],
    executionType: "MAIN",
    variableScopes: {
      global: {},
      workflowExecution: {},
      local: [],
      loop: [],
    },
  };

  // Mock entity with getStatus method
  const mockEntity = {
    getExecution: () => mockWorkflowExecution,
    getStatus: () => "RUNNING",
    id: "test-workflow-execution",
    getWorkflowId: () => "test-workflow",
  } as any;

  return {
    workflowExecutionEntity: mockEntity,
    node: {
      id: "test-node",
      type: "SCRIPT",
      name: "Test Node",
      config: {},
    },
    workflowExecutionId: "test-execution",
    ...overrides,
  } as HookExecutionContext;
}

describe("createWorkflowExecutionStateCheckHook", () => {
  it("The correct Hook configuration should be created", () => {
    const hook = createWorkflowExecutionStateCheckHook(["RUNNING"]);

    expect(hook.hookType).toBe("BEFORE_EXECUTE");
    expect(hook.eventName).toBe("validation.workflow_execution_status_check");
    expect(hook.weight).toBe(200);
    expect(hook.eventPayload).toBeDefined();
    expect(hook.eventPayload!["allowedStates"]).toEqual(["RUNNING"]);
    expect(hook.eventPayload!["handler"]).toBeInstanceOf(Function);
  });

  it("No error is thrown when the workflow execution state is in the allowed list", async () => {
    const hook = createWorkflowExecutionStateCheckHook(["RUNNING", "PAUSED"]);
    const baseContext = createMockExecutionContext();
    const mockEntity = {
      ...baseContext.workflowExecutionEntity,
      getStatus: () => "RUNNING",
    } as any;
    const context: HookExecutionContext = { ...baseContext, workflowExecutionEntity: mockEntity };

    // No errors should be thrown.
    await expect((hook.eventPayload!["handler"] as Function)(context)).resolves.toBeUndefined();
  });

  it("Throws ExecutionError when the workflow execution state is not in the allowed list.", async () => {
    const hook = createWorkflowExecutionStateCheckHook(["RUNNING"]);
    const baseContext = createMockExecutionContext();
    const mockEntity = {
      ...baseContext.workflowExecutionEntity,
      getStatus: () => "COMPLETED",
    } as any;
    const context: HookExecutionContext = { ...baseContext, workflowExecutionEntity: mockEntity };

    await expect((hook.eventPayload!["handler"] as Function)(context)).rejects.toThrow(ExecutionError);

    await expect((hook.eventPayload!["handler"] as Function)(context)).rejects.toThrow(
      "Workflow execution is in COMPLETED state, expected: RUNNING",
    );
  });

  it("Correctly check the state when there is more than one allowed state", async () => {
    const hook = createWorkflowExecutionStateCheckHook(["RUNNING", "PAUSED", "CREATED"]);

    // Testing the RUNNING state
    const baseContext1 = createMockExecutionContext();
    const mockEntity1 = { ...baseContext1.workflowExecutionEntity, getStatus: () => "RUNNING" } as any;
    const context1: HookExecutionContext = { ...baseContext1, workflowExecutionEntity: mockEntity1 };
    await expect((hook.eventPayload!["handler"] as Function)(context1)).resolves.toBeUndefined();

    // Testing the PAUSED state
    const baseContext2 = createMockExecutionContext();
    const mockEntity2 = { ...baseContext2.workflowExecutionEntity, getStatus: () => "PAUSED" } as any;
    const context2: HookExecutionContext = { ...baseContext2, workflowExecutionEntity: mockEntity2 };
    await expect((hook.eventPayload!["handler"] as Function)(context2)).resolves.toBeUndefined();

    // Test the CREATED status.
    const baseContext3 = createMockExecutionContext();
    const mockEntity3 = { ...baseContext3.workflowExecutionEntity, getStatus: () => "CREATED" } as any;
    const context3: HookExecutionContext = { ...baseContext3, workflowExecutionEntity: mockEntity3 };
    await expect((hook.eventPayload!["handler"] as Function)(context3)).resolves.toBeUndefined();

    // Testing states that are not allowed.
    const baseContext4 = createMockExecutionContext();
    const mockEntity4 = { ...baseContext4.workflowExecutionEntity, getStatus: () => "FAILED" } as any;
    const context4: HookExecutionContext = { ...baseContext4, workflowExecutionEntity: mockEntity4 };
    await expect((hook.eventPayload!["handler"] as Function)(context4)).rejects.toThrow();
  });

  it("Using the default list of allowed states", async () => {
    const hook = createWorkflowExecutionStateCheckHook(); // Default ['RUNNING']
    const context = createMockExecutionContext();

    await expect((hook.eventPayload!["handler"] as Function)(context)).resolves.toBeUndefined();
  });
});

describe("createCustomValidationHook", () => {
  it("The correct Hook configuration should be created", () => {
    const validator = vi.fn();
    const hook = createCustomValidationHook(validator);

    expect(hook.hookType).toBe("BEFORE_EXECUTE");
    expect(hook.eventName).toBe("validation.custom_check");
    expect(hook.weight).toBe(150);
    expect(hook.eventPayload).toBeDefined();
    expect(hook.eventPayload!["handler"]).toBe(validator);
  });

  it("Support for customizing event names", () => {
    const validator = vi.fn();
    const hook = createCustomValidationHook(validator, "custom.event");

    expect(hook.eventName).toBe("custom.event");
    expect(hook.eventPayload).toBeDefined();
  });

  it("Support for customized weights", () => {
    const validator = vi.fn();
    const hook = createCustomValidationHook(validator, "custom.event", 100);

    expect(hook.weight).toBe(100);
    expect(hook.eventPayload).toBeDefined();
  });

  it("No error is thrown when validating the resolve function", async () => {
    const validator = vi.fn().mockResolvedValue(undefined);
    const hook = createCustomValidationHook(validator);
    const context = createMockExecutionContext();

    await expect((hook.eventPayload!["handler"] as Function)(context)).resolves.toBeUndefined();
    expect(validator).toHaveBeenCalledWith(context);
  });

  it("Throw an error when the validation function rejects", async () => {
    const validator = vi.fn().mockRejectedValue(new Error("Validation failed"));
    const hook = createCustomValidationHook(validator);
    const context = createMockExecutionContext();

    await expect((hook.eventPayload!["handler"] as Function)(context)).rejects.toThrow("Validation failed");
  });

  it("Support for synchronized validation functions", async () => {
    const validator = vi.fn(); // Synchronous function, returns undefined
    const hook = createCustomValidationHook(validator);
    const context = createMockExecutionContext();

    // Synchronous functions are called directly and do not return a Promise.
    const result = (hook.eventPayload!["handler"] as Function)(context);
    expect(result).toBeUndefined();
    expect(validator).toHaveBeenCalledWith(context);
  });

  it("Verify that the function has access to context information", async () => {
    const validator = vi.fn().mockImplementation((ctx: HookExecutionContext) => {
      // The validation function should have access to the context.
      const execution = ctx.workflowExecutionEntity.getExecution();
      expect(execution.id).toBe("test-workflow-execution");
      expect(ctx.node.id).toBe("test-node");
    });

    const hook = createCustomValidationHook(validator);
    const context = createMockExecutionContext();

    await (hook.eventPayload!["handler"] as Function)(context);
    expect(validator).toHaveBeenCalled();
  });
});

describe("createPermissionCheckHook", () => {
  it("The correct Hook configuration should be created", () => {
    const hook = createPermissionCheckHook(["read", "write"]);

    expect(hook.hookType).toBe("BEFORE_EXECUTE");
    expect(hook.eventName).toBe("business.permission_check");
    expect(hook.weight).toBe(100);
    expect(hook.eventPayload).toBeDefined();
    expect(hook.eventPayload!["requiredPermissions"]).toEqual(["read", "write"]);
    expect(hook.eventPayload!["handler"]).toBeInstanceOf(Function);
  });

  it("Do not throw an error when the user has the required privileges", async () => {
    const hook = createPermissionCheckHook(["read", "write"]);
    const baseContext = createMockExecutionContext();
    const mockWorkflowExecution = baseContext.workflowExecutionEntity.getExecution();
    const context = createMockExecutionContext({
      workflowExecutionEntity: {
        ...baseContext.workflowExecutionEntity,
        getExecution: () => ({
          ...mockWorkflowExecution,
          variableScopes: {
            ...mockWorkflowExecution.variableScopes,
            workflowExecution: { permissions: ["read", "write", "delete"] },
          },
        }),
      } as any,
    });

    await expect((hook.eventPayload!["handler"] as Function)(context)).resolves.toBeUndefined();
  });

  it("Throws ExecutionError when the user lacks privileges.", async () => {
    const hook = createPermissionCheckHook(["read", "write", "admin"]);
    const baseContext = createMockExecutionContext();
    const mockWorkflowExecution = baseContext.workflowExecutionEntity.getExecution();
    const context = createMockExecutionContext({
      workflowExecutionEntity: {
        ...baseContext.workflowExecutionEntity,
        getExecution: () => ({
          ...mockWorkflowExecution,
          variableScopes: {
            ...mockWorkflowExecution.variableScopes,
            workflowExecution: { permissions: ["read"] },
          },
        }),
      } as any,
    });

    await expect((hook.eventPayload!["handler"] as Function)(context)).rejects.toThrow(ExecutionError);

    await expect((hook.eventPayload!["handler"] as Function)(context)).rejects.toThrow(
      "Missing permissions: write, admin",
    );
  });

  it("Throw an error with all required permissions when the user does not have any permissions", async () => {
    const hook = createPermissionCheckHook(["read", "write"]);
    const baseContext = createMockExecutionContext();
    const mockWorkflowExecution = baseContext.workflowExecutionEntity.getExecution();
    const context = createMockExecutionContext({
      workflowExecutionEntity: {
        ...baseContext.workflowExecutionEntity,
        getExecution: () => ({
          ...mockWorkflowExecution,
          variableScopes: {
            ...mockWorkflowExecution.variableScopes,
            workflowExecution: { permissions: [] },
          },
        }),
      } as any,
    });

    await expect((hook.eventPayload!["handler"] as Function)(context)).rejects.toThrow(
      "Missing permissions: read, write",
    );
  });

  it("When a workflow execution scope has no permissions, it is considered to have no permissions at all", async () => {
    const hook = createPermissionCheckHook(["read"]);
    const baseContext = createMockExecutionContext();
    const mockWorkflowExecution = baseContext.workflowExecutionEntity.getExecution();
    const context = createMockExecutionContext({
      workflowExecutionEntity: {
        ...baseContext.workflowExecutionEntity,
        getExecution: () => ({
          ...mockWorkflowExecution,
          variableScopes: {
            ...mockWorkflowExecution.variableScopes,
            workflowExecution: {},
          },
        }),
      } as any,
    });

    await expect((hook.eventPayload!["handler"] as Function)(context)).rejects.toThrow(
      "Missing permissions: read",
    );
  });
});

describe("createAuditLoggingHook", () => {
  it("The correct Hook configuration should be created", () => {
    const auditService = { log: vi.fn() };
    const hook = createAuditLoggingHook(auditService);

    expect(hook.hookType).toBe("BEFORE_EXECUTE");
    expect(hook.eventName).toBe("monitoring.execution_audit");
    expect(hook.weight).toBe(50);
    expect(hook.eventPayload).toBeDefined();
    expect(hook.eventPayload!["handler"]).toBeInstanceOf(Function);
  });

  it("Calling the audit service to log", async () => {
    const mockLog = vi.fn().mockResolvedValue(undefined);
    const auditService = { log: mockLog };
    const hook = createAuditLoggingHook(auditService);

    const baseContext = createMockExecutionContext();
    const mockWorkflowExecution = baseContext.workflowExecutionEntity.getExecution();
    const context = createMockExecutionContext({
      workflowExecutionEntity: {
        ...baseContext.workflowExecutionEntity,
        getExecution: () => ({
          ...mockWorkflowExecution,
          variableScopes: {
            ...mockWorkflowExecution.variableScopes,
            workflowExecution: { userId: "user-123" },
          },
        }),
      } as any,
      node: {
        ...baseContext.node,
        type: "SCRIPT",
        config: {
          scriptName: "test-script.js",
          scriptType: "javascript",
          risk: "high",
        },
      },
    }) as any;

    await (hook.eventPayload!["handler"] as Function)(context);

    expect(mockLog).toHaveBeenCalledTimes(1);
    expect(mockLog).toHaveBeenCalledWith(
      expect.objectContaining({
        eventType: "NODE_EXECUTION_ATTEMPT",
        executionId: "test-workflow-execution",
        nodeId: "test-node",
        nodeName: "Test Node",
        nodeType: "SCRIPT",
        userId: "user-123",
        scriptName: "test-script.js",
        riskLevel: "high",
      }),
    );
  });

  it("Recorded events contain timestamps", async () => {
    const mockLog = vi.fn().mockResolvedValue(undefined);
    const auditService = { log: mockLog };
    const hook = createAuditLoggingHook(auditService);
    const context = createMockExecutionContext();

    if (hook.eventPayload) {
      await (hook.eventPayload["handler"] as Function)(context);

      const loggedEvent = mockLog.mock.calls[0]![0];
      expect(loggedEvent.timestamp).toBeInstanceOf(Date);
    }
  });

  it("When the audit service throws an error, propagate the error", async () => {
    const mockLog = vi.fn().mockRejectedValue(new Error("Audit service error"));
    const auditService = { log: mockLog };
    const hook = createAuditLoggingHook(auditService);
    const context = createMockExecutionContext();

    await expect((hook.eventPayload!["handler"] as Function)(context)).rejects.toThrow("Audit service error");
  });
});
