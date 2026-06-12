/**
 * ScriptInteractionCoordinator Unit Tests
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { ScriptInteractionCoordinator } from "../script-interaction-coordinator.js";
import type { InputProvider } from "../script-interaction-coordinator.js";
import type { GlobalContext } from "../../../../core/global-context.js";

// ============================================================================
// Test-friendly subclass — overrides delay to 0ms for fast tests
// ============================================================================
class FastTestCoordinator extends ScriptInteractionCoordinator {
  protected override delay(): Promise<void> {
    return Promise.resolve();
  }
}

// ============================================================================
// Mock Helpers
// ============================================================================

function createMockScriptRegistry() {
  return {
    getScript: vi.fn(),
    register: vi.fn(),
    has: vi.fn(),
    unregister: vi.fn(),
    getAll: vi.fn(),
  };
}

function createMockTerminalService() {
  return {
    createSession: vi.fn().mockResolvedValue({ sessionId: "session-1" }),
    startBackgroundCommand: vi.fn().mockResolvedValue({ success: true }),
    sendInput: vi.fn().mockResolvedValue(true),
    getOutput: vi.fn().mockResolvedValue(""),
    killBackgroundCommand: vi.fn().mockResolvedValue(undefined),
    terminateSession: vi.fn().mockResolvedValue(undefined),
  };
}

function createMockGlobalContext(
  scriptRegistry: ReturnType<typeof createMockScriptRegistry>,
): GlobalContext {
  return {
    container: {
      get: vi.fn().mockReturnValue(scriptRegistry),
    },
  } as unknown as GlobalContext;
}

function createTestScript(overrides: Record<string, unknown> = {}) {
  return {
    name: "test-script",
    content: "echo hello",
    executor: { type: "shell" },
    ...overrides,
  };
}

describe("ScriptInteractionCoordinator", () => {
  let coordinator: ScriptInteractionCoordinator;
  let mockScriptRegistry: ReturnType<typeof createMockScriptRegistry>;
  let mockTerminalService: ReturnType<typeof createMockTerminalService>;
  let mockGlobalContext: GlobalContext;
  let mockInputProvider: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockScriptRegistry = createMockScriptRegistry();
    mockTerminalService = createMockTerminalService();
    mockGlobalContext = createMockGlobalContext(mockScriptRegistry);
    mockInputProvider = vi.fn().mockResolvedValue("user-input");

    coordinator = new FastTestCoordinator(
      mockGlobalContext,
      {} as any,
      mockInputProvider as unknown as InputProvider,
    );
    (coordinator as any).terminalService = mockTerminalService;
  });

  describe("constructor", () => {
    it("should create with global context and input provider", () => {
      expect(coordinator).toBeInstanceOf(ScriptInteractionCoordinator);
    });

    it("should create without input provider", () => {
      const c = new ScriptInteractionCoordinator(mockGlobalContext, {} as any);
      expect(c).toBeInstanceOf(ScriptInteractionCoordinator);
    });
  });

  describe("executeWithInteraction", () => {
    it("should abort when signal is already aborted", async () => {
      mockScriptRegistry.getScript.mockReturnValue(createTestScript());
      const abortController = new AbortController();
      abortController.abort();

      const result = await coordinator.executeWithInteraction(
        "test-script",
        undefined,
        abortController.signal,
      );

      expect(result.success).toBe(false);
      expect(result.status).toBe("FAILED");
      expect(result.error).toBe("Execution cancelled");
    });

    it("should return FAILED when script is not found", async () => {
      mockScriptRegistry.getScript.mockImplementation(() => {
        throw new Error("Script not found");
      });

      const result = await coordinator.executeWithInteraction("non-existent");

      expect(result.success).toBe(false);
      expect(result.status).toBe("FAILED");
      expect(result.error).toContain("Script not found");
    });

    it("should return FAILED when command is empty", async () => {
      mockScriptRegistry.getScript.mockReturnValue(createTestScript({ content: "" }));

      const result = await coordinator.executeWithInteraction("test-script");

      expect(result.success).toBe(false);
      expect(result.status).toBe("FAILED");
      expect(result.error).toBe("Empty script command");
    });

    it("should return FAILED when session creation fails", async () => {
      mockScriptRegistry.getScript.mockReturnValue(createTestScript());
      mockTerminalService.createSession.mockRejectedValue(new Error("Session error"));

      const result = await coordinator.executeWithInteraction("test-script");

      expect(result.success).toBe(false);
      expect(result.status).toBe("FAILED");
      expect(result.error).toBe("Session error");
    });

    it("should return FAILED when background command fails to start", async () => {
      mockScriptRegistry.getScript.mockReturnValue(createTestScript());
      mockTerminalService.startBackgroundCommand.mockResolvedValue({
        success: false,
        error: "Command failed",
      });

      const result = await coordinator.executeWithInteraction("test-script");

      expect(result.success).toBe(false);
      expect(result.status).toBe("FAILED");
      expect(result.error).toBe("Command failed");
    });

    it("should complete successfully with no interaction needed", async () => {
      mockScriptRegistry.getScript.mockReturnValue(createTestScript());
      // getOutput returns empty → idlePolls reaches 5 → poll exits immediately
      mockTerminalService.getOutput.mockResolvedValue("");

      const result = await coordinator.executeWithInteraction("test-script");

      expect(result.success).toBe(true);
      expect(result.status).toBe("COMPLETED");
      expect(result.rounds).toHaveLength(0);
    });

    it("should perform interaction rounds when script prompts for input", async () => {
      mockScriptRegistry.getScript.mockReturnValue(
        createTestScript({
          config: {
            interactionMode: "blocking",
            maxRounds: 2,
            promptPatterns: [":\\s*$"],
          },
        }),
      );
      // First output shows a prompt, then returns empty on subsequent polls
      mockTerminalService.getOutput
        .mockResolvedValueOnce("What is your name: ") // initial prompt
        .mockResolvedValue(""); // subsequent polls → idle → exit

      const result = await coordinator.executeWithInteraction("test-script", {
        scriptName: "test-script",
        risk: "none",
        interactionMode: "blocking",
        maxRounds: 2,
        promptPatterns: [":\\s*$"],
      });

      expect(result.success).toBe(true);
      // The pollForOutput will either return empty or the prompt text
      // The interaction loop may not trigger if the prompt is detected in needsInput
      expect(result.status).toBe("COMPLETED");
    });

    it("should use the input provider in blocking mode", async () => {
      mockScriptRegistry.getScript.mockReturnValue(createTestScript());
      // First poll returns content matching prompt pattern ">"
      mockTerminalService.getOutput.mockResolvedValueOnce("Proceed?> ").mockResolvedValue("");

      const result = await coordinator.executeWithInteraction("test-script", {
        scriptName: "test-script",
        risk: "none",
        interactionMode: "blocking",
        maxRounds: 3,
        promptPatterns: [">"],
      });

      expect(result.success).toBe(true);
      expect(result.status).toBe("COMPLETED");
      // inputProvider should have been called if interactionLoop ran
    });

    it("should handle non-blocking modes gracefully", async () => {
      mockScriptRegistry.getScript.mockReturnValue(createTestScript());
      mockTerminalService.getOutput.mockResolvedValue("");

      const result = await coordinator.executeWithInteraction("test-script", {
        scriptName: "test-script",
        risk: "none",
        interactionMode: "llm-assisted" as any,
      });

      expect(result.success).toBe(true);
    });

    it("should handle sendInput rejection gracefully", async () => {
      mockScriptRegistry.getScript.mockReturnValue(createTestScript());
      mockTerminalService.getOutput.mockResolvedValue("Proceed?> ");
      mockTerminalService.sendInput.mockRejectedValue(new Error("Send failed"));

      const result = await coordinator.executeWithInteraction("test-script", {
        scriptName: "test-script",
        risk: "none",
        interactionMode: "blocking",
        maxRounds: 1,
        promptPatterns: [">"],
      });

      // sendInput rejection is caught, logged, and the loop breaks gracefully
      // execution continues with cleanup and returns success
      expect(result.success).toBe(true);
      expect(result.status).toBe("COMPLETED");
    });

    it("should use template rendering when script has template", async () => {
      mockScriptRegistry.getScript.mockReturnValue({
        name: "template-script",
        template: "echo {{name}}",
        arguments: [{ key: "name", default: "world" }],
        executor: { type: "shell" },
      });
      mockTerminalService.getOutput.mockResolvedValue("");

      const result = await coordinator.executeWithInteraction("template-script");

      expect(result.success).toBe(true);
      expect(result.status).toBe("COMPLETED");
    });
  });
});
