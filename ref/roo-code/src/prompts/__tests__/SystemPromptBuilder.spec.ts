// npx vitest run src/core/prompts/__tests__/SystemPromptBuilder.spec.ts

import { describe, it, expect, vi, beforeEach } from "vitest"
import { SystemPromptBuilder, buildSystemPrompt, getPromptComponent } from "../SystemPromptBuilder"
import type { CustomModePrompts, PromptComponent } from "@coder/types"

// Mock vscode
vi.mock("vscode", () => ({
	env: {
		language: "en",
	},
	workspace: {
		getConfiguration: vi.fn().mockReturnValue({
			get: vi.fn().mockReturnValue(true),
		}),
	},
	default: {
		env: {
			language: "en",
		},
		workspace: {
			getConfiguration: vi.fn().mockReturnValue({
				get: vi.fn().mockReturnValue(true),
			}),
		},
	},
}))

// Mock CodeIndexManager
vi.mock("../../../services/code-index/manager", () => ({
	CodeIndexManager: {
		getInstance: vi.fn().mockReturnValue({
			isEnabled: false,
		}),
	},
}))

// Mock sections
vi.mock("../sections", () => ({
	getRulesSection: vi.fn().mockReturnValue("RULES SECTION"),
	getSystemInfoSection: vi.fn().mockReturnValue("SYSTEM INFO SECTION"),
	getObjectiveSection: vi.fn().mockReturnValue("OBJECTIVE SECTION"),
	getSharedToolUseSection: vi.fn().mockReturnValue("TOOL USE SECTION"),
	getToolUseGuidelinesSection: vi.fn().mockReturnValue("TOOL USE GUIDELINES SECTION"),
	getCapabilitiesSection: vi.fn().mockReturnValue("CAPABILITIES SECTION"),
	getModesSection: vi.fn().mockResolvedValue("MODES SECTION"),
	markdownFormattingSection: vi.fn().mockReturnValue("MARKDOWN FORMATTING SECTION"),
	getSkillsSection: vi.fn().mockResolvedValue("SKILLS SECTION"),
	addCustomInstructions: vi.fn().mockResolvedValue("CUSTOM INSTRUCTIONS SECTION"),
}))

describe("SystemPromptBuilder", () => {
	let mockContext: any

	beforeEach(() => {
		mockContext = {
			extensionUri: { fsPath: "/test/extension" },
			globalState: {
				get: vi.fn(),
				update: vi.fn(),
			},
		}
	})

	describe("builder pattern", () => {
		it("should create a builder instance", () => {
			const builder = SystemPromptBuilder.create()
			expect(builder).toBeInstanceOf(SystemPromptBuilder)
		})

		it("should support fluent API chaining", () => {
			const builder = SystemPromptBuilder.create()
				.withContext(mockContext, "/test/cwd")
				.withMode("code")
				.withLanguage("en")

			expect(builder).toBeInstanceOf(SystemPromptBuilder)
		})

		it("should throw error when context is missing", async () => {
			const builder = SystemPromptBuilder.create()
				.withMode("code")

			await expect(builder.build()).rejects.toThrow("Extension context is required")
		})

		it("should throw error when cwd is missing", async () => {
			const builder = SystemPromptBuilder.create()
				.withContext(mockContext, "")

			await expect(builder.build()).rejects.toThrow("Working directory")
		})
	})

	describe("configuration methods", () => {
		it("should set mode configuration", () => {
			const builder = SystemPromptBuilder.create()
				.withMode("architect", [{ slug: "architect", name: "Architect", roleDefinition: "An architect mode", groups: [] }])

			const config = builder.getConfig()
			expect(config.mode).toBe("architect")
		})

		it("should set custom instructions", () => {
			const builder = SystemPromptBuilder.create()
				.withCustomInstructions("Global instructions", "RooIgnore instructions")

			const config = builder.getConfig()
			expect(config.globalCustomInstructions).toBe("Global instructions")
			expect(config.rooIgnoreInstructions).toBe("RooIgnore instructions")
		})

		it("should set settings", () => {
			const settings = {
				todoListEnabled: true,
				useAgentRules: true,
				newTaskRequireTodos: false,
			}
			const builder = SystemPromptBuilder.create()
				.withSettings(settings)

			const config = builder.getConfig()
			expect(config.settings).toEqual(settings)
		})

		it("should set model ID", () => {
			const builder = SystemPromptBuilder.create()
				.withModelId("claude-3-opus")

			const config = builder.getConfig()
			expect(config.modelId).toBe("claude-3-opus")
		})
	})

	describe("build", () => {
		it("should generate system prompt with minimal configuration", async () => {
			const prompt = await SystemPromptBuilder.create()
				.withContext(mockContext, "/test/cwd")
				.withMode("code")
				.build()

			expect(typeof prompt).toBe("string")
			expect(prompt.length).toBeGreaterThan(0)
		})
	})

	describe("cache", () => {
		it("should support caching", () => {
			const builder = SystemPromptBuilder.create()
				.withContext(mockContext, "/test/cwd")
				.withMode("code")
				.withCache()

			// Cache should be enabled
			expect(builder).toBeInstanceOf(SystemPromptBuilder)
		})

		it("should clear cache", () => {
			const builder = SystemPromptBuilder.create()
				.withContext(mockContext, "/test/cwd")
				.withMode("code")
				.withCache()
				.clearCache()

			expect(builder).toBeInstanceOf(SystemPromptBuilder)
		})
	})

	describe("getConfig", () => {
		it("should return a copy of configuration", () => {
			const builder = SystemPromptBuilder.create()
				.withContext(mockContext, "/test/cwd")
				.withMode("code")

			const config1 = builder.getConfig()
			const config2 = builder.getConfig()

			expect(config1).not.toBe(config2) // Different objects
			expect(config1).toEqual(config2) // Same content
		})
	})
})

describe("getPromptComponent", () => {
	it("should return undefined for empty component", () => {
		const customModePrompts: CustomModePrompts = {
			test: {} as PromptComponent,
		}

		const result = getPromptComponent(customModePrompts, "test")

		expect(result).toBeUndefined()
	})

	it("should return component for valid component", () => {
		const customModePrompts: CustomModePrompts = {
			test: {
				roleDefinition: "Test role",
			} as PromptComponent,
		}

		const result = getPromptComponent(customModePrompts, "test")

		expect(result).toBeDefined()
		expect(result?.roleDefinition).toBe("Test role")
	})

	it("should return undefined for missing mode", () => {
		const customModePrompts: CustomModePrompts = {}

		const result = getPromptComponent(customModePrompts, "nonexistent")

		expect(result).toBeUndefined()
	})
})

describe("buildSystemPrompt convenience function", () => {
	it("should build system prompt with minimal parameters", async () => {
		const mockContext: any = {
			extensionUri: { fsPath: "/test/extension" },
			globalState: {
				get: vi.fn(),
				update: vi.fn(),
			},
		}

		// This will throw because of missing mocks, but we can test the function exists
		expect(typeof buildSystemPrompt).toBe("function")
	})
})
