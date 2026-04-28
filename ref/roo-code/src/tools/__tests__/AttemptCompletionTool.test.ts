/**
 * AttemptCompletionTool 测试用例
 *
 * 验证 attempt_completion 工具在以下场景下的行为：
 * 1. 主任务完成，用户确认
 * 2. 主任务完成，用户提供反馈
 * 3. 子任务完成，用户确认委托
 * 4. 子任务完成，用户拒绝委托
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { attemptCompletionTool, type AttemptCompletionCallbacks } from "../AttemptCompletionTool"
import { CoderEventName } from "@coder/types"
import { formatResponse } from "../../prompts/responses"

describe("AttemptCompletionTool", () => {
	let mockTask: any
	let mockCallbacks: AttemptCompletionCallbacks

	beforeEach(() => {
		// Mock Task 实例
		mockTask = {
			taskId: "test-task-id",
			parentTaskId: undefined,
			didToolFailInCurrentTurn: false,
			todoList: [],
			clineMessages: [],
			abort: false,
			say: vi.fn().mockResolvedValue(undefined),
			emit: vi.fn(),
			metricsService: {
				recordToolError: vi.fn(),
				getTokenUsage: vi.fn().mockReturnValue({}),
				getToolUsage: vi.fn().mockReturnValue({}),
			},
			ask: vi.fn(),
			providerRef: {
				deref: vi.fn(),
			},
		}

		// Mock callbacks
		mockCallbacks = {
			askApproval: vi.fn().mockResolvedValue({ response: "yesButtonClicked" }),
			toolDescription: vi.fn().mockReturnValue("[attempt_completion]"),
			pushToolResult: vi.fn(),
			handleError: vi.fn().mockResolvedValue(undefined),
			askFinishSubTaskApproval: vi.fn(),
		}
	})

	afterEach(() => {
		vi.clearAllMocks()
	})

	describe("场景1: 主任务完成，用户确认", () => {
		it("应该设置 abort 标志并返回", async () => {
			// Arrange
			mockTask.ask.mockResolvedValue({ response: "yesButtonClicked" })

			// Act
			await attemptCompletionTool.execute(
				{ result: "任务完成" },
				mockTask as any,
				mockCallbacks as any,
			)

			// Assert
			expect(mockTask.abort).toBe(true)
			expect(mockTask.say).toHaveBeenCalledWith("completion_result", "任务完成")
			expect(mockTask.emit).toHaveBeenCalledWith(
				CoderEventName.TaskCompleted,
				mockTask.taskId,
				{},
				{},
			)
			expect(mockCallbacks.pushToolResult).not.toHaveBeenCalled()
		})
	})

	describe("场景2: 主任务完成，用户提供反馈", () => {
		it("应该将用户反馈添加到 userMessageContent 并继续执行", async () => {
			// Arrange
			mockTask.ask.mockResolvedValue({
				response: "messageResponse",
				text: "请添加更多注释",
				images: [],
			})

			// Act
			await attemptCompletionTool.execute(
				{ result: "任务完成" },
				mockTask as any,
				mockCallbacks as any,
			)

			// Assert
			expect(mockTask.abort).toBe(false)
			expect(mockTask.say).toHaveBeenCalledWith("completion_result", "任务完成")
			expect(mockTask.say).toHaveBeenCalledWith("user_feedback", "请添加更多注释", [])
			expect(mockCallbacks.pushToolResult).toHaveBeenCalledWith(
				formatResponse.toolResult("<user_message>\n请添加更多注释\n</user_message>", []),
			)
		})
	})

	describe("场景3: 子任务完成，用户确认委托", () => {
		it("应该委托给父任务而不设置 abort 标志", async () => {
			// Arrange
			mockTask.parentTaskId = "parent-task-id"
			mockCallbacks.askFinishSubTaskApproval = vi.fn().mockResolvedValue(true)

			const mockProvider = {
				getTaskWithId: vi.fn().mockResolvedValue({
					historyItem: { status: "active" },
				}),
				reopenParentFromDelegation: vi.fn().mockResolvedValue(undefined),
			}
			mockTask.providerRef.deref.mockReturnValue(mockProvider)

			// Act
			await attemptCompletionTool.execute(
				{ result: "子任务完成" },
				mockTask as any,
				mockCallbacks as any,
			)

			// Assert
			expect(mockTask.abort).toBe(false)
			expect(mockCallbacks.askFinishSubTaskApproval).toHaveBeenCalled()
			expect(mockCallbacks.pushToolResult).toHaveBeenCalledWith("")
			expect(mockProvider.reopenParentFromDelegation).toHaveBeenCalledWith({
				parentTaskId: "parent-task-id",
				childTaskId: "test-task-id",
				completionResultSummary: "子任务完成",
			})
		})

		it("如果子任务已完成，应该跳过委托", async () => {
			// Arrange
			mockTask.parentTaskId = "parent-task-id"
			mockTask.ask.mockResolvedValue({ response: "yesButtonClicked" })

			const mockProvider = {
				getTaskWithId: vi.fn().mockResolvedValue({
					historyItem: { status: "completed" },
				}),
				reopenParentFromDelegation: vi.fn().mockResolvedValue(undefined),
			}
			mockTask.providerRef.deref.mockReturnValue(mockProvider)

			// Act
			await attemptCompletionTool.execute(
				{ result: "子任务完成" },
				mockTask as any,
				mockCallbacks as any,
			)

			// Assert
			expect(mockTask.abort).toBe(true)
			expect(mockCallbacks.askFinishSubTaskApproval).not.toHaveBeenCalled()
			expect(mockProvider.reopenParentFromDelegation).not.toHaveBeenCalled()
		})
	})

	describe("场景4: 子任务完成，用户拒绝委托", () => {
		it("应该拒绝委托而不设置 abort 标志", async () => {
			// Arrange
			mockTask.parentTaskId = "parent-task-id"
			mockCallbacks.askFinishSubTaskApproval = vi.fn().mockResolvedValue(false)

			const mockProvider = {
				getTaskWithId: vi.fn().mockResolvedValue({
					historyItem: { status: "active" },
				}),
				reopenParentFromDelegation: vi.fn().mockResolvedValue(undefined),
			}
			mockTask.providerRef.deref.mockReturnValue(mockProvider)

			// Act
			await attemptCompletionTool.execute(
				{ result: "子任务完成" },
				mockTask as any,
				mockCallbacks as any,
			)

			// Assert
			expect(mockTask.abort).toBe(false)
			expect(mockCallbacks.askFinishSubTaskApproval).toHaveBeenCalled()
			expect(mockCallbacks.pushToolResult).toHaveBeenCalledWith(formatResponse.toolDenied())
			expect(mockProvider.reopenParentFromDelegation).not.toHaveBeenCalled()
		})
	})

	describe("边界情况", () => {
		it("如果有工具失败，应该拒绝完成", async () => {
			// Arrange
			mockTask.didToolFailInCurrentTurn = true

			// Act
			await attemptCompletionTool.execute(
				{ result: "任务完成" },
				mockTask as any,
				mockCallbacks as any,
			)

			// Assert
			expect(mockTask.say).toHaveBeenCalledWith("error", expect.any(String))
			expect(mockCallbacks.pushToolResult).toHaveBeenCalledWith(
				formatResponse.toolError(expect.any(String)),
			)
			expect(mockTask.abort).toBe(false)
		})

		it("如果有未完成的 todos 且启用了预防选项，应该拒绝完成", async () => {
			// Arrange
			mockTask.todoList = [
				{ status: "completed" },
				{ status: "in_progress" },
			]
			vi.spyOn(require("vscode").workspace, "getConfiguration").mockReturnValue({
				get: vi.fn().mockReturnValue(true),
			})

			// Act
			await attemptCompletionTool.execute(
				{ result: "任务完成" },
				mockTask as any,
				mockCallbacks as any,
			)

			// Assert
			expect(mockCallbacks.pushToolResult).toHaveBeenCalledWith(
				formatResponse.toolError(expect.any(String)),
			)
			expect(mockTask.abort).toBe(false)
		})

		it("如果缺少 result 参数，应该报错", async () => {
			// Act
			await attemptCompletionTool.execute(
				{ result: "" },
				mockTask as any,
				mockCallbacks as any,
			)

			// Assert
			expect(mockCallbacks.pushToolResult).toHaveBeenCalledWith(
				expect.any(String),
			)
			expect(mockTask.abort).toBe(false)
		})

		it("如果获取历史记录失败，应该跳过委托", async () => {
			// Arrange
			mockTask.parentTaskId = "parent-task-id"
			mockTask.ask.mockResolvedValue({ response: "yesButtonClicked" })

			const mockProvider = {
				getTaskWithId: vi.fn().mockRejectedValue(new Error("Failed to get history")),
				reopenParentFromDelegation: vi.fn().mockResolvedValue(undefined),
			}
			mockTask.providerRef.deref.mockReturnValue(mockProvider)

			// Act
			await attemptCompletionTool.execute(
				{ result: "子任务完成" },
				mockTask as any,
				mockCallbacks as any,
			)

			// Assert
			expect(mockTask.abort).toBe(true)
			expect(mockCallbacks.askFinishSubTaskApproval).not.toHaveBeenCalled()
			expect(mockProvider.reopenParentFromDelegation).not.toHaveBeenCalled()
		})
	})
})
