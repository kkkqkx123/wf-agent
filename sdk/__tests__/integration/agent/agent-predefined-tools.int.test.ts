/**
 * Integration Test: Agent with Predefined Tools
 *
 * Tests the agent loop execution with real predefined tool implementations
 * (filesystem read/glob/list_files/grep etc.) registered in the ToolRegistry.
 *
 * Uses MockLLMWrapper to instruct "which tool to call" while the actual
 * tool handler executes against the real filesystem. This validates:
 * - Tool registration and schema resolution in the full coordinator chain
 * - Tool execution with real side effects (filesystem reads)
 * - Tool results flowing through the agent loop conversation
 * - Error handling for missing tools and invalid arguments
 *
 * Fixtures: predefined-tools-fixtures.ts provides the setup.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { createPredefinedToolsFixture, createAgentConfigWithTools } from "./__shared/predefined-tools-fixtures.js";
import type { FullAgentLoopTestFixture } from "./__shared/fixtures.js";
import type { AgentLoopResult, ToolCallRecord } from "@wf-agent/types";

// =============================================================================
// Constants
// =============================================================================

/**
 * Relative paths from the SDK root (process.cwd() when running from sdk/):
 *
 * When running `pnpm test` from sdk/, process.cwd() == D:\...\wf-agent\sdk
 * The predefined-tools-fixture sets workspaceDir = process.cwd(), so all tool
 * paths are relative to sdk/.
 */
const TESTS_RELATIVE = "__tests__/integration/agent";
const CURRENT_FILE_RELATIVE = `${TESTS_RELATIVE}/agent-predefined-tools.int.test.ts`;
const SHARED_DIR_RELATIVE = `${TESTS_RELATIVE}/__shared`;

// =============================================================================
// Helper
// =============================================================================

/**
 * Iterate through the entity's iteration history and return the result of
 * the first tool call found.  Returns `undefined` when no tool call exists.
 */
function getFirstToolResult(fixture: FullAgentLoopTestFixture): unknown {
  const entities = fixture.registry.getCompleted();
  expect(entities.length).toBeGreaterThanOrEqual(1);
  const entity = entities[entities.length - 1]!;
  for (const record of entity.state.iterationHistory) {
    if (record.toolCalls.length > 0) {
      return record.toolCalls[0]!.result;
    }
  }
  return undefined;
}

/**
 * Extract all tool call records from the entity's iteration history.
 */
function getAllToolCalls(fixture: FullAgentLoopTestFixture): ToolCallRecord[] {
  const entities = fixture.registry.getCompleted();
  expect(entities.length).toBeGreaterThanOrEqual(1);
  const entity = entities[entities.length - 1]!;
  const calls: ToolCallRecord[] = [];
  for (const record of entity.state.iterationHistory) {
    calls.push(...record.toolCalls);
  }
  return calls;
}

// =============================================================================
// Tests
// =============================================================================

describe("Agent with Predefined Tools", () => {
  let fixture: FullAgentLoopTestFixture;

  beforeEach(async () => {
    fixture = await createPredefinedToolsFixture();
  });

  afterEach(async () => {
    await fixture.storage.clear();
    fixture.mockLLMWrapper.reset();
  });

  // ===========================================================================
  // list_files
  // ===========================================================================

  it("should execute list_files tool and return directory listing", async () => {
    fixture.mockLLMWrapper.setResponseSequence([
      {
        content: "",
        toolCalls: [
          {
            id: "call_list_files_1",
            name: "list_files",
            arguments: JSON.stringify({ path: ".", recursive: false }),
          },
        ],
      },
      {
        content: "The directory listing was obtained successfully.",
      },
    ]);

    const config = createAgentConfigWithTools(["list_files"]);
    const result: AgentLoopResult = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
    expect(result.toolCallCount).toBe(1);
    expect(result.iterations).toBe(2);
    expect(result.content).toContain("successfully");

    const toolResult = getFirstToolResult(fixture) as
      | { success: boolean; result: { result: { entries: Array<unknown>; display: string; total: number; truncated: boolean } } }
      | undefined;
    expect(toolResult).toBeDefined();
    expect(toolResult!.success).toBe(true);
    // Structured return — array access without string parsing
    expect(Array.isArray(toolResult!.result.result.entries)).toBe(true);
    expect(toolResult!.result.result.entries.length).toBeGreaterThan(0);
    expect(toolResult!.result.result.display).toContain("[DIR]");
    expect(toolResult!.result.result.total).toBeGreaterThan(0);
  });

  it("should execute list_files tool with recursive=true on a subdirectory", async () => {
    fixture.mockLLMWrapper.setResponseSequence([
      {
        content: "",
        toolCalls: [
          {
            id: "call_list_files_2",
            name: "list_files",
            // Use __tests__ subdirectory to avoid permission errors at root
            arguments: JSON.stringify({ path: TESTS_RELATIVE, recursive: true }),
          },
        ],
      },
      {
        content: "Recursive listing completed.",
      },
    ]);

    const config = createAgentConfigWithTools(["list_files"]);
    const result: AgentLoopResult = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
    expect(result.toolCallCount).toBe(1);
    expect(result.iterations).toBe(2);

    const toolResult = getFirstToolResult(fixture) as
      | { success: boolean; result: { result: { entries: Array<{ name: string; type: string; path: string }>; display: string; total: number; truncated: boolean } } }
      | undefined;
    expect(toolResult).toBeDefined();
    expect(toolResult!.success).toBe(true);
    // Structured return — check entries array + display string
    expect(toolResult!.result.result.display).toContain("[DIR]");
    expect(toolResult!.result.result.display).toMatch(/Summary:/);
    expect(toolResult!.result.result.display).toContain("agent-predefined-tools.int.test.ts");
    // Direct array access (no string parsing)
    const entryPaths = toolResult!.result.result.entries.map(e => e.path);
    expect(entryPaths.some((p: string) => p.includes("agent-predefined-tools.int.test.ts"))).toBe(true);
  });

  it("should handle list_files on non-existent directory gracefully", async () => {
    fixture.mockLLMWrapper.setResponseSequence([
      {
        content: "",
        toolCalls: [
          {
            id: "call_list_files_err",
            name: "list_files",
            arguments: JSON.stringify({ path: "non_existent_dir_xyz_123" }),
          },
        ],
      },
      {
        content: "The tool reported an error.",
      },
    ]);

    const config = createAgentConfigWithTools(["list_files"]);
    const result: AgentLoopResult = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
    expect(result.toolCallCount).toBe(1);

    const toolResult = getFirstToolResult(fixture) as { success: boolean; error?: string } | undefined;
    expect(toolResult).toBeDefined();
    expect(toolResult!.success).toBe(false);
    expect(toolResult!.error).toContain("not found");
  });

  // ===========================================================================
  // glob
  // ===========================================================================

  it("should execute glob tool and return matching files", async () => {
    // Glob *.ts inside the agent test directory
    // Note: `**/*.ts` already implies recursive matching; no separate recursive flag needed
    fixture.mockLLMWrapper.setResponseSequence([
      {
        content: "",
        toolCalls: [
          {
            id: "call_glob_1",
            name: "glob",
            arguments: JSON.stringify({
              path: TESTS_RELATIVE,
              pattern: "**/*.ts",
            }),
          },
        ],
      },
      {
        content: "Glob results found.",
      },
    ]);

    const config = createAgentConfigWithTools(["glob"]);
    const result: AgentLoopResult = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
    expect(result.toolCallCount).toBe(1);
    expect(result.iterations).toBe(2);

    const toolResult = getFirstToolResult(fixture) as
      | { success: boolean; result: { result: { entries: Array<unknown>; display: string } } }
      | undefined;
    expect(toolResult).toBeDefined();
    expect(toolResult!.success).toBe(true);
    // Structured array — no string parsing needed
    expect(Array.isArray(toolResult!.result.result.entries)).toBe(true);
    expect(toolResult!.result.result.entries.length).toBeGreaterThan(0);
    // Display string should contain the test file name
    expect(toolResult!.result.result.display).toContain(".ts");
    expect(toolResult!.result.result.display).toContain("agent-predefined-tools.int.test.ts");
  });

  it("should execute glob tool inside __shared directory", async () => {
    fixture.mockLLMWrapper.setResponseSequence([
      {
        content: "",
        toolCalls: [
          {
            id: "call_glob_2",
            name: "glob",
            arguments: JSON.stringify({
              path: SHARED_DIR_RELATIVE,
              pattern: "**/*.ts",
            }),
          },
        ],
      },
      {
        content: "Glob found test fixture files.",
      },
    ]);

    const config = createAgentConfigWithTools(["glob"]);
    const result: AgentLoopResult = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
    expect(result.toolCallCount).toBe(1);

    const toolResult = getFirstToolResult(fixture) as
      | { success: boolean; result: { result: { entries: Array<{ path: string }>; display: string } } }
      | undefined;
    expect(toolResult).toBeDefined();
    expect(toolResult!.success).toBe(true);
    // Should find fixture files like fixtures.ts, mock-llm-wrapper.ts, predefined-tools-fixtures.ts
    expect(toolResult!.result.result.display).toContain("fixtures.ts");
    // Structured array access
    const entryPaths = toolResult!.result.result.entries.map((e) => e.path);
    expect(entryPaths.some((p: string) => p.includes("predefined-tools-fixtures"))).toBe(true);
  });

  // ===========================================================================
  // read_file
  // ===========================================================================

  it("should execute read_file tool and return file content", async () => {
    fixture.mockLLMWrapper.setResponseSequence([
      {
        content: "",
        toolCalls: [
          {
            id: "call_read_1",
            name: "read_file",
            arguments: JSON.stringify({
              path: CURRENT_FILE_RELATIVE,
              limit: 50,
            }),
          },
        ],
      },
      {
        content: "The file contains integration test code.",
      },
    ]);

    const config = createAgentConfigWithTools(["read_file"]);
    const result: AgentLoopResult = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
    expect(result.toolCallCount).toBe(1);

    const toolResult = getFirstToolResult(fixture) as { success: boolean; result: { result: string } } | undefined;
    expect(toolResult).toBeDefined();
    expect(toolResult!.success).toBe(true);
    // Should contain text from this test file
    expect(toolResult!.result.result).toContain("Agent with Predefined Tools");
  });

  it("should handle read_file on non-existent file gracefully", async () => {
    fixture.mockLLMWrapper.setResponseSequence([
      {
        content: "",
        toolCalls: [
          {
            id: "call_read_err",
            name: "read_file",
            arguments: JSON.stringify({ path: "non_existent_file_xyz_123.ts" }),
          },
        ],
      },
      {
        content: "The tool reported an error for the missing file.",
      },
    ]);

    const config = createAgentConfigWithTools(["read_file"]);
    const result: AgentLoopResult = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
    expect(result.toolCallCount).toBe(1);

    const toolResult = getFirstToolResult(fixture) as { success: boolean; error?: string } | undefined;
    expect(toolResult).toBeDefined();
    expect(toolResult!.success).toBe(false);
    expect(toolResult!.error).toContain("not found");
  });

  // ===========================================================================
  // grep
  // ===========================================================================

  it("should execute grep tool and return matching lines", async () => {
    fixture.mockLLMWrapper.setResponseSequence([
      {
        content: "",
        toolCalls: [
          {
            id: "call_grep_1",
            name: "grep",
            arguments: JSON.stringify({
              path: SHARED_DIR_RELATIVE,
              regex: "createPredefinedTools",
            }),
          },
        ],
      },
      {
        content: "Grep found matching lines.",
      },
    ]);

    const config = createAgentConfigWithTools(["grep"]);
    const result: AgentLoopResult = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
    expect(result.toolCallCount).toBe(1);

    const toolResult = getFirstToolResult(fixture) as {
      success: boolean;
      result?: { result?: string };
      error?: string;
    } | undefined;
    expect(toolResult).toBeDefined();

    // The grep tool requires ripgrep installed on the host. If it's not
    // available the error will indicate that instead of the search result.
    if (toolResult!.success) {
      expect(toolResult!.result!.result).toContain("createPredefinedTools");
    } else {
      // ripgrep not available — that's acceptable in CI environments
      expect(toolResult!.error).toMatch(/ripgrep|not found|not installed/i);
    }
  });

  it("should handle grep with no matches gracefully", async () => {
    fixture.mockLLMWrapper.setResponseSequence([
      {
        content: "",
        toolCalls: [
          {
            id: "call_grep_none",
            name: "grep",
            arguments: JSON.stringify({
              path: SHARED_DIR_RELATIVE,
              regex: "THIS_PATTERN_SHOULD_NOT_MATCH_ANYTHING_XYZ_999",
            }),
          },
        ],
      },
      {
        content: "Grep completed.",
      },
    ]);

    const config = createAgentConfigWithTools(["grep"]);
    const result: AgentLoopResult = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
    expect(result.toolCallCount).toBe(1);

    const toolResult = getFirstToolResult(fixture) as {
      success: boolean;
      result?: { result?: string };
      error?: string;
    } | undefined;
    expect(toolResult).toBeDefined();

    if (toolResult!.success) {
      // grep returns "No matches found" as success with content
      expect(toolResult!.result!.result).toContain("No matches found");
    } else {
      // ripgrep not available
      expect(toolResult!.error).toMatch(/ripgrep|not found|not installed/i);
    }
  });

  // ===========================================================================
  // Multiple Tool Calls in Sequence
  // ===========================================================================

  it("should execute two sequential tool calls (list_files then glob)", async () => {
    fixture.mockLLMWrapper.setResponseSequence([
      {
        content: "",
        toolCalls: [
          {
            id: "call_seq_list",
            name: "list_files",
            arguments: JSON.stringify({ path: ".", recursive: false }),
          },
        ],
      },
      {
        content: "",
        toolCalls: [
          {
            id: "call_seq_glob",
            name: "glob",
            arguments: JSON.stringify({
              path: TESTS_RELATIVE,
              pattern: "*.ts",
              recursive: false,
            }),
          },
        ],
      },
      {
        content: "Both tools executed successfully.",
      },
    ]);

    const config = createAgentConfigWithTools(["list_files", "glob"], { maxIterations: 5 });
    const result: AgentLoopResult = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
    expect(result.toolCallCount).toBe(2);
    expect(result.iterations).toBe(3);
    expect(result.content).toContain("successfully");

    const toolCalls = getAllToolCalls(fixture);
    expect(toolCalls).toHaveLength(2);
    expect(toolCalls[0]!.name).toBe("list_files");
    expect(toolCalls[1]!.name).toBe("glob");
  });

  // ===========================================================================
  // Tool Not Found
  // ===========================================================================

  it("should handle agent calling a non-existent tool gracefully", async () => {
    fixture.mockLLMWrapper.setResponseSequence([
      {
        content: "",
        toolCalls: [
          {
            id: "call_nonexistent",
            name: "non_existent_tool_xyz",
            arguments: JSON.stringify({}),
          },
        ],
      },
      {
        content: "The tool was not found, but I continued.",
      },
    ]);

    const config = createAgentConfigWithTools(["list_files"]);
    const result: AgentLoopResult = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
    expect(result.toolCallCount).toBe(1);
    expect(result.iterations).toBe(2);

    const toolCalls = getAllToolCalls(fixture);
    expect(toolCalls).toHaveLength(1);
    expect(toolCalls[0]!.name).toBe("non_existent_tool_xyz");
    expect(toolCalls[0]!.error).toBeDefined();
    expect(toolCalls[0]!.error).toContain("not found");
  });

  // ===========================================================================
  // maxIterations reached with tool calls
  // ===========================================================================

  it("should use tools across multiple iterations until maxIterations", async () => {
    const sequence = Array.from({ length: 5 }, (_, i) => ({
      content: "",
      toolCalls: [
        {
          id: `call_iter_${i + 1}`,
          name: "list_files" as const,
          arguments: JSON.stringify({ path: ".", recursive: false }),
        },
      ],
    }));

    fixture.mockLLMWrapper.setResponseSequence(sequence);

    const config = createAgentConfigWithTools(["list_files"], { maxIterations: 3 });
    const result: AgentLoopResult = await fixture.coordinator.execute(config);

    expect(result.success).toBe(true);
    expect(result.iterations).toBe(3);
    expect(result.toolCallCount).toBe(3);

    const toolCalls = getAllToolCalls(fixture);
    expect(toolCalls).toHaveLength(3);
    for (const tc of toolCalls) {
      expect(tc.name).toBe("list_files");
    }
  });
});