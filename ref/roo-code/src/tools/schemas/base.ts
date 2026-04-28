import { z } from "zod"
import { zodToJsonSchema } from "zod-to-json-schema"
import type OpenAI from "openai"

/**
 * Base utilities for tool schema definitions.
 *
 * This module provides common utilities for:
 * - Type coercion (string to number/boolean)
 * - JSON Schema generation from Zod schemas
 * - OpenAI tool definition creation
 */

// ─── Type Coercion Utilities ────────────────────────────────────────────────────

/**
 * Coerces an optional value to a number.
 * Handles both number and string inputs.
 *
 * @param value - The value to coerce
 * @returns The coerced number, or undefined if coercion fails
 */
export function coerceOptionalNumber(value: unknown): number | undefined {
	if (typeof value === "number" && Number.isFinite(value)) {
		return value
	}
	if (typeof value === "string") {
		const n = Number(value)
		if (Number.isFinite(n)) {
			return n
		}
	}
	return undefined
}

/**
 * Coerces an optional value to a boolean.
 * Handles both boolean and string inputs ("true"/"false").
 *
 * @param value - The value to coerce
 * @returns The coerced boolean, or undefined if coercion fails
 */
export function coerceOptionalBoolean(value: unknown): boolean | undefined {
	if (typeof value === "boolean") {
		return value
	}
	if (typeof value === "string") {
		const lower = value.trim().toLowerCase()
		if (lower === "true") {
			return true
		}
		if (lower === "false") {
			return false
		}
	}
	return undefined
}

// ─── Zod Coercion Schemas ────────────────────────────────────────────────────────

/**
 * Zod schema that coerces string input to number.
 * Useful for API parameters that may be sent as strings.
 */
export const coercedNumber = z.union([
	z.number(),
	z.string().transform((val, ctx) => {
		const n = Number(val)
		if (Number.isNaN(n)) {
			ctx.addIssue({
				code: z.ZodIssueCode.custom,
				message: "Invalid number string",
			})
			return z.NEVER
		}
		return n
	}),
])

/**
 * Zod schema that coerces string input to boolean.
 * Accepts "true"/"false" (case-insensitive) and boolean values.
 */
export const coercedBoolean = z.union([
	z.boolean(),
	z.string().transform((val, ctx) => {
		const lower = val.trim().toLowerCase()
		if (lower === "true") return true
		if (lower === "false") return false
		ctx.addIssue({
			code: z.ZodIssueCode.custom,
			message: "Invalid boolean string. Expected 'true' or 'false'",
		})
		return z.NEVER
	}),
])

// ─── JSON Schema Generation ──────────────────────────────────────────────────────

/**
 * Options for creating an OpenAI tool definition from a Zod schema.
 */
export interface CreateToolOptions {
	/** Tool name */
	name: string
	/** Tool description */
	description: string
	/** Zod schema for the tool parameters */
	schema: z.ZodType<any, any>
	/** Whether to use strict mode (default: true) */
	strict?: boolean
}

/**
 * Creates an OpenAI ChatCompletionTool from a Zod schema.
 *
 * @param options - Tool creation options
 * @returns OpenAI tool definition
 */
export function createOpenAITool(options: CreateToolOptions): OpenAI.Chat.ChatCompletionTool {
	const { name, description, schema, strict = true } = options

	return {
		type: "function",
		function: {
			name,
			description,
			strict,
			parameters: zodToJsonSchema(schema),
		},
	} satisfies OpenAI.Chat.ChatCompletionTool
}

/**
 * Converts a Zod schema to JSON Schema format for OpenAI.
 *
 * @param schema - Zod schema to convert
 * @returns JSON Schema object
 */
export function schemaToJsonSchema<T extends z.ZodType<any, any>>(
	schema: T,
): Record<string, unknown> {
	return zodToJsonSchema(schema) as Record<string, unknown>
}

// ─── Partial Parsing Support ─────────────────────────────────────────────────────

/**
 * Options for partial parsing of tool arguments.
 */
export interface PartialParseOptions {
	/** Whether to allow partial/incomplete objects */
	partial?: boolean
}

/**
 * Safely parses tool arguments using a Zod schema.
 * Returns the parsed data or undefined if parsing fails.
 *
 * @param schema - Zod schema to use for parsing
 * @param args - Arguments to parse
 * @param options - Parse options
 * @returns Parsed data or undefined
 */
export function safeParseArgs<T extends z.ZodType<any, any>>(
	schema: T,
	args: unknown,
	options: PartialParseOptions = {},
): z.infer<T> | undefined {
	const { partial = false } = options

	// For partial parsing, we use safeParse and return partial data
	const result = schema.safeParse(args)

	if (result.success) {
		return result.data
	}

	// For partial mode, try to extract what we can
	if (partial) {
		// Return the raw args as-is for partial updates
		// The actual partial handling is done by the tool handlers
		return args as z.infer<T>
	}

	return undefined
}
