// npx vitest utils/__tests__/streaming-token-counter.spec.ts

import { describe, it, expect, beforeEach } from "vitest"
import { StreamingTokenCounter, tiktoken, TokenizerManager } from "../tiktoken"
import { Anthropic } from "@anthropic-ai/sdk"

describe("StreamingTokenCounter", () => {
	let counter: StreamingTokenCounter

	beforeEach(() => {
		counter = new StreamingTokenCounter()
	})

	describe("addText", () => {
		it("should return 0 for empty text", () => {
			const result = counter.addText("")
			expect(result).toBe(0)
			expect(counter.getTotalTokens()).toBe(0)
		})

		it("should count tokens for a single text addition", () => {
			const text = "Hello, world!"
			const result = counter.addText(text)
			expect(result).toBeGreaterThan(0)
			// getTotalTokens() now returns exact count without fudge factor
			expect(counter.getTotalTokens()).toBe(result)
		})
	
		it("should incrementally count tokens for multiple additions", () => {
			const text1 = "Hello"
			const text2 = ", "
			const text3 = "world!"
	
			const tokens1 = counter.addText(text1)
			const tokens2 = counter.addText(text2)
			const tokens3 = counter.addText(text3)
	
			expect(tokens1).toBeGreaterThan(0)
			expect(tokens2).toBeGreaterThan(0)
			expect(tokens3).toBeGreaterThan(0)
	
			// getTotalTokens() returns exact sum without fudge factor
			const total = counter.getTotalTokens()
			expect(total).toBe(tokens1 + tokens2 + tokens3)
		})
	})

	describe("addReasoning", () => {
		it("should return 0 for empty reasoning text", () => {
			const result = counter.addReasoning("")
			expect(result).toBe(0)
			expect(counter.getTotalTokens()).toBe(0)
		})

		it("should count tokens for reasoning content", () => {
			const text = "Let me think about this..."
			const result = counter.addReasoning(text)
			expect(result).toBeGreaterThan(0)
			// getTotalTokens() returns exact count without fudge factor
			expect(counter.getTotalTokens()).toBe(result)
		})

		it("should accumulate reasoning tokens separately from text", () => {
			counter.addText("Hello")
			const textTokens = counter.getTotalTokens()

			counter.addReasoning("Thinking...")
			const totalTokens = counter.getTotalTokens()

			expect(totalTokens).toBeGreaterThan(textTokens)
		})
	})

	describe("addToolCall", () => {
		it("should count tokens for tool calls", () => {
			const result = counter.addToolCall("tool_1", "read_file", '{"path": "test.txt"}')
			expect(result).toBeGreaterThan(0)
			expect(counter.getTotalTokens()).toBeGreaterThan(0)
		})

		it("should return 0 for empty tool ID or name", () => {
			const result1 = counter.addToolCall("", "read_file", '{"path": "test.txt"}')
			expect(result1).toBe(0)

			const result2 = counter.addToolCall("tool_1", "", '{"path": "test.txt"}')
			expect(result2).toBe(0)

			expect(counter.getTotalTokens()).toBe(0)
		})

		it("should update tokens when tool args are streamed", () => {
			const toolId = "tool_123"

			// Initial tool call with empty args
			const initialTokens = counter.addToolCall(toolId, "write_file", "")
			expect(initialTokens).toBeGreaterThan(0)

			// Update with partial args
			const partialTokens = counter.addToolCall(toolId, "write_file", '{"path":')
			expect(partialTokens).not.toBe(0)

			// Update with complete args
			const finalTokens = counter.addToolCall(toolId, "write_file", '{"path": "test.txt", "content": "hello"}')
			expect(finalTokens).not.toBe(0)

			// Total should reflect the final state
			const breakdown = counter.getTokenBreakdown()
			expect(breakdown.toolCalls).toBeGreaterThan(0)
		})

		it("should handle multiple calls to the same tool with different IDs", () => {
			// First call to read_file
			const tokens1 = counter.addToolCall("tool_1", "read_file", '{"path": "a.txt"}')
			expect(tokens1).toBeGreaterThan(0)

			// Second call to read_file (different ID, should be tracked separately)
			const tokens2 = counter.addToolCall("tool_2", "read_file", '{"path": "b.txt"}')
			expect(tokens2).toBeGreaterThan(0)

			// Both calls should be counted
			const breakdown = counter.getTokenBreakdown()
			expect(breakdown.toolCalls).toBeGreaterThan(tokens1)
			expect(breakdown.total).toBeGreaterThanOrEqual(tokens1 + tokens2)
		})

		it("should handle multiple different tool calls", () => {
			const tokens1 = counter.addToolCall("tool_1", "read_file", '{"path": "a.txt"}')
			const tokens2 = counter.addToolCall("tool_2", "write_file", '{"path": "b.txt"}')

			const breakdown = counter.getTokenBreakdown()
			expect(breakdown.toolCalls).toBeGreaterThan(0)
			expect(breakdown.total).toBeGreaterThan(0)
		})
	})

	describe("getTokenBreakdown", () => {
		it("should return correct breakdown for mixed content", () => {
			counter.addText("Hello world")
			counter.addReasoning("Let me think...")
			counter.addToolCall("tool_1", "read_file", '{"path": "test.txt"}')

			const breakdown = counter.getTokenBreakdown()

			expect(breakdown.text).toBeGreaterThan(0)
			expect(breakdown.reasoning).toBeGreaterThan(0)
			expect(breakdown.toolCalls).toBeGreaterThan(0)
			// breakdown.total now equals the sum of raw values (no fudge factor)
			const rawSum = breakdown.text + breakdown.reasoning + breakdown.toolCalls
			expect(breakdown.total).toBe(rawSum)
		})

		it("should return zeros for empty counter", () => {
			const breakdown = counter.getTokenBreakdown()

			expect(breakdown.text).toBe(0)
			expect(breakdown.reasoning).toBe(0)
			expect(breakdown.toolCalls).toBe(0)
			expect(breakdown.total).toBe(0)
		})
	})

	describe("reset", () => {
		it("should reset all counters", () => {
			counter.addText("Hello")
			counter.addReasoning("Thinking...")
			counter.addToolCall("tool_1", "read_file", '{"path": "test.txt"}')

			expect(counter.getTotalTokens()).toBeGreaterThan(0)

			counter.reset()

			const breakdown = counter.getTokenBreakdown()
			expect(breakdown.text).toBe(0)
			expect(breakdown.reasoning).toBe(0)
			expect(breakdown.toolCalls).toBe(0)
			expect(breakdown.total).toBe(0)
		})

		it("should allow reuse after reset", () => {
			counter.addText("First text")
			counter.reset()

			counter.addText("Second text")
			const breakdown = counter.getTokenBreakdown()

			expect(breakdown.text).toBeGreaterThan(0)
			expect(breakdown.reasoning).toBe(0)
			expect(breakdown.toolCalls).toBe(0)
		})
	})

	describe("integration", () => {
		it("should simulate a realistic streaming conversation", () => {
			// Simulate streaming text response
			const textChunks = ["Hello", " ", "there", "! ", "How ", "can ", "I ", "help?"]
			textChunks.forEach((chunk) => counter.addText(chunk))

			// Simulate reasoning
			counter.addReasoning("The user is asking for help...")

			// Simulate tool call streaming with proper tool ID
			const toolId = "call_search_001"
			counter.addToolCall(toolId, "search", "") // Start
			counter.addToolCall(toolId, "search", '{"query":') // Partial
			counter.addToolCall(toolId, "search", '{"query": "test"}') // Complete

			const breakdown = counter.getTokenBreakdown()

			// All categories should have tokens
			expect(breakdown.text).toBeGreaterThan(0)
			expect(breakdown.reasoning).toBeGreaterThan(0)
			expect(breakdown.toolCalls).toBeGreaterThan(0)
			expect(breakdown.total).toBeGreaterThan(0)
		})

		it("should handle multiple sequential tool calls with different IDs", () => {
			// Simulate a more complex conversation with multiple tool invocations
			counter.addText("Let me search for information...")

			// First tool call
			const toolId1 = "call_search_001"
			counter.addToolCall(toolId1, "search", '{"query": "example"}')

			// Second tool call
			const toolId2 = "call_read_001"
			counter.addToolCall(toolId2, "read_file", '{"path": "/test.txt"}')

			// Third tool call to the same tool as the first, but different ID
			const toolId3 = "call_search_002"
			counter.addToolCall(toolId3, "search", '{"query": "another"}')

			const breakdown = counter.getTokenBreakdown()

			// All should be counted separately
			expect(breakdown.text).toBeGreaterThan(0)
			expect(breakdown.toolCalls).toBeGreaterThan(0)
			expect(breakdown.total).toBeGreaterThan(breakdown.text)
		})

		it("should handle large text efficiently", () => {
			const largeText = "a".repeat(10000)
			const result = counter.addText(largeText)
			expect(result).toBeGreaterThan(0)
			expect(counter.getTotalTokens()).toBeGreaterThan(0)
		})
	})

	describe("FLAW-1 Validation: Incremental vs Full String Token Count", () => {
		it("should match tiktoken() for complete string after incremental additions", async () => {
				const fullText = "Hello, world! This is a test of the incremental token counter."
	
				// Add text incrementally (without trailing space to match exact string)
				const chunks = fullText.split(" ")
				for (let i = 0; i < chunks.length; i++) {
					if (i > 0) {
						counter.addText(" ")
					}
					counter.addText(chunks[i])
				}
	
				// Get the incremental counter result (without fudge factor for comparison)
				const breakdown = counter.getTokenBreakdown()
				const incrementalCount = breakdown.text
	
				// Use TokenizerManager directly for raw count
				const tiktokenCount = TokenizerManager.countTokens(fullText)
	
				// The counts should match exactly since we're using the same encoder
				expect(incrementalCount).toBe(tiktokenCount)
			})

		it("should handle Chinese characters correctly", async () => {
			const fullText = "你好世界，这是一个测试。"

			// Add text character by character (simulating worst-case streaming)
			for (const char of fullText) {
				counter.addText(char)
			}

			const breakdown = counter.getTokenBreakdown()
			const incrementalCount = breakdown.text
			const tiktokenCount = TokenizerManager.countTokens(fullText)

			expect(incrementalCount).toBe(tiktokenCount)
		})

		it("should handle mixed English and Chinese text", async () => {
			const fullText = "Hello 你好 world 世界！This is a 测试。"

			// Add text in chunks
			const chunks = ["Hello ", "你好", " world ", "世界！", "This is a ", "测试。"]
			for (const chunk of chunks) {
				counter.addText(chunk)
			}

			const breakdown = counter.getTokenBreakdown()
			const incrementalCount = breakdown.text
			const tiktokenCount = TokenizerManager.countTokens(fullText)

			expect(incrementalCount).toBe(tiktokenCount)
		})

		it("should handle special characters and punctuation", async () => {
			const fullText = "Test: something\nNew line\tTab \"quotes\" 'apostrophe' <tags> & symbols!"

			const chunks = fullText.split("")
			for (const char of chunks) {
				counter.addText(char)
			}

			const breakdown = counter.getTokenBreakdown()
			const incrementalCount = breakdown.text
			const tiktokenCount = TokenizerManager.countTokens(fullText)

			expect(incrementalCount).toBe(tiktokenCount)
		})

		it("should handle code snippets with special syntax", async () => {
			const fullText = `function test() {
		const x = "hello" + 'world';
		return x?.length ?? 0;
}`

			const chunks = fullText.split("\n")
			for (const chunk of chunks) {
				counter.addText(chunk + "\n")
			}

			const breakdown = counter.getTokenBreakdown()
			const incrementalCount = breakdown.text
			const tiktokenCount = TokenizerManager.countTokens(fullText)

			expect(incrementalCount).toBe(tiktokenCount)
		})

		it("should validate getTotalTokens() returns exact count without fudge factor", async () => {
			const fullText = "Test text for token count validation."
	
			counter.addText(fullText)
	
			const rawCount = TokenizerManager.countTokens(fullText)
			const actualTotal = counter.getTotalTokens()
	
			// No fudge factor applied - exact count returned
			expect(actualTotal).toBe(rawCount)
		})

		it("should handle reasoning content with same accuracy", async () => {
			const fullReasoning = "Let me think about this step by step..."

			counter.addReasoning(fullReasoning)

			const breakdown = counter.getTokenBreakdown()
			const incrementalCount = breakdown.reasoning
			const tiktokenCount = TokenizerManager.countTokens(fullReasoning)

			expect(incrementalCount).toBe(tiktokenCount)
		})

		it("should handle combined text, reasoning, and tool calls", async () => {
			const text = "I'll help you with that."
			const reasoning = "The user needs assistance."
			const toolId = "tool_1"
			const toolName = "read_file"
			const toolArgs = '{"path": "/test.txt"}'

			counter.addText(text)
			counter.addReasoning(reasoning)
			counter.addToolCall(toolId, toolName, toolArgs)

			const breakdown = counter.getTokenBreakdown()

			// Verify each component matches tiktoken
			expect(breakdown.text).toBe(TokenizerManager.countTokens(text))
			expect(breakdown.reasoning).toBe(TokenizerManager.countTokens(reasoning))

			// Tool calls are formatted as "Tool: name\nArguments: args"
			const toolStr = `Tool: ${toolName}\nArguments: ${toolArgs}`
			expect(breakdown.toolCalls).toBe(TokenizerManager.countTokens(toolStr))
		})
	})

	describe("TokenizerManager (FLAW-4 Fix)", () => {
		beforeEach(() => {
			// Reset the manager before each test
			TokenizerManager.reset()
		})

		it("should create encoder lazily on first access", () => {
			expect(TokenizerManager.hasInstance()).toBe(false)

			const encoder = TokenizerManager.getInstance()

			expect(encoder).toBeDefined()
			expect(TokenizerManager.hasInstance()).toBe(true)
		})

		it("should return the same instance on subsequent calls", () => {
			const instance1 = TokenizerManager.getInstance()
			const instance2 = TokenizerManager.getInstance()

			expect(instance1).toBe(instance2)
		})

		it("should allow disposal and recreation of encoder", () => {
			const instance1 = TokenizerManager.getInstance()
			expect(TokenizerManager.hasInstance()).toBe(true)

			TokenizerManager.dispose()
			expect(TokenizerManager.hasInstance()).toBe(false)

			const instance2 = TokenizerManager.getInstance()
			expect(TokenizerManager.hasInstance()).toBe(true)
			expect(instance1).not.toBe(instance2) // New instance
		})

		it("should count tokens correctly using static method", () => {
			const text = "Hello world"
			const count = TokenizerManager.countTokens(text)

			expect(count).toBeGreaterThan(0)
		})

		it("should handle empty text", () => {
			const count = TokenizerManager.countTokens("")
			expect(count).toBe(0)
		})

		it("should support reset for testing", () => {
			TokenizerManager.getInstance()
			expect(TokenizerManager.hasInstance()).toBe(true)

			TokenizerManager.reset()
			expect(TokenizerManager.hasInstance()).toBe(false)
		})
	})
})
