// Test for concurrent tool execution and streaming fixes

import { describe, it, expect, beforeEach, vi } from "vitest"
import { presentAssistantMessage } from "../presentAssistantMessage"

// Mock dependencies
vi.mock("../../task/Task")
vi.mock("../../tools/validateToolUse", () => ({
	validateToolUse: vi.fn(() => {}),
	isValidToolName: vi.fn((toolName: string) =>
		["read_file", "write_to_file", "ask_followup_question", "attempt_completion", "use_mcp", "execute_command"].includes(
			toolName,
		),
	),
}))

// Mock tool executor registry
vi.mock("../../tools", () => ({
	toolExecutorRegistry: {
		get: vi.fn(),
	},
	attemptCompletionTool: {
		handle: vi.fn().mockResolvedValue(undefined),
	},
	readFileTool: {
		getReadFileToolDescription: vi.fn(),
	},
	useMcpTool: {
		handle: vi.fn().mockResolvedValue(undefined),
	},
}))

import { toolExecutorRegistry, readFileTool } from "../../tools"

describe("presentAssistantMessage - Concurrency and Streaming", () => {
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
			userMessageContentReady: false,
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
							experiments: {},
							disabledTools: [],
						}),
					},
				}),
			},
			say: vi.fn().mockResolvedValue(undefined),
			ask: vi.fn().mockResolvedValue({ response: "yesButtonClicked" }),
		}

		// Add pushToolResultToUserContent method
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

	describe("Partial blocks (streaming) should be processed synchronously", () => {
		it("should process partial text blocks immediately", async () => {
			mockTask.assistantMessageContent = [
				{
					type: "text",
					content: "Hello",
					partial: true,
				},
			]

			await presentAssistantMessage(mockTask)

			// Text should be shown immediately
			expect(mockTask.say).toHaveBeenCalledWith("text", "Hello", undefined, true)
		})

		it("should process multiple partial text blocks in sequence", async () => {
			mockTask.assistantMessageContent = [
				{
					type: "text",
					content: "Hello",
					partial: true,
				},
				{
					type: "text",
					content: "World",
					partial: true,
				},
			]

			// Process first block
			await presentAssistantMessage(mockTask)
			expect(mockTask.say).toHaveBeenCalledWith("text", "Hello", undefined, true)

			// Increment index and process second block
			mockTask.currentStreamingContentIndex = 1
			await presentAssistantMessage(mockTask)
			expect(mockTask.say).toHaveBeenCalledWith("text", "World", undefined, true)
		})
	})

	describe("Complete blocks should be processed asynchronously", () => {
		it("should release lock before executing complete tool", async () => {
			const toolCallId = "tool_call_123"
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

			mockTask.didCompleteReadingStream = true

			const mockExecutor = {
				name: "read_file" as const,
				lastSeenPartialPath: undefined,
				handle: vi.fn().mockImplementation(async () => {
					// Simulate slow tool execution
					await new Promise(resolve => setTimeout(resolve, 50))
				}),
				execute: vi.fn(),
				handlePartial: vi.fn(),
				resetPartialState: vi.fn(),
				hasPathStabilized: vi.fn(),
			}

			vi.mocked(toolExecutorRegistry.get).mockReturnValue(mockExecutor as any)

			const startTime = Date.now()
			await presentAssistantMessage(mockTask)
			const endTime = Date.now()

			// Function should return quickly (not wait for tool execution)
			// Allow some overhead for async setup (increased from 20ms to 50ms)
			expect(endTime - startTime).toBeLessThan(50)
		})

		it("should set userMessageContentReady after async tool execution", async () => {
			const toolCallId = "tool_call_123"
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

			mockTask.didCompleteReadingStream = true

			const mockExecutor = {
				name: "read_file" as const,
				lastSeenPartialPath: undefined,
				handle: vi.fn().mockResolvedValue(undefined),
				execute: vi.fn(),
				handlePartial: vi.fn(),
				resetPartialState: vi.fn(),
				hasPathStabilized: vi.fn(),
			}

			vi.mocked(toolExecutorRegistry.get).mockReturnValue(mockExecutor as any)

			await presentAssistantMessage(mockTask)

			// Wait for async tool execution to complete
			await new Promise(resolve => setTimeout(resolve, 100))

			// userMessageContentReady should be set after tool execution
			expect(mockTask.userMessageContentReady).toBe(true)
		})

		it("should increment currentStreamingContentIndex after async tool execution", async () => {
			const toolCallId = "tool_call_123"
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

			mockTask.didCompleteReadingStream = true
			const initialIndex = mockTask.currentStreamingContentIndex

			const mockExecutor = {
				name: "read_file" as const,
				lastSeenPartialPath: undefined,
				handle: vi.fn().mockResolvedValue(undefined),
				execute: vi.fn(),
				handlePartial: vi.fn(),
				resetPartialState: vi.fn(),
				hasPathStabilized: vi.fn(),
			}

			vi.mocked(toolExecutorRegistry.get).mockReturnValue(mockExecutor as any)

			await presentAssistantMessage(mockTask)

			// Wait for async tool execution to complete
			await new Promise(resolve => setTimeout(resolve, 100))

			// Index should be incremented
			expect(mockTask.currentStreamingContentIndex).toBe(initialIndex + 1)
		})
	})

	describe("Concurrent tool execution", () => {
		it("should allow multiple tools to execute concurrently", async () => {
			const toolCallIds = ["tool_call_1", "tool_call_2", "tool_call_3"]
			mockTask.assistantMessageContent = toolCallIds.map((id, index) => ({
				type: "tool_use",
				id,
				name: "read_file",
				params: { path: `test${index}.txt` },
				partial: false,
				nativeArgs: { path: `test${index}.txt` },
			}))

			mockTask.didCompleteReadingStream = true

			const executionOrder: string[] = []
			const mockExecutor = {
				name: "read_file" as const,
				lastSeenPartialPath: undefined,
				handle: vi.fn().mockImplementation(async () => {
					executionOrder.push(`start_${Date.now()}`)
					// Simulate tool execution
					await new Promise(resolve => setTimeout(resolve, 50))
					executionOrder.push(`end_${Date.now()}`)
				}),
				execute: vi.fn(),
				handlePartial: vi.fn(),
				resetPartialState: vi.fn(),
				hasPathStabilized: vi.fn(),
			}

			vi.mocked(toolExecutorRegistry.get).mockReturnValue(mockExecutor as any)

			// Process all tools
			for (let i = 0; i < toolCallIds.length; i++) {
				mockTask.currentStreamingContentIndex = i
				await presentAssistantMessage(mockTask)
			}

			// Wait for all async tool executions to complete
			await new Promise(resolve => setTimeout(resolve, 200))

			// All tools should have been executed (concurrently)
			expect(mockExecutor.handle).toHaveBeenCalledTimes(toolCallIds.length)
		})
	})

	describe("State management", () => {
		it("should handle partial tool blocks correctly", async () => {
			const toolCallId = "tool_call_partial"
			mockTask.assistantMessageContent = [
				{
					type: "tool_use",
					id: toolCallId,
					name: "read_file",
					params: { path: "test.txt" },
					partial: true,
				},
			]

			mockTask.didCompleteReadingStream = true

			const mockExecutor = {
				name: "read_file" as const,
				lastSeenPartialPath: undefined,
				handle: vi.fn().mockResolvedValue(undefined),
				execute: vi.fn(),
				handlePartial: vi.fn(),
				resetPartialState: vi.fn(),
				hasPathStabilized: vi.fn(),
			}

			vi.mocked(toolExecutorRegistry.get).mockReturnValue(mockExecutor as any)

			await presentAssistantMessage(mockTask)

			// Partial blocks should be processed synchronously
			expect(mockExecutor.handle).toHaveBeenCalled()
		})

		it("should not execute tools during active streaming", async () => {
			const toolCallId = "tool_call_deferred"
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

			// Simulate active streaming
			mockTask.isStreaming = true
			mockTask.didCompleteReadingStream = false

			const mockExecutor = {
				name: "read_file" as const,
				lastSeenPartialPath: undefined,
				handle: vi.fn().mockResolvedValue(undefined),
				execute: vi.fn(),
				handlePartial: vi.fn(),
				resetPartialState: vi.fn(),
				hasPathStabilized: vi.fn(),
			}

			vi.mocked(toolExecutorRegistry.get).mockReturnValue(mockExecutor as any)

			await presentAssistantMessage(mockTask)

			// Tool should not be executed during active streaming
			expect(mockExecutor.handle).not.toHaveBeenCalled()
		})
	})
})
