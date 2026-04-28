// Test for streaming text blocking issue
// This test verifies that partial text blocks are processed without blocking

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

describe("presentAssistantMessage - Streaming Text Blocking Issue", () => {
	let mockTask: any
	let sayCallTimes: number[] = []
	let sayDelays: number[] = []

	beforeEach(() => {
		// Reset all mocks
		vi.clearAllMocks()
		sayCallTimes = []
		sayDelays = []

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
			// Mock say method that simulates slow UI updates
			say: vi.fn().mockImplementation(async (type: string, text?: string, images?: string[], partial?: boolean) => {
				const callTime = Date.now()
				sayCallTimes.push(callTime)
				
				// Simulate slow webview message update (100ms delay)
				// This mimics the real behavior where postMessageToWebview takes time
				await new Promise(resolve => setTimeout(resolve, 100))
				
				const endTime = Date.now()
				sayDelays.push(endTime - callTime)
			}),
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

	describe("CRITICAL: Streaming text blocks should not block each other", () => {
		it("should process multiple partial text blocks concurrently (not sequentially)", async () => {
			// Setup: 5 partial text blocks
			const textBlocks = [
				{ type: "text", content: "Hello", partial: true },
				{ type: "text", content: "Hello World", partial: true },
				{ type: "text", content: "Hello World!", partial: true },
				{ type: "text", content: "Hello World! How", partial: true },
				{ type: "text", content: "Hello World! How are", partial: true },
			]
			
			mockTask.assistantMessageContent = textBlocks

			const startTime = Date.now()

			// Process all blocks sequentially (simulating streaming processor behavior)
			for (let i = 0; i < textBlocks.length; i++) {
				mockTask.currentStreamingContentIndex = i
				await presentAssistantMessage(mockTask)
			}

			const endTime = Date.now()
			const totalTime = endTime - startTime

			// EXPECTED BEHAVIOR: All blocks should be processed quickly
			// If each say() takes 100ms and they run sequentially, total time would be ~500ms
			// If they run concurrently (non-blocking), total time should be ~100-150ms
			
			console.log(`Total time: ${totalTime}ms`)
			console.log(`Say call times: ${sayCallTimes.length}`)
			console.log(`Say delays: ${sayDelays.join(', ')}`)

			// CRITICAL TEST: This should FAIL if blocking exists
			// If the current implementation is blocking, this will take ~500ms
			// If fixed, it should take ~100-150ms (allowing some overhead)
			expect(totalTime).toBeLessThan(200) // Should be much less than 500ms if non-blocking
			
			// All say calls should have been made
			expect(mockTask.say).toHaveBeenCalledTimes(textBlocks.length)
		})

		it("should verify that say() is called asynchronously (not awaited)", async () => {
			mockTask.assistantMessageContent = [
				{ type: "text", content: "Test", partial: true },
			]

			const startTime = Date.now()
			await presentAssistantMessage(mockTask)
			const endTime = Date.now()

			// If say() is awaited, this would take ~100ms
			// If say() is NOT awaited (async), this should return immediately
			const executionTime = endTime - startTime
			
			console.log(`Execution time: ${executionTime}ms`)

			// CRITICAL: presentAssistantMessage should return immediately
			// NOT wait for say() to complete
			expect(executionTime).toBeLessThan(50) // Should be nearly instant
		})

		it("should handle rapid consecutive partial text updates", async () => {
			// Simulate rapid streaming: 10 text chunks arriving quickly
			const rapidUpdates = Array.from({ length: 10 }, (_, i) => ({
				type: "text",
				content: `Chunk ${i}`,
				partial: true,
			}))

			mockTask.assistantMessageContent = rapidUpdates

			const startTime = Date.now()

			// Process all updates
			for (let i = 0; i < rapidUpdates.length; i++) {
				mockTask.currentStreamingContentIndex = i
				await presentAssistantMessage(mockTask)
			}

			const endTime = Date.now()
			const totalTime = endTime - startTime

			console.log(`Rapid updates total time: ${totalTime}ms`)

			// If blocking: 10 chunks * 100ms = 1000ms
			// If non-blocking: ~100-150ms
			expect(totalTime).toBeLessThan(300) // Allow some overhead
		})
	})

	describe("Mutex behavior for partial text blocks", () => {
		it("should NOT acquire mutex for partial text blocks", async () => {
			mockTask.assistantMessageContent = [
				{ type: "text", content: "Test", partial: true },
			]

			// First call should process without mutex
			await presentAssistantMessage(mockTask)
			
			// Second call should also process immediately (not blocked by mutex)
			mockTask.assistantMessageContent = [
				{ type: "text", content: "Test Updated", partial: true },
			]
			
			const startTime = Date.now()
			await presentAssistantMessage(mockTask)
			const endTime = Date.now()

			// Should not be blocked by mutex
			expect(endTime - startTime).toBeLessThan(50)
		})
	})

	describe("Comparison: Complete text blocks should still be processed normally", () => {
		it("should process complete text blocks normally", async () => {
			mockTask.assistantMessageContent = [
				{ type: "text", content: "Complete message", partial: false },
			]

			await presentAssistantMessage(mockTask)

			expect(mockTask.say).toHaveBeenCalledWith("text", "Complete message", undefined, false)
		})
	})
})
