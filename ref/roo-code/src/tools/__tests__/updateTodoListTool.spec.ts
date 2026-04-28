import { describe, it, expect, beforeEach, vi } from "vitest"
import { parseMarkdownChecklist, validateTodos } from "../UpdateTodoListTool"
import { TodoItem } from "@coder/types"

describe("parseMarkdownChecklist", () => {
	describe("standard checkbox format (without dash prefix)", () => {
		it("should parse pending tasks", () => {
			const md = `[ ] Task 1
[ ] Task 2`
			const result = parseMarkdownChecklist(md)
			expect(result).toHaveLength(2)
			expect(result[0]!.content).toBe("Task 1")
			expect(result[0]!.status).toBe("pending")
			expect(result[1]!.content).toBe("Task 2")
			expect(result[1]!.status).toBe("pending")
		})

		it("should parse completed tasks with lowercase x", () => {
			const md = `[x] Completed task 1
[x] Completed task 2`
			const result = parseMarkdownChecklist(md)
			expect(result).toHaveLength(2)
			expect(result[0]!.content).toBe("Completed task 1")
			expect(result[0]!.status).toBe("completed")
			expect(result[1]!.content).toBe("Completed task 2")
			expect(result[1]!.status).toBe("completed")
		})

		it("should parse completed tasks with uppercase X", () => {
			const md = `[X] Completed task 1
[X] Completed task 2`
			const result = parseMarkdownChecklist(md)
			expect(result).toHaveLength(2)
			expect(result[0]!.content).toBe("Completed task 1")
			expect(result[0]!.status).toBe("completed")
			expect(result[1]!.content).toBe("Completed task 2")
			expect(result[1]!.status).toBe("completed")
		})

		it("should parse in-progress tasks with dash", () => {
			const md = `[-] In progress task 1
[-] In progress task 2`
			const result = parseMarkdownChecklist(md)
			expect(result).toHaveLength(2)
			expect(result[0]!.content).toBe("In progress task 1")
			expect(result[0]!.status).toBe("in_progress")
			expect(result[1]!.content).toBe("In progress task 2")
			expect(result[1]!.status).toBe("in_progress")
		})

		it("should parse in-progress tasks with tilde", () => {
			const md = `[~] In progress task 1
[~] In progress task 2`
			const result = parseMarkdownChecklist(md)
			expect(result).toHaveLength(2)
			expect(result[0]!.content).toBe("In progress task 1")
			expect(result[0]!.status).toBe("in_progress")
			expect(result[1]!.content).toBe("In progress task 2")
			expect(result[1]!.status).toBe("in_progress")
		})
	})

	describe("dash-prefixed checkbox format", () => {
		it("should parse pending tasks with dash prefix", () => {
			const md = `- [ ] Task 1
- [ ] Task 2`
			const result = parseMarkdownChecklist(md)
			expect(result).toHaveLength(2)
			expect(result[0]!.content).toBe("Task 1")
			expect(result[0]!.status).toBe("pending")
			expect(result[1]!.content).toBe("Task 2")
			expect(result[1]!.status).toBe("pending")
		})

		it("should parse completed tasks with dash prefix and lowercase x", () => {
			const md = `- [x] Completed task 1
- [x] Completed task 2`
			const result = parseMarkdownChecklist(md)
			expect(result).toHaveLength(2)
			expect(result[0]!.content).toBe("Completed task 1")
			expect(result[0]!.status).toBe("completed")
			expect(result[1]!.content).toBe("Completed task 2")
			expect(result[1]!.status).toBe("completed")
		})

		it("should parse completed tasks with dash prefix and uppercase X", () => {
			const md = `- [X] Completed task 1
- [X] Completed task 2`
			const result = parseMarkdownChecklist(md)
			expect(result).toHaveLength(2)
			expect(result[0]!.content).toBe("Completed task 1")
			expect(result[0]!.status).toBe("completed")
			expect(result[1]!.content).toBe("Completed task 2")
			expect(result[1]!.status).toBe("completed")
		})

		it("should parse in-progress tasks with dash prefix and dash marker", () => {
			const md = `- [-] In progress task 1
- [-] In progress task 2`
			const result = parseMarkdownChecklist(md)
			expect(result).toHaveLength(2)
			expect(result[0]!.content).toBe("In progress task 1")
			expect(result[0]!.status).toBe("in_progress")
			expect(result[1]!.content).toBe("In progress task 2")
			expect(result[1]!.status).toBe("in_progress")
		})

		it("should parse in-progress tasks with dash prefix and tilde marker", () => {
			const md = `- [~] In progress task 1
- [~] In progress task 2`
			const result = parseMarkdownChecklist(md)
			expect(result).toHaveLength(2)
			expect(result[0]!.content).toBe("In progress task 1")
			expect(result[0]!.status).toBe("in_progress")
			expect(result[1]!.content).toBe("In progress task 2")
			expect(result[1]!.status).toBe("in_progress")
		})
	})

	describe("mixed formats", () => {
		it("should parse mixed formats correctly", () => {
			const md = `[ ] Task without dash
- [ ] Task with dash
[x] Completed without dash
- [X] Completed with dash
[-] In progress without dash
- [~] In progress with dash`
			const result = parseMarkdownChecklist(md)
			expect(result).toHaveLength(6)

			expect(result[0]!.content).toBe("Task without dash")
			expect(result[0]!.status).toBe("pending")

			expect(result[1]!.content).toBe("Task with dash")
			expect(result[1]!.status).toBe("pending")

			expect(result[2]!.content).toBe("Completed without dash")
			expect(result[2]!.status).toBe("completed")

			expect(result[3]!.content).toBe("Completed with dash")
			expect(result[3]!.status).toBe("completed")

			expect(result[4]!.content).toBe("In progress without dash")
			expect(result[4]!.status).toBe("in_progress")

			expect(result[5]!.content).toBe("In progress with dash")
			expect(result[5]!.status).toBe("in_progress")
		})
	})

	describe("edge cases", () => {
		it("should handle empty strings", () => {
			const result = parseMarkdownChecklist("")
			expect(result).toEqual([])
		})

		it("should handle non-string input", () => {
			const result = parseMarkdownChecklist(null as any)
			expect(result).toEqual([])
		})

		it("should handle undefined input", () => {
			const result = parseMarkdownChecklist(undefined as any)
			expect(result).toEqual([])
		})

		it("should ignore non-checklist lines", () => {
			const md = `This is not a checklist
[ ] Valid task
Just some text
- Not a checklist item
- [x] Valid completed task
[not valid] Invalid format`
			const result = parseMarkdownChecklist(md)
			expect(result).toHaveLength(2)
			expect(result[0]!.content).toBe("Valid task")
			expect(result[0]!.status).toBe("pending")
			expect(result[1]!.content).toBe("Valid completed task")
			expect(result[1]!.status).toBe("completed")
		})

		it("should handle extra spaces", () => {
			const md = `  [ ]   Task with spaces  
-  [ ]  Task with dash and spaces
  [x]  Completed with spaces
-   [X]   Completed with dash and spaces`
			const result = parseMarkdownChecklist(md)
			expect(result).toHaveLength(4)
			expect(result[0]!.content).toBe("Task with spaces")
			expect(result[1]!.content).toBe("Task with dash and spaces")
			expect(result[2]!.content).toBe("Completed with spaces")
			expect(result[3]!.content).toBe("Completed with dash and spaces")
		})

		it("should handle Windows line endings", () => {
			const md = "[ ] Task 1\r\n- [x] Task 2\r\n[-] Task 3"
			const result = parseMarkdownChecklist(md)
			expect(result).toHaveLength(3)
			expect(result[0]!.content).toBe("Task 1")
			expect(result[0]!.status).toBe("pending")
			expect(result[1]!.content).toBe("Task 2")
			expect(result[1]!.status).toBe("completed")
			expect(result[2]!.content).toBe("Task 3")
			expect(result[2]!.status).toBe("in_progress")
		})
	})

	describe("ID generation", () => {
		it("should generate consistent IDs for the same content and status", () => {
			const md1 = `[ ] Task 1
[x] Task 2`
			const md2 = `[ ] Task 1
[x] Task 2`
			const result1 = parseMarkdownChecklist(md1)
			const result2 = parseMarkdownChecklist(md2)

			expect(result1[0]!.id).toBe(result2[0]!.id)
			expect(result1[1]!.id).toBe(result2[1]!.id)
		})

		it("should generate different IDs for different content", () => {
			const md = `[ ] Task 1
[ ] Task 2`
			const result = parseMarkdownChecklist(md)
			expect(result[0]!.id).not.toBe(result[1]!.id)
		})

		it("should generate different IDs for same content but different status", () => {
			const md = `[ ] Task 1
[x] Task 1`
			const result = parseMarkdownChecklist(md)
			expect(result[0]!.id).not.toBe(result[1]!.id)
		})

		it("should generate same IDs regardless of dash prefix", () => {
			const md1 = `[ ] Task 1`
			const md2 = `- [ ] Task 1`
			const result1 = parseMarkdownChecklist(md1)
			const result2 = parseMarkdownChecklist(md2)
			expect(result1[0]!.id).toBe(result2[0]!.id)
		})
	})

	describe("JSON array format (LLM compatibility)", () => {
		it("should parse JSON array format with all status types", () => {
			const json = JSON.stringify([
				{ id: "1", content: "Task 1", status: "pending" },
				{ id: "2", content: "Task 2", status: "completed" },
				{ id: "3", content: "Task 3", status: "in_progress" },
			])
			const result = parseMarkdownChecklist(json)
			expect(result).toHaveLength(3)
			expect(result[0]!.id).toBe("1")
			expect(result[0]!.content).toBe("Task 1")
			expect(result[0]!.status).toBe("pending")
			expect(result[1]!.id).toBe("2")
			expect(result[1]!.content).toBe("Task 2")
			expect(result[1]!.status).toBe("completed")
			expect(result[2]!.id).toBe("3")
			expect(result[2]!.content).toBe("Task 3")
			expect(result[2]!.status).toBe("in_progress")
		})

		it("should parse JSON array with missing IDs (generate UUID)", () => {
			const json = JSON.stringify([
				{ content: "Task without ID", status: "pending" },
			])
			const result = parseMarkdownChecklist(json)
			expect(result).toHaveLength(1)
			expect(result[0]!.content).toBe("Task without ID")
			expect(result[0]!.status).toBe("pending")
			expect(result[0]!.id).toBeDefined()
		})

		it("should parse JSON array with invalid status (normalize to pending)", () => {
			const json = JSON.stringify([
				{ id: "1", content: "Task 1", status: "invalid_status" },
				{ id: "2", content: "Task 2", status: undefined },
			])
			const result = parseMarkdownChecklist(json)
			expect(result).toHaveLength(2)
			expect(result[0]!.status).toBe("pending")
			expect(result[1]!.status).toBe("pending")
		})

		it("should handle empty JSON array", () => {
			const json = JSON.stringify([])
			const result = parseMarkdownChecklist(json)
			expect(result).toEqual([])
		})

		it("should fall back to markdown parsing for invalid JSON starting with [", () => {
			const invalidJson = "[ not valid json"
			const result = parseMarkdownChecklist(invalidJson)
			expect(result).toEqual([])
		})

		it("should handle JSON array with non-object items", () => {
			const json = JSON.stringify(["string", 123, null])
			const result = parseMarkdownChecklist(json)
			expect(result).toEqual([])
		})

		it("should handle JSON array with partial valid objects", () => {
			const json = JSON.stringify([
				{ content: "Valid task", status: "completed" },
				{ noContent: "Invalid" },
				{ id: "2", content: "Another valid task", status: "pending" },
			])
			const result = parseMarkdownChecklist(json)
			expect(result).toHaveLength(2)
			expect(result[0]!.content).toBe("Valid task")
			expect(result[1]!.content).toBe("Another valid task")
		})

		it("should handle the exact LLM failure case from user report", () => {
			const json = JSON.stringify([
				{ id: "1", content: "分析 StreamingProcessor.ts 和 types.ts 的类型错误", status: "completed" },
				{ id: "2", content: "修复 ChunkHandlerContext 类型不匹配问题", status: "in_progress" },
				{ id: "3", content: "修复 Boolean 类型调用错误", status: "pending" },
				{ id: "4", content: "修复 ModelInfo 导入问题", status: "pending" },
				{ id: "5", content: "运行类型检查验证修复", status: "pending" },
			])
			const result = parseMarkdownChecklist(json)
			expect(result).toHaveLength(5)
			expect(result[0]!.content).toBe("分析 StreamingProcessor.ts 和 types.ts 的类型错误")
			expect(result[0]!.status).toBe("completed")
			expect(result[1]!.content).toBe("修复 ChunkHandlerContext 类型不匹配问题")
			expect(result[1]!.status).toBe("in_progress")
			expect(result[2]!.content).toBe("修复 Boolean 类型调用错误")
			expect(result[2]!.status).toBe("pending")
		})
	})

	describe("double-quoted string handling (LLM bug workaround)", () => {
		it("should parse markdown wrapped in double quotes", () => {
			// This is the exact format from the user's bug report
			const doubleQuoted = '"[ ] Task 1\\n[ ] Task 2\\n[x] Task 3"'
			const result = parseMarkdownChecklist(doubleQuoted)
			expect(result).toHaveLength(3)
			expect(result[0]!.content).toBe("Task 1")
			expect(result[0]!.status).toBe("pending")
			expect(result[1]!.content).toBe("Task 2")
			expect(result[1]!.status).toBe("pending")
			expect(result[2]!.content).toBe("Task 3")
			expect(result[2]!.status).toBe("completed")
		})

		it("should handle double-quoted string with Chinese characters", () => {
			// Real-world case from user report
			const doubleQuoted = '"[ ] 查看 apply_patch 工具的使用说明\\n[ ] 在 temp 目录创建几个有内容的 txt 文件"'
			const result = parseMarkdownChecklist(doubleQuoted)
			expect(result).toHaveLength(2)
			expect(result[0]!.content).toBe("查看 apply_patch 工具的使用说明")
			expect(result[0]!.status).toBe("pending")
			expect(result[1]!.content).toBe("在 temp 目录创建几个有内容的 txt 文件")
			expect(result[1]!.status).toBe("pending")
		})

		it("should handle double-quoted string with in-progress status", () => {
			const doubleQuoted = '"[-] In progress task\\n[ ] Pending task"'
			const result = parseMarkdownChecklist(doubleQuoted)
			expect(result).toHaveLength(2)
			expect(result[0]!.content).toBe("In progress task")
			expect(result[0]!.status).toBe("in_progress")
			expect(result[1]!.content).toBe("Pending task")
			expect(result[1]!.status).toBe("pending")
		})

		it("should handle double-quoted string with dash prefix", () => {
			const doubleQuoted = '"- [ ] Task 1\\n- [x] Task 2"'
			const result = parseMarkdownChecklist(doubleQuoted)
			expect(result).toHaveLength(2)
			expect(result[0]!.content).toBe("Task 1")
			expect(result[0]!.status).toBe("pending")
			expect(result[1]!.content).toBe("Task 2")
			expect(result[1]!.status).toBe("completed")
		})

		it("should handle malformed double-quoted string (fallback to manual strip)", () => {
			// Invalid JSON but has quotes at start and end
			const malformed = '"[ ] Task 1\n[ ] Task 2"'
			const result = parseMarkdownChecklist(malformed)
			expect(result).toHaveLength(2)
			expect(result[0]!.content).toBe("Task 1")
			expect(result[1]!.content).toBe("Task 2")
		})

		it("should not affect normal markdown without quotes", () => {
			const normal = "[ ] Task 1\n[ ] Task 2"
			const result = parseMarkdownChecklist(normal)
			expect(result).toHaveLength(2)
		})
	})
})

describe("validateTodos", () => {
	describe("basic validation", () => {
		it("should validate a valid todo list", () => {
			const todos: TodoItem[] = [
				{ id: "1", content: "Task 1", status: "pending" },
				{ id: "2", content: "Task 2", status: "completed" },
			]
			const result = validateTodos(todos)
			expect(result.valid).toBe(true)
		})

		it("should reject non-array input", () => {
			const result = validateTodos("not an array" as any)
			expect(result.valid).toBe(false)
			expect(result.error).toBe("todos must be an array")
		})

		it("should reject item without id", () => {
			const todos = [{ content: "Task 1", status: "pending" }]
			const result = validateTodos(todos)
			expect(result.valid).toBe(false)
			expect(result.error).toBe("Item 1 is missing id")
		})

		it("should reject item without content", () => {
			const todos = [{ id: "1", status: "pending" }]
			const result = validateTodos(todos)
			expect(result.valid).toBe(false)
			expect(result.error).toBe("Item 1 is missing content")
		})

		it("should reject item with invalid status", () => {
			const todos = [{ id: "1", content: "Task 1", status: "invalid" }]
			const result = validateTodos(todos)
			expect(result.valid).toBe(false)
			expect(result.error).toBe("Item 1 has invalid status")
		})
	})

	describe("empty array validation with raw input", () => {
		it("should allow empty array when input is empty string", () => {
			const result = validateTodos([], "")
			expect(result.valid).toBe(true)
		})

		it("should allow empty array when input is whitespace only", () => {
			const result = validateTodos([], "   ")
			expect(result.valid).toBe(true)
		})

		it("should allow empty array when input is empty JSON array", () => {
			const result = validateTodos([], "[]")
			expect(result.valid).toBe(true)
		})

		it("should reject empty array when input is non-empty but unparseable", () => {
			// This is the key bug case: LLM sent data but parsing failed
			const result = validateTodos([], "some random text that is not a todo")
			expect(result.valid).toBe(false)
			expect(result.error).toContain("could not be parsed")
		})

		it("should reject empty array when input is double-quoted but still unparseable", () => {
			// Double-quoted string that doesn't contain valid markdown
			const result = validateTodos([], '"not a valid todo format"')
			expect(result.valid).toBe(false)
			expect(result.error).toContain("could not be parsed")
		})

		it("should include the raw input in error message (truncated)", () => {
			const longInput = "a".repeat(200)
			const result = validateTodos([], longInput)
			expect(result.valid).toBe(false)
			expect(result.error).toContain("a".repeat(100))
			expect(result.error).toContain("...")
		})

		it("should not truncate short input in error message", () => {
			const shortInput = "short input"
			const result = validateTodos([], shortInput)
			expect(result.valid).toBe(false)
			expect(result.error).toContain(shortInput)
			expect(result.error).not.toContain("...")
		})
	})

	describe("integration with parseMarkdownChecklist", () => {
		it("should validate successfully after parsing valid markdown", () => {
			const raw = "[ ] Task 1\n[x] Task 2"
			const todos = parseMarkdownChecklist(raw)
			const result = validateTodos(todos, raw)
			expect(result.valid).toBe(true)
		})

		it("should validate successfully after parsing double-quoted markdown", () => {
			const raw = '"[ ] Task 1\\n[x] Task 2"'
			const todos = parseMarkdownChecklist(raw)
			const result = validateTodos(todos, raw)
			expect(result.valid).toBe(true)
		})

		it("should fail validation when parsing invalid format returns empty array", () => {
			// This simulates the original bug: LLM sent data but it couldn't be parsed
			const raw = '"this is not a valid todo format"'
			const todos = parseMarkdownChecklist(raw)
			// After the fix, parseMarkdownChecklist should handle double-quoted strings
			// But if the content inside is still invalid, it should return empty array
			const result = validateTodos(todos, raw)
			// The raw input is double-quoted, so parseMarkdownChecklist will strip quotes
			// and try to parse "this is not a valid todo format" as markdown
			// Since it doesn't match the checklist pattern, it returns empty array
			expect(todos).toHaveLength(0)
			expect(result.valid).toBe(false)
			expect(result.error).toContain("could not be parsed")
		})

		it("should handle the exact user-reported bug case", () => {
			// The exact format from the user's bug report
			const raw = '"[ ] 查看 apply_patch 工具的使用说明\\n[ ] 在 temp 目录创建几个有内容的 txt 文件\\n[ ] 尝试使用 apply_patch 编辑文件\\n[ ] 尝试使用 apply_patch 移动/重命名文件\\n[ ] 尝试其他操作（删除、新建等）\\n[ ] 整理工具逻辑与预期不符的地方\\n[ ] 将结果写入 docs 目录的 md 文件"'
			const todos = parseMarkdownChecklist(raw)
			const result = validateTodos(todos, raw)
			expect(result.valid).toBe(true)
			expect(todos).toHaveLength(7)
		})
	})
})
