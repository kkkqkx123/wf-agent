import { Task } from "../task/Task"
import { formatResponse } from "../prompts/responses"
import { BaseTool, ToolCallbacks } from "./core/BaseTool"
import type { ToolUse } from "../../shared/tools"
import cloneDeep from "clone-deep"
import crypto from "crypto"
import { TodoItem, TodoStatus, todoStatusSchema } from "@coder/types"
import { getLatestTodo } from "../../shared/todo"

interface UpdateTodoListParams {
	todos: string
}

let approvedTodoList: TodoItem[] | undefined = undefined

export class UpdateTodoListTool extends BaseTool<"update_todo_list"> {
	readonly name = "update_todo_list" as const

	async execute(params: UpdateTodoListParams, task: Task, callbacks: ToolCallbacks): Promise<void> {
		const { pushToolResult, handleError, askApproval } = callbacks

		try {
			const todosRaw = params.todos

			let todos: TodoItem[]
			try {
				todos = parseMarkdownChecklist(todosRaw || "")
			} catch {
				task.recordToolError("update_todo_list")
				task.didToolFailInCurrentTurn = true
				pushToolResult(formatResponse.toolError("The todos parameter is not valid markdown checklist or JSON"))
				return
			}

			const { valid, error } = validateTodos(todos, todosRaw)
			if (!valid) {
				task.recordToolError("update_todo_list")
				task.didToolFailInCurrentTurn = true
				pushToolResult(formatResponse.toolError(error || "todos parameter validation failed"))
				return
			}

			let normalizedTodos: TodoItem[] = todos.map((t) => ({
				id: t.id,
				content: t.content,
				status: normalizeStatus(t.status),
			}))

			const approvalMsg = JSON.stringify({
				tool: "updateTodoList",
				todos: normalizedTodos,
			})

			approvedTodoList = cloneDeep(normalizedTodos)
			const didApprove = await askApproval("tool", approvalMsg)
			if (!didApprove) {
				pushToolResult("User declined to update the todoList.")
				return
			}

			const isTodoListChanged =
				approvedTodoList !== undefined && JSON.stringify(normalizedTodos) !== JSON.stringify(approvedTodoList)
			if (isTodoListChanged) {
				normalizedTodos = approvedTodoList ?? []
				task.say(
					"user_edit_todos",
					JSON.stringify({
						tool: "updateTodoList",
						todos: normalizedTodos,
					}),
				)
			}

			await setTodoListForTask(task, normalizedTodos)

			if (isTodoListChanged) {
				const md = todoListToMarkdown(normalizedTodos)
				pushToolResult(formatResponse.toolResult("User edits todo:\n\n" + md))
			} else {
				pushToolResult(formatResponse.toolResult("Todo list updated successfully."))
			}
		} catch (error) {
			await handleError("update todo list", error as Error)
		}
	}

	override async handlePartial(task: Task, block: ToolUse<"update_todo_list">): Promise<void> {
		const todosRaw = block.params.todos

		// Parse the markdown checklist to maintain consistent format with execute()
		let todos: TodoItem[]
		try {
			todos = parseMarkdownChecklist(todosRaw || "")
		} catch {
			// If parsing fails during partial, send empty array
			todos = []
		}

		const approvalMsg = JSON.stringify({
			tool: "updateTodoList",
			todos: todos,
		})
		await task.ask("tool", approvalMsg, block.partial).catch(() => { })
	}
}

export function addTodoToTask(cline: Task, content: string, status: TodoStatus = "pending", id?: string): TodoItem {
	const todo: TodoItem = {
		id: id ?? crypto.randomUUID(),
		content,
		status,
	}
	if (!cline.todoList) cline.todoList = []
	cline.todoList.push(todo)
	return todo
}

export function updateTodoStatusForTask(cline: Task, id: string, nextStatus: TodoStatus): boolean {
	if (!cline.todoList) return false
	const idx = cline.todoList.findIndex((t) => t.id === id)
	if (idx === -1) return false
	const current = cline.todoList[idx]
	if (!current) return false
	
	if (
		(current.status === "pending" && nextStatus === "in_progress") ||
		(current.status === "in_progress" && nextStatus === "completed") ||
		current.status === nextStatus
	) {
		cline.todoList[idx] = { ...current, status: nextStatus }
		return true
	}
	return false
}

export function removeTodoFromTask(cline: Task, id: string): boolean {
	if (!cline.todoList) return false
	const idx = cline.todoList.findIndex((t) => t.id === id)
	if (idx === -1) return false
	cline.todoList.splice(idx, 1)
	return true
}

export function getTodoListForTask(cline: Task): TodoItem[] | undefined {
	return cline.todoList?.slice()
}

export async function setTodoListForTask(cline?: Task, todos?: TodoItem[]) {
	if (cline === undefined) return
	cline.todoList = Array.isArray(todos) ? todos : []
}

export function restoreTodoListForTask(cline: Task, todoList?: TodoItem[]) {
	if (todoList) {
		cline.todoList = Array.isArray(todoList) ? todoList : []
		return
	}
	cline.todoList = getLatestTodo(cline.clineMessages)
}

function todoListToMarkdown(todos: TodoItem[]): string {
	return todos
		.map((t) => {
			let box = "[ ]"
			if (t.status === "completed") box = "[x]"
			else if (t.status === "in_progress") box = "[-]"
			return `${box} ${t.content}`
		})
		.join("\n")
}

function normalizeStatus(status: string | undefined): TodoStatus {
	if (status === "completed") return "completed"
	if (status === "in_progress") return "in_progress"
	return "pending"
}

export function parseMarkdownChecklist(md: string): TodoItem[] {
	if (typeof md !== "string") return []

	// Handle double-quoted strings (LLM sometimes wraps markdown in quotes)
	// e.g., "\"[ ] Task 1\n[ ] Task 2\"" -> "[ ] Task 1\n[ ] Task 2"
	if (md.startsWith('"') && md.endsWith('"')) {
		try {
			const unquoted = JSON.parse(md)
			if (typeof unquoted === "string") {
				md = unquoted
			}
		} catch {
			// If JSON parse fails, try stripping quotes manually
			md = md.slice(1, -1)
		}
	}

	// Convert escaped newlines to actual newlines (handle \\n, \n, etc.)
	md = md.replace(/\\n/g, "\n")

	// Try to parse as JSON array format first (LLMs sometimes use this format)
	// Format: [{"id": "1", "content": "Task 1", "status": "completed"}, ...]
	if (md.trim().startsWith("[")) {
		try {
			const parsed = JSON.parse(md)
			if (Array.isArray(parsed)) {
				const todos: TodoItem[] = []
				for (const item of parsed) {
					if (item && typeof item === "object" && "content" in item) {
						const status = normalizeStatus(item.status as string | undefined)
						todos.push({
							id: item.id || crypto.randomUUID(),
							content: String(item.content),
							status,
						})
					}
				}
				if (todos.length > 0) {
					return todos
				}
			}
		} catch {
			// Not valid JSON, fall through to markdown parsing
		}
	}

	// Parse markdown checklist format
	// Format:
	// [ ] Task 1
	// [x] Task 2 (completed)
	// [-] Task 3 (in progress)
	const lines = md
		.split(/\r?\n/)
		.map((l) => l.trim())
		.filter(Boolean)
	const todos: TodoItem[] = []
	for (const line of lines) {
		const match = line.match(/^(?:-\s*)?\[\s*([ xX\-~])\s*\]\s+(.+)$/)
		if (!match) continue
		let status: TodoStatus = "pending"
		if (match[1] === "x" || match[1] === "X") status = "completed"
		else if (match[1] === "-" || match[1] === "~") status = "in_progress"
		const content = match[2] || ""
		const id = crypto
			.createHash("md5")
			.update(content + status)
			.digest("hex")
		todos.push({
			id,
			content,
			status,
		})
	}
	return todos
}

export function setPendingTodoList(todos: TodoItem[]) {
	approvedTodoList = todos
}

export function validateTodos(todos: any[], rawInput?: string): { valid: boolean; error?: string } {
	if (!Array.isArray(todos)) return { valid: false, error: "todos must be an array" }

	// Check if the input was non-empty but parsing resulted in empty array
	// This indicates a format error rather than intentional clearing
	if (todos.length === 0 && rawInput && rawInput.trim().length > 0) {
		// Check if the input was intentionally empty (empty string or empty JSON array)
		const trimmedInput = rawInput.trim()
		if (trimmedInput !== "" && trimmedInput !== "[]") {
			return {
				valid: false,
				error: `The todos parameter could not be parsed. Expected markdown checklist format (e.g., "[ ] Task 1") or JSON array format. Received: "${trimmedInput.substring(0, 100)}${trimmedInput.length > 100 ? "..." : ""}"`
			}
		}
	}

	

	for (const [i, t] of todos.entries()) {
		if (!t || typeof t !== "object") return { valid: false, error: `Item ${i + 1} is not an object` }
		if (!t.id || typeof t.id !== "string") return { valid: false, error: `Item ${i + 1} is missing id` }
		if (!t.content || typeof t.content !== "string")
			return { valid: false, error: `Item ${i + 1} is missing content` }
		if (t.status && !todoStatusSchema.options.includes(t.status as TodoStatus))
			return { valid: false, error: `Item ${i + 1} has invalid status` }
	}
	return { valid: true }
}

export const updateTodoListTool = new UpdateTodoListTool()
