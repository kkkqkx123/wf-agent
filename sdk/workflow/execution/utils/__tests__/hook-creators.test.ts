/**
 * Unit Tests for HookCreators
 * Testing the utility functions of the Hook Creator tool
 */

import { describe, it, expect, vi } from "vitest";
import {
  createThreadStateCheckHook,
  createCustomValidationHook,
  createPermissionCheckHook,
  createAuditLoggingHook,
} from "../hook-creators.js";
import { HookType } from "@wf-agent/types";
import { ExecutionError } from "@wf-agent/types";
import type { HookExecutionContext } from "../../handlers/hook-handlers/hook-handler.js";
import type { Thread } from "@wf-agent/types";

/**
 * Create a simulated HookExecutionContext
 */
function createMockExecutionContext(
  overrides?: Partial<HookExecutionContext>,
): HookExecutionContext {
  const mockThread: Thread = {
    id: "test-thread",
    workflowId: "test-workflow",
    workflowVersion: "1.0.0.0",
    status: "RUNNING",
    currentNodeId: "test-node",
    input: {},
    output: {},
    nodeResults: [],
    errors: [],
    startTime: Date.now(),
    graph: {} as any,
    variables: [],
    threadType: "MAIN",
    variableScopes: {
      global: {},
      thread: {},
      local: [],
      loop: [],
    },
  };

  return {
    thread: mockThread,
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

describe("createThreadStateCheckHook", () => {
  it("The correct Hook configuration should be created", () => {
    const hook = createThreadStateCheckHook(["RUNNING"]);

    expect(hook.hookType).toBe("BEFORE_EXECUTE");
    expect(hook.eventName).toBe("validation.thread_status_check");
    expect(hook.weight).toBe(200);
    expect(hook.eventPayload).toBeDefined();
    expect(hook.eventPayload!["allowedStates"]).toEqual(["RUNNING"]);
    expect(hook.eventPayload!["handler"]).toBeInstanceOf(Function);
  });

  it("No error is thrown when the thread state is in the allowed list", async () => {
    const hook = createThreadStateCheckHook(["RUNNING", "PAUSED"]);
    const context = createMockExecutionContext({
      thread: { ...createMockExecutionContext().thread, status: "RUNNING" },
    });

    // No errors should be thrown.
    await expect(hook.eventPayload!["handler"](context)).resolves.toBeUndefined();
  });

  it("Throws ExecutionError when the thread state is not in the allowed list.", async () => {
    const hook = createThreadStateCheckHook(["RUNNING"]);
    const context = createMockExecutionContext({
      thread: { ...createMockExecutionContext().thread, status: "COMPLETED" },
    });

    await expect(hook.eventPayload!["handler"](context)).rejects.toThrow(ExecutionError);

    await expect(hook.eventPayload!["handler"](context)).rejects.toThrow(
      "Thread is in COMPLETED state, expected: RUNNING",
    );
  });

  it("Correctly check the state when there is more than one allowed state", async () => {
    const hook = createThreadStateCheckHook(["RUNNING", "PAUSED", "CREATED"]);

    // Testing the RUNNING state
    const context1 = createMockExecutionContext({
      thread: { ...createMockExecutionContext().thread, status: "RUNNING" },
    });
    await expect(hook.eventPayload!["handler"](context1)).resolves.toBeUndefined();

    // Testing the PAUSED state
    const context2 = createMockExecutionContext({
      thread: { ...createMockExecutionContext().thread, status: "PAUSED" },
    });
    await expect(hook.eventPayload!["handler"](context2)).resolves.toBeUndefined();

    // Test the CREATED status.
    const context3 = createMockExecutionContext({
      thread: { ...createMockExecutionContext().thread, status: "CREATED" },
    });
    await expect(hook.eventPayload!["handler"](context3)).resolves.toBeUndefined();

    // Testing states that are not allowed.
    const context4 = createMockExecutionContext({
      thread: { ...createMockExecutionContext().thread, status: "FAILED" },
    });
    await expect(hook.eventPayload!["handler"](context4)).rejects.toThrow();
  });

  it("Using the default list of allowed states", async () => {
    const hook = createThreadStateCheckHook(); // Default ['RUNNING']
    const context = createMockExecutionContext({
      thread: { ...createMockExecutionContext().thread, status: "RUNNING" },
    });

    await expect(hook.eventPayload!["handler"](context)).resolves.toBeUndefined();
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

    await expect(hook.eventPayload!["handler"](context)).resolves.toBeUndefined();
    expect(validator).toHaveBeenCalledWith(context);
  });

  it("Throw an error when the validation function rejects", async () => {
    const validator = vi.fn().mockRejectedValue(new Error("Validation failed"));
    const hook = createCustomValidationHook(validator);
    const context = createMockExecutionContext();

    await expect(hook.eventPayload!["handler"](context)).rejects.toThrow("Validation failed");
  });

  it("Support for synchronized validation functions", async () => {
    const validator = vi.fn(); // Synchronous function, returns undefined
    const hook = createCustomValidationHook(validator);
    const context = createMockExecutionContext();

    // Synchronous functions are called directly and do not return a Promise.
    const result = hook.eventPayload!["handler"](context);
    expect(result).toBeUndefined();
    expect(validator).toHaveBeenCalledWith(context);
  });

  it("Verify that the function has access to context information", async () => {
    const validator = vi.fn().mockImplementation((ctx: HookExecutionContext) => {
      // The validation function should have access to the context.
      expect(ctx.thread.id).toBe("test-thread");
      expect(ctx.node.id).toBe("test-node");
    });

    const hook = createCustomValidationHook(validator);
    const context = createMockExecutionContext();

    await hook.eventPayload!["handler"](context);
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
    const context = createMockExecutionContext({
      thread: {
        ...baseContext.thread,
        variableScopes: {
          ...baseContext.thread.variableScopes,
          thread: { permissions: ["read", "write", "delete"] },
        },
      },
    });

    await expect(hook.eventPayload!["handler"](context)).resolves.toBeUndefined();
  });

  it("Throws ExecutionError when the user lacks privileges.", async () => {
    const hook = createPermissionCheckHook(["read", "write", "admin"]);
    const baseContext = createMockExecutionContext();
    const context = createMockExecutionContext({
      thread: {
        ...baseContext.thread,
        variableScopes: {
          ...baseContext.thread.variableScopes,
          thread: { permissions: ["read"] },
        },
      },
    });

    await expect(hook.eventPayload!["handler"](context)).rejects.toThrow(ExecutionError);

    await expect(hook.eventPayload!["handler"](context)).rejects.toThrow(
      "Missing permissions: write, admin",
    );
  });

  it("Throw an error with all required permissions when the user does not have any permissions", async () => {
    const hook = createPermissionCheckHook(["read", "write"]);
    const baseContext = createMockExecutionContext();
    const context = createMockExecutionContext({
      thread: {
        ...baseContext.thread,
        variableScopes: {
          ...baseContext.thread.variableScopes,
          thread: { permissions: [] },
        },
      },
    });

    await expect(hook.eventPayload!["handler"](context)).rejects.toThrow(
      "Missing permissions: read, write",
    );
  });

  it("When a thread scope has no permissions, it is considered to have no permissions at all", async () => {
    const hook = createPermissionCheckHook(["read"]);
    const baseContext = createMockExecutionContext();
    const context = createMockExecutionContext({
      thread: {
        ...baseContext.thread,
        variableScopes: {
          ...baseContext.thread.variableScopes,
          thread: {},
        },
      },
    });

    await expect(hook.eventPayload!["handler"](context)).rejects.toThrow(
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
    const context = createMockExecutionContext({
      thread: {
        ...baseContext.thread,
        variableScopes: {
          ...baseContext.thread.variableScopes,
          thread: { userId: "user-123" },
        },
      },
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

    await hook.eventPayload!["handler"](context);

    expect(mockLog).toHaveBeenCalledTimes(1);
    expect(mockLog).toHaveBeenCalledWith(
      expect.objectContaining({
        eventType: "NODE_EXECUTION_ATTEMPT",
        threadId: "test-thread",
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
      await hook.eventPayload["handler"](context);

      const loggedEvent = mockLog.mock.calls[0]![0];
      expect(loggedEvent.timestamp).toBeInstanceOf(Date);
    }
  });

  it("When the audit service throws an error, propagate the error", async () => {
    const mockLog = vi.fn().mockRejectedValue(new Error("Audit service error"));
    const auditService = { log: mockLog };
    const hook = createAuditLoggingHook(auditService);
    const context = createMockExecutionContext();

    await expect(hook.eventPayload!["handler"](context)).rejects.toThrow("Audit service error");
  });
});
