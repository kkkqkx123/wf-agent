import { Anthropic } from "@anthropic-ai/sdk"
import { Tiktoken } from "tiktoken/lite"
import o200kBase from "tiktoken/encoders/o200k_base"

// Note: TOKEN_FUDGE_FACTOR has been removed. Token counting now returns exact
// values from tiktoken encoder. API providers (Anthropic, OpenAI) return
// accurate usage data, making the fudge factor unnecessary and causing
// significant overestimation (50%) in cost calculations.

/**
 * TokenizerManager - Manages the lifecycle of the Tiktoken encoder.
 *
 * This class addresses FLAW-4 by providing:
 * - Controlled initialization (lazy loading)
 * - Explicit cleanup to release WASM memory
 * - Thread-safe access through a singleton pattern
 * - Test-friendly reset capability
 *
 * Usage:
 *   const encoder = TokenizerManager.getInstance()
 *   const tokens = encoder.encode(text)
 *   // ... when done ...
 *   TokenizerManager.dispose()
 */
export class TokenizerManager {
	private static instance: Tiktoken | null = null

	/**
	 * Get the singleton encoder instance.
	 * Creates the encoder lazily on first access.
	 * @returns The Tiktoken encoder instance
	 */
	static getInstance(): Tiktoken {
		if (!TokenizerManager.instance) {
			TokenizerManager.instance = new Tiktoken(
				o200kBase.bpe_ranks,
				o200kBase.special_tokens,
				o200kBase.pat_str,
			)
		}
		return TokenizerManager.instance
	}

	/**
	 * Check if the encoder instance exists.
	 * @returns true if encoder is initialized
	 */
	static hasInstance(): boolean {
		return TokenizerManager.instance !== null
	}

	/**
	 * Dispose of the encoder instance to free WASM memory.
	 * After calling this, getInstance() will create a new instance.
	 *
	 * Note: Tiktoken lite doesn't have an explicit dispose method,
	 * but setting the reference to null allows GC to reclaim memory.
	 */
	static dispose(): void {
		TokenizerManager.instance = null
	}

	/**
	 * Reset the encoder instance (primarily for testing).
	 * Forces creation of a fresh instance on next getInstance() call.
	 */
	static reset(): void {
		TokenizerManager.dispose()
	}

	/**
	 * Encode text and return token array.
	 * Convenience method that gets encoder and encodes in one call.
	 * @param text - Text to encode
	 * @returns Uint32Array of token IDs
	 */
	static encode(text: string): Uint32Array {
	 const encoder = TokenizerManager.getInstance()
	 return encoder.encode(text, undefined, [])
	}

	/**
	 * Count tokens for text without returning token array.
	 * More efficient when only the count is needed.
	 * @param text - Text to count tokens for
	 * @returns Number of tokens
	 */
	static countTokens(text: string): number {
	 const tokens = TokenizerManager.encode(text)
	 return tokens.length
	}
}

/**
 * Comprehensive incremental token counter for streaming assistant responses.
 * Tracks text, reasoning, and tool calls to provide accurate token estimation.
 */
export class StreamingTokenCounter {
	private accumulatedText: string = ""
	private accumulatedReasoning: string = ""
	private toolCalls: Map<string, { name: string; args: string }> = new Map()
	private textTokenCount: number = 0
	private reasoningTokenCount: number = 0
	private toolCallsTokenCount: number = 0

	/**
	 * Add text content and return the incremental token count.
	 * @param text - New text to add
	 * @returns The number of tokens in the newly added text
	 */
	addText(text: string): number {
		if (!text || text.length === 0) {
			return 0
		}

		this.accumulatedText += text
		const newTotalTokens = this.countTokens(this.accumulatedText)
		const incrementalTokens = newTotalTokens - this.textTokenCount
		this.textTokenCount = newTotalTokens

		return incrementalTokens
	}

	/**
	 * Add reasoning content and return the incremental token count.
	 * @param text - New reasoning text to add
	 * @returns The number of tokens in the newly added reasoning text
	 */
	addReasoning(text: string): number {
		if (!text || text.length === 0) {
			return 0
		}

		this.accumulatedReasoning += text
		const newTotalTokens = this.countTokens(this.accumulatedReasoning)
		const incrementalTokens = newTotalTokens - this.reasoningTokenCount
		this.reasoningTokenCount = newTotalTokens

		return incrementalTokens
	}

	/**
	 * Add or update a tool call and return the incremental token count.
	 * @param toolCallId - Unique identifier for this tool call (required for distinguishing multiple calls to same tool)
	 * @param toolName - Name of the tool
	 * @param args - Tool arguments (partial or complete)
	 * @returns The incremental token count for this tool call
	 */
	addToolCall(toolCallId: string, toolName: string, args: string): number {
		if (!toolCallId || !toolName) {
			return 0
		}

		// Find existing tool call by ID (supports multiple calls to same tool)
		const existingCall = this.toolCalls.get(toolCallId)
		const toolCallStr = `Tool: ${toolName}\nArguments: ${args}`
		const newTokens = this.countTokens(toolCallStr)

		if (existingCall) {
			// Update existing tool call with streaming arguments
			const oldToolCallStr = `Tool: ${existingCall.name}\nArguments: ${existingCall.args}`
			const oldTokens = this.countTokens(oldToolCallStr)
			this.toolCallsTokenCount -= oldTokens

			this.toolCalls.set(toolCallId, { name: toolName, args })
			this.toolCallsTokenCount += newTokens

			return newTokens - oldTokens
		} else {
			// Add new tool call
			this.toolCalls.set(toolCallId, { name: toolName, args })
			this.toolCallsTokenCount += newTokens
			return newTokens
		}
	}

	/**
	 * Get the total token count for all accumulated content.
	 * Returns exact token count from tiktoken encoder without any fudge factor.
	 * @returns Total token count (text + reasoning + tool calls)
	 */
	getTotalTokens(): number {
		return this.textTokenCount + this.reasoningTokenCount + this.toolCallsTokenCount
	}

	/**
	 * Get token count breakdown by category.
	 * Useful for debugging and understanding the distribution of tokens.
	 * @returns Object with text, reasoning, toolCalls, and total counts
	 */
	getTokenBreakdown(): { text: number; reasoning: number; toolCalls: number; total: number } {
		return {
			text: this.textTokenCount,
			reasoning: this.reasoningTokenCount,
			toolCalls: this.toolCallsTokenCount,
			total: this.getTotalTokens(),
		}
	}

	/**
	 * Reset the counter.
	 */
	reset(): void {
		this.accumulatedText = ""
		this.accumulatedReasoning = ""
		this.toolCalls = new Map()
		this.textTokenCount = 0
		this.reasoningTokenCount = 0
		this.toolCallsTokenCount = 0
	}

	/**
	 * Count tokens for a given text string.
	 * Uses TokenizerManager for efficient encoder lifecycle management.
	 * @param text - Text to count tokens for
	 * @returns Token count
	 */
	private countTokens(text: string): number {
		if (!text || text.length === 0) {
			return 0
		}
		return TokenizerManager.countTokens(text)
	}
}

/**
 * Serializes a tool_use block to text for token counting.
 * Approximates how the API sees the tool call.
 */
function serializeToolUse(block: Anthropic.Messages.ToolUseBlockParam): string {
	const parts = [`Tool: ${block.name}`]
	if (block.input !== undefined) {
		try {
			parts.push(`Arguments: ${JSON.stringify(block.input)}`)
		} catch {
			parts.push(`Arguments: [serialization error]`)
		}
	}
	return parts.join("\n")
}

/**
 * Serializes a tool_result block to text for token counting.
 * Handles both string content and array content.
 */
function serializeToolResult(block: Anthropic.Messages.ToolResultBlockParam): string {
	const parts = [`Tool Result (${block.tool_use_id})`]

	if (block.is_error) {
		parts.push(`[Error]`)
	}

	const content = block.content
	if (typeof content === "string") {
		parts.push(content)
	} else if (Array.isArray(content)) {
		// Handle array of content blocks recursively
		for (const item of content) {
			if (item.type === "text") {
				parts.push(item.text || "")
			} else if (item.type === "image") {
				parts.push("[Image content]")
			} else {
				parts.push(`[Unsupported content block: ${String((item as { type?: unknown }).type)}]`)
			}
		}
	}

	return parts.join("\n")
}

export async function tiktoken(content: Anthropic.Messages.ContentBlockParam[]): Promise<number> {
	if (content.length === 0) {
		return 0
	}

	let totalTokens = 0

	// Process each content block using the TokenizerManager.
	for (const block of content) {
		if (block.type === "text") {
			const text = block.text || ""

			if (text.length > 0) {
				const tokens = TokenizerManager.encode(text)
				totalTokens += tokens.length
			}
		} else if (block.type === "image") {
			// Estimate image tokens based on Anthropic's Vision Token model.
			// Images are charged based on their resolution, not file size.
			// Rough formula: ceil(width * height / 750) + 200, but we estimate from base64 data size.
			const imageSource = block.source

			if (imageSource && typeof imageSource === "object" && "data" in imageSource) {
				const base64Data = imageSource.data as string
				// Base64 encoding: 4 chars = 3 bytes, so actual data size = base64.length * 3/4
				const estimatedDataBytes = base64Data.length * 0.75
				// Approximate resolution from data size (assuming typical JPEG compression ~0.5 bytes per pixel)
				// Pixels ≈ dataBytes / 0.5 = dataBytes * 2
				// Vision tokens ≈ ceil(pixels / 750) + 200
				const estimatedPixels = estimatedDataBytes * 2
				const estimatedTokens = Math.ceil(estimatedPixels / 750) + 200
				totalTokens += estimatedTokens
			} else {
				// Conservative estimate for images without data
				// Standard estimate: ~170 tokens for typical image (VGA 640x480)
				totalTokens += 170
			}
		} else if (block.type === "tool_use") {
			// Serialize tool_use block to text and count tokens
			const serialized = serializeToolUse(block as Anthropic.Messages.ToolUseBlockParam)
			if (serialized.length > 0) {
				const tokens = TokenizerManager.encode(serialized)
				totalTokens += tokens.length
			}
		} else if (block.type === "tool_result") {
			// Serialize tool_result block to text and count tokens
			const serialized = serializeToolResult(block as Anthropic.Messages.ToolResultBlockParam)
			if (serialized.length > 0) {
				const tokens = TokenizerManager.encode(serialized)
				totalTokens += tokens.length
			}
		}
	}

	// Return exact token count without fudge factor.
	// API providers (Anthropic, OpenAI) return accurate usage data,
	// making estimation buffers unnecessary for most use cases.
	return totalTokens
}
