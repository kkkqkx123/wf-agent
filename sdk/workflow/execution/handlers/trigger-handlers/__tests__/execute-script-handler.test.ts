import { describe, it, expect, vi, beforeEach } from "vitest";
import type { TriggerAction } from "@wf-agent/types";
import { executeScriptHandler } from "../execute-script-handler.js";
import type { GlobalContext } from "../../../../../core/global-context.js";
import type { ScriptRegistry } from "../../../../../core/registry/script-registry.js";
import type { Container } from "@wf-agent/common-utils";

const mockScriptRegistry = {
  execute: vi.fn(),
} as unknown as ScriptRegistry;

const mockContainer = {
  get: vi.fn(),
} as unknown as Container;

const mockGlobalContext = {
  container: mockContainer,
} as unknown as GlobalContext;

describe("execute-script-handler", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (mockContainer.get as any).mockReturnValue(mockScriptRegistry);
  });

  it("should execute script successfully", async () => {
    (mockScriptRegistry.execute as any).mockResolvedValue({ result: "script output" });
    const action: TriggerAction = {
      type: "execute_script",
      parameters: { scriptName: "my-script", parameters: { key: "value" } },
    };

    const result = await executeScriptHandler(action, "trigger-1", mockGlobalContext);

    expect(mockScriptRegistry.execute).toHaveBeenCalledWith("my-script", {
      key: "value",
    });
    expect(result.success).toBe(true);
    expect(result.result).toEqual({
      message: "Script my-script executed successfully",
      result: { result: "script output" },
    });
  });

  it("should execute script with timeout", async () => {
    (mockScriptRegistry.execute as any).mockResolvedValue({ result: "done" });
    const action: TriggerAction = {
      type: "execute_script",
      parameters: { scriptName: "timeout-script", timeout: 5000 },
    };

    const result = await executeScriptHandler(action, "trigger-2", mockGlobalContext);

    expect(mockScriptRegistry.execute).toHaveBeenCalledWith("timeout-script", {
      timeout: 5000,
    });
    expect(result.success).toBe(true);
  });

  it("should ignore error when ignoreError is true", async () => {
    (mockScriptRegistry.execute as any).mockRejectedValue(new Error("Script failed"));
    const action: TriggerAction = {
      type: "execute_script",
      parameters: { scriptName: "failing-script", ignoreError: true },
    };

    const result = await executeScriptHandler(action, "trigger-3", mockGlobalContext);

    expect(result.success).toBe(true);
    expect(result.result).toEqual({
      warning: expect.stringContaining("Script execution failed but ignored"),
    });
  });

  it("should fail when scriptName is missing", async () => {
    const action = {
      type: "execute_script",
      parameters: {},
    } as unknown as TriggerAction;

    const result = await executeScriptHandler(action, "trigger-4", mockGlobalContext);

    expect(result.success).toBe(false);
    expect(result.error).toContain("scriptName is required");
  });

  it("should fail when globalContext is missing", async () => {
    const action: TriggerAction = {
      type: "execute_script",
      parameters: { scriptName: "my-script" },
    };

    const result = await executeScriptHandler(action, "trigger-5", undefined);

    expect(result.success).toBe(false);
    expect(result.error).toContain("GlobalContext is required");
  });

  it("should fail when script registry not available", async () => {
    (mockContainer.get as any).mockReturnValue(undefined);
    const action: TriggerAction = {
      type: "execute_script",
      parameters: { scriptName: "missing-registry-script" },
    };

    const result = await executeScriptHandler(action, "trigger-6", mockGlobalContext);

    expect(result.success).toBe(false);
    expect(result.error).toContain("not available");
  });

  it("should fail when script execution throws error", async () => {
    (mockScriptRegistry.execute as any).mockRejectedValue(new Error("Execution error"));
    const action: TriggerAction = {
      type: "execute_script",
      parameters: { scriptName: "error-script" },
    };

    const result = await executeScriptHandler(action, "trigger-7", mockGlobalContext);

    expect(result.success).toBe(false);
    expect(result.error).toBe("Execution error");
  });
});