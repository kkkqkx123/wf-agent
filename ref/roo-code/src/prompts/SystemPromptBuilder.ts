/**
 * System Prompt Builder
 * 
 * Provides a fluent Builder pattern interface for constructing system prompts.
 * This simplifies the configuration process and reduces the number of parameters
 * needed when creating system prompts.
 * 
 * Features:
 * - Fluent API for configuration
 * - Caching support for repeated builds with same configuration
 * - Type-safe configuration
 * - Reusable builder instances
 */

import * as vscode from "vscode"
import { type ModeConfig, type PromptComponent, type CustomModePrompts, type TodoItem } from "@coder/types"
import { Mode, modes, defaultModeSlug, getModeBySlug, getGroupName, getModeSelection } from "../../shared/modes"
import { DiffStrategy } from "../../shared/tools"
import { formatLanguage } from "@coder/types"
import { isEmpty } from "../../utils/object"

import { McpHub } from "../../services/mcp/McpHub"
import { CodeIndexManager } from "../../services/code-index/manager"
import { SkillsManager } from "../../services/skills/SkillsManager"

import type { SystemPromptSettings } from "./types"
import {
	getRulesSection,
	getSystemInfoSection,
	getObjectiveSection,
	getSharedToolUseSection,
	getToolUseGuidelinesSection,
	getCapabilitiesSection,
	getModesSection,
	addCustomInstructions,
	markdownFormattingSection,
	getSkillsSection,
} from "./sections"

/**
 * Configuration for SystemPromptBuilder
 */
export interface SystemPromptConfig {
	context: vscode.ExtensionContext
	cwd: string
	supportsComputerUse: boolean
	mcpHub?: McpHub
	diffStrategy?: DiffStrategy
	mode: Mode
	customModePrompts?: CustomModePrompts
	customModes?: ModeConfig[]
	globalCustomInstructions?: string
	experiments?: Record<string, boolean>
	language?: string
	rooIgnoreInstructions?: string
	settings?: SystemPromptSettings
	todoList?: TodoItem[]
	modelId?: string
	skillsManager?: SkillsManager
}

/**
 * Builder for creating system prompts with a fluent API
 * 
 * @example
 * ```typescript
 * const systemPrompt = await SystemPromptBuilder.create()
 *   .withContext(context, cwd)
 *   .withMode(mode, customModes, customModePrompts)
 *   .withMcp(mcpHub)
 *   .withCustomInstructions(customInstructions, rooIgnoreInstructions)
 *   .withSettings(settings)
 *   .build()
 * ```
 */
export class SystemPromptBuilder {
	private config: Partial<SystemPromptConfig> = {}
	private cacheKey: string | null = null
	private cachedPrompt: string | null = null

	private constructor() {}

	/**
	 * Create a new SystemPromptBuilder instance
	 */
	static create(): SystemPromptBuilder {
		return new SystemPromptBuilder()
	}

	/**
	 * Set the extension context and working directory
	 */
	withContext(context: vscode.ExtensionContext, cwd: string): this {
		this.config.context = context
		this.config.cwd = cwd
		return this
	}

	/**
	 * Set the mode configuration
	 */
	withMode(
		mode: Mode,
		customModes?: ModeConfig[],
		customModePrompts?: CustomModePrompts
	): this {
		this.config.mode = mode
		this.config.customModes = customModes
		this.config.customModePrompts = customModePrompts
		return this
	}

	/**
	 * Set the MCP hub for MCP server integration
	 */
	withMcp(mcpHub?: McpHub): this {
		this.config.mcpHub = mcpHub
		return this
	}

	/**
	 * Set the diff strategy
	 */
	withDiffStrategy(diffStrategy?: DiffStrategy): this {
		this.config.diffStrategy = diffStrategy
		return this
	}

	/**
	 * Set custom instructions
	 */
	withCustomInstructions(
		globalCustomInstructions?: string,
		rooIgnoreInstructions?: string
	): this {
		this.config.globalCustomInstructions = globalCustomInstructions
		this.config.rooIgnoreInstructions = rooIgnoreInstructions
		return this
	}

	/**
	 * Set the language preference
	 */
	withLanguage(language?: string): this {
		this.config.language = language
		return this
	}

	/**
	 * Set experiments configuration
	 */
	withExperiments(experiments?: Record<string, boolean>): this {
		this.config.experiments = experiments
		return this
	}

	/**
	 * Set system prompt settings
	 */
	withSettings(settings?: SystemPromptSettings): this {
		this.config.settings = settings
		return this
	}

	/**
	 * Set the model ID
	 */
	withModelId(modelId?: string): this {
		this.config.modelId = modelId
		return this
	}

	/**
	 * Set the skills manager
	 */
	withSkillsManager(skillsManager?: SkillsManager): this {
		this.config.skillsManager = skillsManager
		return this
	}

	/**
	 * Set the todo list
	 */
	withTodoList(todoList?: TodoItem[]): this {
		this.config.todoList = todoList
		return this
	}

	/**
	 * Set whether computer use is supported
	 */
	withComputerUseSupport(supported: boolean): this {
		this.config.supportsComputerUse = supported
		return this
	}

	/**
	 * Set the full configuration at once
	 */
	withFullConfig(config: SystemPromptConfig): this {
		this.config = { ...config }
		return this
	}

	/**
	 * Enable caching for repeated builds
	 * The cache key is computed from the current configuration
	 */
	withCache(): this {
		this.cacheKey = this.computeCacheKey()
		return this
	}

	/**
	 * Build the system prompt
	 * 
	 * @returns The generated system prompt string
	 * @throws Error if required configuration is missing
	 */
	async build(): Promise<string> {
		// Validate required fields
		if (!this.config.context) {
			throw new Error("Extension context is required for generating system prompt")
		}
		if (!this.config.cwd) {
			throw new Error("Working directory (cwd) is required for generating system prompt")
		}

		// Check cache if enabled
		if (this.cacheKey && this.cachedPrompt) {
			const currentKey = this.computeCacheKey()
			if (currentKey === this.cacheKey) {
				return this.cachedPrompt
			}
		}

		// Build the prompt
		const prompt = await this.generatePrompt()

		// Cache if enabled
		if (this.cacheKey) {
			this.cachedPrompt = prompt
		}

		return prompt
	}

	/**
	 * Generate the system prompt from current configuration
	 */
	private async generatePrompt(): Promise<string> {
		const {
			context,
			cwd,
			supportsComputerUse = false,
			mcpHub,
			diffStrategy,
			mode = defaultModeSlug,
			customModePrompts,
			customModes,
			globalCustomInstructions,
			experiments,
			language,
			rooIgnoreInstructions,
			settings,
			todoList,
			modelId,
			skillsManager,
		} = this.config

		// Get prompt component for custom mode
		const promptComponent = getPromptComponent(customModePrompts, mode)

		// Get mode configuration
		const modeConfig = getModeBySlug(mode, customModes) || modes.find((m) => m.slug === mode) || modes[0]
		const { roleDefinition, baseInstructions } = getModeSelection(mode, promptComponent, customModes)

		// Check if MCP should be included
		const hasMcpGroup = modeConfig?.groups.some((groupEntry) => getGroupName(groupEntry) === "mcp") ?? false
		const hasMcpServers = mcpHub && mcpHub.getServers().length > 0
		const shouldIncludeMcp = hasMcpGroup && hasMcpServers

		// Get code index manager
		const codeIndexManager = context ? CodeIndexManager.getInstance(context, cwd!) : undefined

		// Get modes and skills sections in parallel
		const [modesSection, skillsSection] = await Promise.all([
			getModesSection(context!),
			getSkillsSection(skillsManager, mode as string, settings?.skillsEnabled, settings?.disabledSkills),
		])

		// Build the base prompt
		const basePrompt = `${roleDefinition}

${markdownFormattingSection()}

${getSharedToolUseSection()}

\t${getToolUseGuidelinesSection()}

${getCapabilitiesSection(cwd!, shouldIncludeMcp ? mcpHub : undefined)}

${modesSection}
${skillsSection ? `\n${skillsSection}` : ""}
${getRulesSection(cwd!, settings)}

${getSystemInfoSection(cwd!)}

${getObjectiveSection()}

${await addCustomInstructions(baseInstructions, globalCustomInstructions || "", cwd!, mode, {
	language: language ?? formatLanguage(vscode.env.language),
	rooIgnoreInstructions,
	settings,
})}`

		return basePrompt
	}

	/**
	 * Compute a cache key from the current configuration
	 */
	private computeCacheKey(): string {
		const {
			cwd,
			mode,
			customModes,
			globalCustomInstructions,
			rooIgnoreInstructions,
			settings,
			modelId,
		} = this.config

		// Create a simple hash of the key configuration values
		const keyParts = [
			cwd || "",
			mode || "",
			JSON.stringify(customModes?.map((m) => m.slug)?.sort() || []),
			globalCustomInstructions || "",
			rooIgnoreInstructions || "",
			JSON.stringify(settings || {}),
			modelId || "",
		]

		return keyParts.join("|")
	}

	/**
	 * Clear the cache
	 */
	clearCache(): this {
		this.cacheKey = null
		this.cachedPrompt = null
		return this
	}

	/**
	 * Get the current configuration (for debugging)
	 */
	getConfig(): Partial<SystemPromptConfig> {
		return { ...this.config }
	}
}

/**
 * Helper function to get prompt component, filtering out empty objects
 * (Re-exported from system.ts for consistency)
 */
export function getPromptComponent(
	customModePrompts: CustomModePrompts | undefined,
	mode: string
): PromptComponent | undefined {
	const component = customModePrompts?.[mode]
	if (isEmpty(component)) {
		return undefined
	}
	return component
}

/**
 * Convenience function to build a system prompt with minimal configuration
 */
export async function buildSystemPrompt(
	context: vscode.ExtensionContext,
	cwd: string,
	options?: Partial<SystemPromptConfig>
): Promise<string> {
	return SystemPromptBuilder.create()
		.withContext(context, cwd)
		.withFullConfig({ ...options, context, cwd } as SystemPromptConfig)
		.build()
}

/**
 * Legacy SYSTEM_PROMPT function - kept for backward compatibility and testing purposes.
 * This function is a wrapper around SystemPromptBuilder.
 * New code should use SystemPromptBuilder directly.
 */
export const SYSTEM_PROMPT = async (
	context: vscode.ExtensionContext,
	cwd: string,
	supportsComputerUse: boolean,
	mcpHub?: McpHub,
	diffStrategy?: DiffStrategy,
	mode: Mode = defaultModeSlug,
	customModePrompts?: CustomModePrompts,
	customModes?: ModeConfig[],
	globalCustomInstructions?: string,
	experiments?: Record<string, boolean>,
	language?: string,
	rooIgnoreInstructions?: string,
	settings?: SystemPromptSettings,
	todoList?: TodoItem[],
	modelId?: string,
	skillsManager?: SkillsManager,
): Promise<string> => {
	if (!context) {
		throw new Error("Extension context is required for generating system prompt")
	}

	return await SystemPromptBuilder.create()
		.withContext(context, cwd)
		.withMode(mode, customModes, customModePrompts)
		.withMcp(mcpHub)
		.withDiffStrategy(diffStrategy)
		.withCustomInstructions(globalCustomInstructions, rooIgnoreInstructions)
		.withExperiments(experiments)
		.withLanguage(language)
		.withSettings(settings)
		.withTodoList(todoList)
		.withModelId(modelId)
		.withSkillsManager(skillsManager)
		.withComputerUseSupport(supportsComputerUse)
		.build()
}
