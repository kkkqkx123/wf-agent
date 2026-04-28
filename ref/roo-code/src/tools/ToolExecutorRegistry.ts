/**
 * Tool Executor Registry
 *
 * Provides a centralized registry for tool executors, enabling:
 * - Unified tool dispatch mechanism
 * - Dynamic tool registration
 * - Simplified tool management
 *
 * This replaces the previous approach of scattered singleton exports
 * and large switch-case dispatch in presentAssistantMessage.ts.
 */

import type { ToolName } from "@coder/types"
import type { Task } from "../task/Task"
import type { ToolUse } from "../../shared/tools"
import type { BaseTool, ToolCallbacks } from "./core/BaseTool"

/**
 * Tool Executor Registry
 *
 * Manages all tool executor instances in a centralized location.
 * Provides O(1) lookup for tool dispatch.
 */
class ToolExecutorRegistry {
	/**
	 * Internal map storing tool executors by name
	 */
	private executors = new Map<ToolName, BaseTool<any>>()

	/**
	 * Register a tool executor
	 *
	 * @param name - Tool name (must match ToolName type)
	 * @param executor - Tool executor instance
	 */
	register<T extends ToolName>(name: T, executor: BaseTool<T>): void {
		if (this.executors.has(name)) {
			console.warn(`[ToolExecutorRegistry] Tool "${name}" is already registered, overwriting`)
		}
		this.executors.set(name, executor)
	}

	/**
	 * Get a tool executor by name
	 *
	 * @param name - Tool name
	 * @returns Tool executor instance or undefined if not found
	 */
	get<T extends ToolName>(name: T): BaseTool<T> | undefined {
		return this.executors.get(name)
	}

	/**
	 * Check if a tool is registered
	 *
	 * @param name - Tool name to check
	 * @returns true if the tool is registered
	 */
	has(name: string): boolean {
		return this.executors.has(name as ToolName)
	}

	/**
	 * Get all registered tool names
	 *
	 * @returns Array of registered tool names
	 */
	getNames(): ToolName[] {
		return Array.from(this.executors.keys())
	}

	/**
	 * Get the number of registered tools
	 *
	 * @returns Number of registered tools
	 */
	get size(): number {
		return this.executors.size
	}

	/**
	 * Unified execution entry point
	 *
	 * @param name - Tool name
	 * @param task - Task instance
	 * @param block - Tool use block from assistant message
	 * @param callbacks - Tool execution callbacks
	 * @throws Error if tool is not registered
	 */
	async execute(
		name: ToolName,
		task: Task,
		block: ToolUse<any>,
		callbacks: ToolCallbacks,
	): Promise<void> {
		const executor = this.executors.get(name)
		if (!executor) {
			throw new Error(`Unknown tool: ${name}`)
		}
		return executor.handle(task, block, callbacks)
	}
}

/**
 * Singleton instance of the tool executor registry
 *
 * All built-in tools are registered in src/core/tools/index.ts
 */
export const toolExecutorRegistry = new ToolExecutorRegistry()

/**
 * Re-export ToolCallbacks for convenience
 */
export type { ToolCallbacks } from "./core/BaseTool"
