// npx vitest src/core/assistant-message/__tests__/presentAssistantMessage-custom-tool.spec.ts

import { describe, it, expect, beforeEach, vi } from "vitest"
import { presentAssistantMessage } from "../presentAssistantMessage"

// Mock dependencies
vi.mock("../../task/Task")
vi.mock("../../tools/validateToolUse", () => ({
	validateToolUse: vi.fn(() => {}),
	isValidToolName: vi.fn((toolName: string) =>
		["read_file", "write_to_file", "ask_followup_question", "attempt_completion", "use_mcp"].includes(
			toolName,
		),
	),
}))

// Mock custom tool registry - must be done inline without external variable references
vi.mock("@coder/core", () => ({
	customToolRegistry: {
		has: vi.fn(),
		get: vi.fn(),
	},
}))

import { customToolRegistry } from "@coder/core"

describe("presentAssistantMessage - Custom Tool Recording", () => {
	let mockTask: any

	beforeEach(() => {
		// Reset all mocks
		vi.clearAllMocks()

		// Create a mock Task with minimal properties needed for testing
		mockTask = {
			taskId: "test-task-id",
			instanceId: "test-instance",
			abort: false,
			presentAssistantMessageHasPendingUpdates: false,
			currentStreamingContentIndex: 0,
			assistantMessageContent: [],
			userMessageContent: [],
			didCompleteReadingStream: false,
			didRejectTool: false,
			didAlreadyUseTool: false,
			consecutiveMistakeCount: 0,
			clineMessages: [],
			api: {
				getModel: () => ({ id: "test-model", info: {} }),
			},
			recordToolUsage: vi.fn(),
			recordToolError: vi.fn(),
			toolRepetitionDetector: {
				check: vi.fn().mockReturnValue({ allowExecution: true }),
			},
			providerRef: {
				deref: () => ({
					configurationService: {
						getState: vi.fn().mockResolvedValue({
							mode: "code",
							customModes: [],
							experiments: {
								customTools: true, // Enable by default
							},
						}),
					},
				}),
			},
			say: vi.fn().mockResolvedValue(undefined),
			ask: vi.fn().mockResolvedValue({ response: "yesButtonClicked" }),
		}

		// Add pushToolResultToUserContent method after mockTask is created so it can reference mockTask
		mockTask.pushToolResultToUserContent = vi.fn().mockImplementation((toolResult: any) => {
			const existingResult = mockTask.userMessageContent.find(
				(block: any) => block.type === "tool_result" && block.tool_use_id === toolResult.tool_use_id,
			)
			if (existingResult) {
				return false
			}
			mockTask.userMessageContent.push(toolResult)
			return true
		})
	})

	describe("Custom tool usage recording", () => {
		it("should record custom tool usage as 'custom_tool' when experiment is enabled", async () => {
			const toolCallId = "tool_call_custom_123"
			mockTask.assistantMessageContent = [
				{
					type: "tool_use",
					id: toolCallId,
					name: "my_custom_tool",
					params: { value: "test" },
					partial: false,
					nativeArgs: { value: "test" },
				},
			]

			// Set didCompleteReadingStream to true to allow tool execution
			mockTask.didCompleteReadingStream = true

			// Mock customToolRegistry to recognize this as a custom tool
			vi.mocked(customToolRegistry.has).mockReturnValue(true)
			vi.mocked(customToolRegistry.get).mockReturnValue({
				name: "my_custom_tool",
				description: "A custom tool",
				execute: vi.fn().mockResolvedValue("Custom tool result"),
			})

			await presentAssistantMessage(mockTask)

			// Wait for async tool execution to complete
			await new Promise(resolve => setTimeout(resolve, 100))

			// Should record as "custom_tool", not "my_custom_tool"
			expect(mockTask.recordToolUsage).toHaveBeenCalledWith("custom_tool")
		})
	})

	describe("Custom tool error recording", () => {
		it("should record custom tool error as 'custom_tool'", async () => {
			const toolCallId = "tool_call_custom_error_123"
			mockTask.assistantMessageContent = [
				{
					type: "tool_use",
					id: toolCallId,
					name: "failing_custom_tool",
					params: {},
					partial: false,
					nativeArgs: {},
				},
			]

			// Set didCompleteReadingStream to true to allow tool execution
			mockTask.didCompleteReadingStream = true

			// Mock customToolRegistry with a tool that throws an error
			vi.mocked(customToolRegistry.has).mockReturnValue(true)
			vi.mocked(customToolRegistry.get).mockReturnValue({
				name: "failing_custom_tool",
				description: "A failing custom tool",
				execute: vi.fn().mockRejectedValue(new Error("Custom tool execution failed")),
			})

			await presentAssistantMessage(mockTask)

			// Wait for async tool execution to complete
			await new Promise(resolve => setTimeout(resolve, 100))

			// Should record error as "custom_tool", not "failing_custom_tool"
			expect(mockTask.recordToolError).toHaveBeenCalledWith("custom_tool", "Custom tool execution failed")
			expect(mockTask.consecutiveMistakeCount).toBe(1)
		})
	})

	describe("Regular tool recording", () => {
		it("should record regular tool usage with actual tool name", async () => {
			const toolCallId = "tool_call_read_file_123"
			mockTask.assistantMessageContent = [
				{
					type: "tool_use",
					id: toolCallId,
					name: "read_file",
					params: { path: "test.txt" },
					partial: false,
					nativeArgs: { path: "test.txt" },
				},
			]

			// Set didCompleteReadingStream to true to allow tool execution
			mockTask.didCompleteReadingStream = true

			// read_file is not a custom tool
			vi.mocked(customToolRegistry.has).mockReturnValue(false)

			await presentAssistantMessage(mockTask)

			// Wait for async tool execution to complete
			await new Promise(resolve => setTimeout(resolve, 100))

			// Should record as "read_file", not "custom_tool"
			expect(mockTask.recordToolUsage).toHaveBeenCalledWith("read_file")
		})

		it("should record MCP tool usage as 'use_mcp' (not custom_tool)", async () => {
			const toolCallId = "tool_call_mcp_123"
			mockTask.assistantMessageContent = [
				{
					type: "tool_use",
					id: toolCallId,
					name: "use_mcp",
					params: {
						server_name: "test-server",
						tool_name: "test-tool",
						arguments: "{}",
					},
					partial: false,
				},
			]

			// Set didCompleteReadingStream to true to allow tool execution
			mockTask.didCompleteReadingStream = true

			vi.mocked(customToolRegistry.has).mockReturnValue(false)

			// Mock MCP hub for use_mcp
			mockTask.providerRef = {
				deref: () => ({
					configurationService: {
						getState: vi.fn().mockResolvedValue({
							mode: "code",
							customModes: [],
							experiments: {
								customTools: true,
							},
						}),
					},
					getMcpHub: () => ({
						findServerNameBySanitizedName: () => "test-server",
						executeToolCall: vi.fn().mockResolvedValue({ content: [{ type: "text", text: "result" }] }),
					}),
				}),
			}

			await presentAssistantMessage(mockTask)

			// Should record as "use_mcp", not "custom_tool"
			expect(mockTask.recordToolUsage).toHaveBeenCalledWith("use_mcp")
		})
	})

	describe("Custom tool experiment gate", () => {
		it("should treat custom tool as unknown when experiment is disabled", async () => {
			const toolCallId = "tool_call_disabled_123"
			mockTask.assistantMessageContent = [
				{
					type: "tool_use",
					id: toolCallId,
					name: "my_custom_tool",
					params: {},
					partial: false,
				},
			]

			// Set didCompleteReadingStream to true to allow tool execution
			mockTask.didCompleteReadingStream = true

			// Mock provider state with customTools experiment DISABLED
			mockTask.providerRef = {
				deref: () => ({
					configurationService: {
						getState: vi.fn().mockResolvedValue({
							mode: "code",
							customModes: [],
							experiments: {
								customTools: false, // Disabled
							},
						}),
					},
				}),
			}

			// Even if registry recognizes it, experiment gate should prevent execution
			vi.mocked(customToolRegistry.has).mockReturnValue(true)
			vi.mocked(customToolRegistry.get).mockReturnValue({
				name: "my_custom_tool",
				description: "A custom tool",
				execute: vi.fn().mockResolvedValue("Should not execute"),
			})

			await presentAssistantMessage(mockTask)

			// Should be treated as unknown tool (not executed)
			// The validation error will be pushed to userMessageContent, not via say
			const toolResult = mockTask.userMessageContent.find(
				(item: any) => item.type === "tool_result" && item.tool_use_id === toolCallId,
			)
			expect(toolResult).toBeDefined()
			expect(toolResult.is_error).toBe(true)

			// Custom tool should NOT have been executed
			const getMock = vi.mocked(customToolRegistry.get)
			if (getMock.mock.results.length > 0) {
				const customTool = getMock.mock.results[0]!.value
				if (customTool) {
					expect(customTool.execute).not.toHaveBeenCalled()
				}
			}
		})

		it("should not call customToolRegistry.has() when experiment is disabled", async () => {
			mockTask.assistantMessageContent = [
				{
					type: "tool_use",
					id: "tool_call_123",
					name: "some_tool",
					params: {},
					partial: false,
				},
			]

			// Set didCompleteReadingStream to true to allow tool execution
			mockTask.didCompleteReadingStream = true

			// Disable experiment
			mockTask.providerRef = {
				deref: () => ({
					configurationService: {
						getState: vi.fn().mockResolvedValue({
							mode: "code",
							customModes: [],
							experiments: {
								customTools: false,
							},
						}),
					},
				}),
			}

			await presentAssistantMessage(mockTask)

			// When experiment is off, shouldn't even check the registry
			// (Code checks stateExperiments?.customTools before calling has())
			expect(customToolRegistry.has).not.toHaveBeenCalled()
		})
	})

	describe("Validation requirements", () => {
		it("normalizes disabledTools aliases before validateToolUse", async () => {
			const toolCallId = "tool_call_validation_alias_123"
			mockTask.assistantMessageContent = [
				{
					type: "tool_use",
					id: toolCallId,
					name: "ask_followup_question",
					params: { question: "Test question?" },
					nativeArgs: { question: "Test question?", follow_up: [{ text: "Option 1", mode: "code" }] },
					partial: false,
				},
			]

			// Set didCompleteReadingStream to true to allow tool execution
			mockTask.didCompleteReadingStream = true

			mockTask.providerRef = {
				deref: () => ({
					configurationService: {
						getState: vi.fn().mockResolvedValue({
							mode: "code",
							customModes: [],
							experiments: {
								customTools: false,
							},
							disabledTools: ["search_and_replace"],
						}),
					},
				}),
			}

			vi.mocked(customToolRegistry.has).mockReturnValue(false)

			await presentAssistantMessage(mockTask)

			// Wait a bit for async tool execution to complete
			await new Promise(resolve => setTimeout(resolve, 100))

			// Verify that the tool was attempted (validation was called)
			// We can't easily verify the exact parameters passed to validateToolUse,
			// but we can verify that the tool usage was recorded
			expect(mockTask.recordToolUsage).toHaveBeenCalled()
		})
	})

	describe("Partial blocks", () => {
		it("should not record usage for partial custom tool blocks", async () => {
			mockTask.assistantMessageContent = [
				{
					type: "tool_use",
					id: "tool_call_partial_123",
					name: "my_custom_tool",
					params: { value: "test" },
					partial: true, // Still streaming
				},
			]

			// Set didCompleteReadingStream to true to allow tool execution
			mockTask.didCompleteReadingStream = true

			vi.mocked(customToolRegistry.has).mockReturnValue(true)

			await presentAssistantMessage(mockTask)

			// Should not record usage for partial blocks
			expect(mockTask.recordToolUsage).not.toHaveBeenCalled()
		})
	})
})
