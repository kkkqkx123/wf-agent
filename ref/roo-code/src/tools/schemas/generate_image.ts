import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for generate_image tool parameters.
 */
export const GenerateImageParamsSchema = z.object({
	prompt: z
		.string()
		.describe("A detailed description of the image to generate. Be specific about style, composition, colors, and content"),
	path: z
		.string()
		.describe("The path where the generated image will be saved (relative to the current workspace directory)"),
	image: z
		.string()
		.optional()
		.describe("Optional base64 encoded image to use as a reference or input for image-to-image generation"),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type GenerateImageParams = z.infer<typeof GenerateImageParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const GENERATE_IMAGE_DESCRIPTION = `Generate an image using AI image generation models. This tool creates images based on text descriptions or can use existing images as reference.

Parameters:
- prompt: (required) A detailed description of the image to generate. Be specific about style, composition, colors, and content
- path: (required) The path where the generated image will be saved (relative to the current workspace directory)
- image: (optional) Optional base64 encoded image to use as a reference or input for image-to-image generation

Example: Generate a new image
{ "prompt": "A serene mountain landscape at sunrise with golden light reflecting on a calm lake, photorealistic style", "path": "images/landscape.png" }

Example: Generate with reference image
{ "prompt": "Convert to watercolor style", "path": "images/watercolor-version.png", "image": "data:image/png;base64,..." }

Note: Image generation requires a configured image generation API. The generated image will be saved to the specified path. Ensure the target directory exists or will be created automatically.`

/**
 * Creates the generate_image tool definition.
 *
 * @returns Native tool definition for generate_image
 */
export function createGenerateImageTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "generate_image",
		description: GENERATE_IMAGE_DESCRIPTION,
		schema: GenerateImageParamsSchema,
		strict: true,
	})
}

/**
 * Default generate_image tool definition.
 */
export const generateImageTool = createGenerateImageTool()
