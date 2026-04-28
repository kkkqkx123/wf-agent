/**
 * Tool parameter validation utilities
 *
 * This module provides utilities for validating and filtering tool parameters.
 * All validation is based on Zod schema definitions, no static lists are maintained.
 *
 * Key features:
 * - Uses Zod schemas for type-safe validation
 * - Automatically extracts parameter names from schemas
 * - Provides detailed validation errors
 * - Filters invalid parameters
 */

import type { ToolName } from "@coder/types"
import type { ZodType, ZodError } from "zod"
import { getToolSchema, getToolNames } from "./schemas/registry"
import { resolveToolAlias } from "../prompts/tools/filter-tools-for-mode"

/**
 * Validation result interface
 */
export interface ValidationResult<T = unknown> {
	/** Whether validation passed */
	valid: boolean
	/** Validation errors (empty if valid) */
	errors: string[]
	/** Parsed and validated data (only if valid) */
	data?: T
	/** Raw Zod error (for debugging) */
	zodError?: ZodError
}

/**
 * Get parameter names for a specific tool from its Zod schema.
 *
 * @param toolName - The name of the tool
 * @returns Array of parameter names for the tool
 *
 * @example
 * ```typescript
 * const params = getToolParamNames("read_file")
 * // Returns: ["path", "mode", "offset", "limit", "indentation"]
 * ```
 */
export function getToolParamNames(toolName: ToolName): string[] {
	// Resolve tool alias to canonical name before getting schema
	const canonicalName = resolveToolAlias(toolName)
	const schema = getToolSchema(canonicalName as any)
	if (!schema) return []

	// Extract parameter names from Zod schema
	if ('shape' in schema && typeof schema.shape === 'object') {
		return Object.keys(schema.shape)
	}

	return []
}

/**
 * Validate tool parameters using the tool's Zod schema.
 * This is the primary validation method that uses Zod's full validation capabilities.
 *
 * @param toolName - The name of the tool
 * @param params - The parameters to validate
 * @returns Validation result with errors and validated data
 *
 * @example
 * ```typescript
 * const result = validateToolParams("read_file", {
 *   path: "file.txt",
 *   mode: "slice"
 * })
 * if (result.valid) {
 *   // result.data contains validated and typed parameters
 *   console.log(result.data.path)
 * } else {
 *   // result.errors contains detailed validation errors
 *   console.error(result.errors)
 * }
 * ```
 */
export function validateToolParams<TName extends ToolName>(
	toolName: TName,
	params: Record<string, unknown>,
): ValidationResult {
	// Resolve tool alias to canonical name before getting schema
	const canonicalName = resolveToolAlias(toolName)
	const schema = getToolSchema(canonicalName as any)

	if (!schema) {
		return {
			valid: false,
			errors: [`Tool '${toolName}' not found in registry`],
		}
	}

	// Use Zod's safeParse for validation
	const result = schema.safeParse(params)

	if (result.success) {
		return {
			valid: true,
			errors: [],
			data: result.data,
		}
	}

	// Extract error messages from Zod error
	const errors = result.error.errors.map((issue: any) => {
		const path = issue.path.length > 0 ? issue.path.join('.') : 'root'
		return `${path}: ${issue.message}`
	})

	return {
		valid: false,
		errors,
		zodError: result.error,
	}
}

/**
 * Filter valid parameters for a tool.
 * This is a lightweight alternative to full validation that only checks parameter names.
 *
 * @param toolName - The name of the tool
 * @param params - The parameters to filter
 * @returns Filtered parameters containing only valid ones
 *
 * @example
 * ```typescript
 * const filtered = filterToolParams("read_file", {
 *   path: "file.txt",
 *   invalid_param: "value"
 * })
 * // Returns: { path: "file.txt" }
 * ```
 */
export function filterToolParams<TName extends ToolName>(
	toolName: TName,
	params: Record<string, unknown>,
): Record<string, unknown> {
	const validNames = new Set(getToolParamNames(toolName))

	const filtered: Record<string, unknown> = {}
	for (const [key, value] of Object.entries(params)) {
		if (validNames.has(key)) {
			filtered[key] = value
		}
	}

	return filtered
}

/**
 * Check if a parameter name is valid for a tool.
 *
 * @param toolName - The name of the tool
 * @param paramName - The parameter name to check
 * @returns True if the parameter name is valid
 */
export function isValidToolParam(
	toolName: ToolName,
	paramName: string,
): boolean {
	const validNames = getToolParamNames(toolName)
	return validNames.includes(paramName)
}

/**
 * Get all parameter names across all tools.
 *
 * @returns Set of all unique parameter names
 */
export function getAllToolParamNames(): Set<string> {
	const allNames = new Set<string>()

	const toolNames = getToolNames()
	for (const toolName of toolNames) {
		const paramNames = getToolParamNames(toolName)
		for (const name of paramNames) {
			allNames.add(name)
		}
	}

	return allNames
}

/**
 * Check if a parameter name is used by any tool.
 *
 * @param paramName - The parameter name to check
 * @returns True if the parameter name is used by any tool
 */
export function isParamNameUsed(paramName: string): boolean {
	const allNames = getAllToolParamNames()
	return allNames.has(paramName)
}

/**
 * Get the Zod schema for a tool.
 * This is a convenience wrapper around getToolSchema from the registry.
 *
 * @param toolName - The name of the tool
 * @returns Zod schema or undefined if not found
 */
export function getToolZodSchema<TName extends ToolName>(
	toolName: TName,
): ZodType | undefined {
	// Resolve tool alias to canonical name before getting schema
	const canonicalName = resolveToolAlias(toolName)
	return getToolSchema(canonicalName as any)
}

/**
 * Parse and validate tool parameters with automatic type coercion.
 * This uses Zod's parse method which throws on validation failure.
 *
 * @param toolName - The name of the tool
 * @param params - The parameters to parse
 * @returns Parsed and validated parameters
 * @throws Error if validation fails
 *
 * @example
 * ```typescript
 * try {
 *   const data = parseToolParams("read_file", { path: "file.txt" })
 *   // data is typed correctly
 * } catch (error) {
 *   // Handle validation error
 * }
 * ```
 */
export function parseToolParams<TName extends ToolName>(
	toolName: TName,
	params: Record<string, unknown>,
): unknown {
	// Resolve tool alias to canonical name before getting schema
	const canonicalName = resolveToolAlias(toolName)
	const schema = getToolSchema(canonicalName as any)

	if (!schema) {
		throw new Error(`Tool '${toolName}' not found in registry`)
	}

	return schema.parse(params)
}
