// Test for tool approval blocking issue
// This test verifies that tool approval asks work correctly after streaming fix

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

import { toolExecutorRegistry } from "../../tools"

describe("presentAssistantMessage - Tool Approval Blocking Issue", () => {
	let mockTask: any
	let askCallCount = 0
	let askResolvers: Array<(value: any) => void> = []

	beforeEach(() => {
		vi.clearAllMocks()
		askCallCount = 0
		askResolvers = []

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
			// Mock ask that simulates user approval
			ask: vi.fn().mockImplementation(async (type: string, text?: string) => {
				askCallCount++
				console.log(`[Test] ask called: type=${type}, call #${askCallCount}`)
				
				// Simulate user approval (yes button clicked)
				return new Promise((resolve) => {
					askResolvers.push(resolve)
				})
			}),
		}

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

	describe("Tool approval should work correctly", () => {
		it("should handle execute_command tool approval", async () => {
			const toolCallId = "tool_call_123"
			mockTask.assistantMessageContent = [
				{
					type: "tool_use",
					id: toolCallId,
					name: "execute_command",
					params: { command: "ls -la" },
					partial: false,
					nativeArgs: { command: "ls -la" },
				},
			]

			mockTask.didCompleteReadingStream = true

			const mockExecutor = {
				name: "execute_command" as const,
				handle: vi.fn().mockImplementation(async (cline: any, toolUse: any, callbacks: any) => {
					console.log("[Test] execute_command handle called")
					// Simulate asking for approval
					const approved = await callbacks.askApproval("command", JSON.stringify({ command: "ls -la" }))
					console.log("[Test] Approval result:", approved)
					if (approved) {
						callbacks.pushToolResult("Command executed successfully")
					}
				}),
			}

			vi.mocked(toolExecutorRegistry.get).mockReturnValue(mockExecutor as any)

			// Start processing
			const processPromise = presentAssistantMessage(mockTask)

			// Wait a bit for ask to be called
			await new Promise(resolve => setTimeout(resolve, 50))

			// Verify ask was called
			expect(askCallCount).toBe(1)
			console.log("[Test] Ask was called, now resolving with approval")

			// Simulate user approval
			if (askResolvers.length > 0) {
				askResolvers[0]({ response: "yesButtonClicked", text: undefined, images: undefined })
			}

			// Wait for processing to complete
			await processPromise

			// Verify tool was executed
			expect(mockExecutor.handle).toHaveBeenCalled()
		})

		it("should handle ask_followup_question tool", async () => {
			const toolCallId = "tool_call_456"
			mockTask.assistantMessageContent = [
				{
					type: "tool_use",
					id: toolCallId,
					name: "ask_followup_question",
					params: { question: "What would you like to do?" },
					partial: false,
					nativeArgs: { question: "What would you like to do?" },
				},
			]

			mockTask.didCompleteReadingStream = true

			const mockExecutor = {
				name: "ask_followup_question" as const,
				handle: vi.fn().mockImplementation(async (cline: any, toolUse: any, callbacks: any) => {
					console.log("[Test] ask_followup_question handle called")
					// Simulate asking question
					const result = await callbacks.askApproval("followup", "What would you like to do?")
					console.log("[Test] Question result:", result)
					if (result) {
						callbacks.pushToolResult("User responded")
					}
				}),
			}

			vi.mocked(toolExecutorRegistry.get).mockReturnValue(mockExecutor as any)

			// Start processing
			const processPromise = presentAssistantMessage(mockTask)

			// Wait a bit for ask to be called
			await new Promise(resolve => setTimeout(resolve, 50))

			// Verify ask was called
			expect(askCallCount).toBe(1)
			console.log("[Test] Ask was called, now resolving with user response")

			// Simulate user response
			if (askResolvers.length > 0) {
				askResolvers[0]({ response: "messageResponse", text: "I want to continue", images: undefined })
			}

			// Wait for processing to complete
			await processPromise

			// Verify tool was executed
			expect(mockExecutor.handle).toHaveBeenCalled()
		})

		it("CRITICAL: should not block when ask is called during streaming", async () => {
			// This test simulates the real scenario:
			// 1. Text streaming is happening (partial text blocks)
			// 2. Then a tool call arrives that needs approval
			// 3. The approval should work correctly

			// First, process some partial text
			mockTask.assistantMessageContent = [
				{ type: "text", content: "Hello", partial: true },
			]

			await presentAssistantMessage(mockTask)
			expect(mockTask.say).toHaveBeenCalledWith("text", "Hello", undefined, true)

			// Now a tool call arrives
			mockTask.assistantMessageContent = [
				{ type: "text", content: "Hello", partial: false },
				{
					type: "tool_use",
					id: "tool_call_789",
					name: "execute_command",
					params: { command: "echo test" },
					partial: false,
					nativeArgs: { command: "echo test" },
				},
			]
			mockTask.currentStreamingContentIndex = 1
			mockTask.didCompleteReadingStream = true

			const mockExecutor = {
				name: "execute_command" as const,
				handle: vi.fn().mockImplementation(async (cline: any, toolUse: any, callbacks: any) => {
					console.log("[Test] Tool handle called after streaming")
					const approved = await callbacks.askApproval("command", JSON.stringify({ command: "echo test" }))
					if (approved) {
						callbacks.pushToolResult("Success")
					}
				}),
			}

			vi.mocked(toolExecutorRegistry.get).mockReturnValue(mockExecutor as any)

			// Start processing tool
			const processPromise = presentAssistantMessage(mockTask)

			// Wait for ask
			await new Promise(resolve => setTimeout(resolve, 50))
			expect(askCallCount).toBe(1)

			// Approve
			if (askResolvers.length > 0) {
				askResolvers[0]({ response: "yesButtonClicked", text: undefined, images: undefined })
			}

			await processPromise

			expect(mockExecutor.handle).toHaveBeenCalled()
		})
	})
})
