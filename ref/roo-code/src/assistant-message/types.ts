import type { ToolUse, McpToolUse } from "../../shared/tools"

/**
 * Text block with partial support for streaming.
 */
export interface TextBlock {
	type: "text"
	/** The text content */
	content: string
	/** Whether this is a partial block still being streamed */
	partial?: boolean
}

export type AssistantMessageContent = TextBlock | ToolUse | McpToolUse

// Re-export ToolUse for use in other modules
export type { ToolUse } from "../../shared/tools"
