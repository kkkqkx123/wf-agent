import path from "path"
import fs from "fs/promises"

import { type ClineSayTool } from "@coder/types"
import {
	type ImageGenerationConfig,
	type ImageGenerationProvider,
	getDefaultBaseUrl,
	getDefaultApiMethod,
} from "@coder/types"

import { Task } from "../task/Task"
import { formatResponse } from "../prompts/responses"
import { fileExistsAtPath, createDirectoriesForFile } from "../../utils/fs"
import { getReadablePath } from "../../utils/path"
import { isPathOutsideWorkspace } from "../../utils/pathUtils"
import {
	generateImageWithProvider,
	generateImageWithImagesApi,
	type ImageGenerationResult,
} from "../../api/providers/utils/image-generation"
import { t } from "../../i18n"

import { BaseTool, ToolCallbacks } from "./core/BaseTool"
import { MissingParameterError } from "../errors/tools/index.js"

interface GenerateImageParams {
	prompt: string
	path: string
	image?: string
}

export class GenerateImageTool extends BaseTool<"generate_image"> {
	readonly name = "generate_image" as const

	async execute(params: GenerateImageParams, task: Task, callbacks: ToolCallbacks): Promise<void> {
		const { pushToolResult, handleError, askApproval } = callbacks
		const { prompt, path: relPath, image: inputImage } = params

		// Validate required parameters
		if (!prompt) {
			const error = new MissingParameterError("generate_image", "prompt")
			task.recordToolError("generate_image", error.toLogEntry())
			pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
			return
		}

		if (!relPath) {
			const error = new MissingParameterError("generate_image", "path")
			task.recordToolError("generate_image", error.toLogEntry())
			pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
			return
		}

		// Get image generation configuration
		const config = await this.getImageGenerationConfig(task)

		if (!config) {
			const errorMsg = t("tools:generateImage.noConfig")
			pushToolResult(formatResponse.toolError(errorMsg))
			return
		}

		// Get API key from secret storage
		const apiKey = await this.getApiKey(task, config)

		if (!apiKey) {
			const errorMsg = t("tools:generateImage.noApiKey")
			pushToolResult(formatResponse.toolError(errorMsg))
			return
		}

		const absolutePath = path.resolve(task.cwd, relPath)

		// Create parent directories if needed
		try {
			await createDirectoriesForFile(absolutePath, task.cwd)
		} catch (error) {
			const errorDetails = error instanceof Error ? error.message : String(error)
			pushToolResult(formatResponse.toolError(`Failed to create directory: ${errorDetails}`))
			return
		}

		// Ask for approval
		const approvalMessage: ClineSayTool = {
			tool: "generateImage",
			path: getReadablePath(relPath, task.cwd),
		}

		const didApprove = await askApproval("tool", JSON.stringify(approvalMessage))

		if (!didApprove) {
			pushToolResult(formatResponse.toolDenied())
			return
		}

		// Generate the image
		try {
			const baseUrl = config.baseUrl || getDefaultBaseUrl(config.provider)

			if (!baseUrl) {
				throw new Error(`No base URL configured for provider: ${config.provider}`)
			}

			const apiMethod = config.apiMethod || getDefaultApiMethod(config.provider)

			let result: ImageGenerationResult

			if (apiMethod === "images_api") {
				result = await generateImageWithImagesApi({
					baseURL: baseUrl,
					authToken: apiKey,
					model: config.modelId,
					prompt,
					inputImage,
				})
			} else {
				result = await generateImageWithProvider({
					baseURL: baseUrl,
					authToken: apiKey,
					model: config.modelId,
					prompt,
					inputImage,
				})
			}

			if (!result.success) {
				throw new Error(result.error || "Unknown error during image generation")
			}

			// Save the image
			if (result.imageData) {
				// Extract base64 data from data URL if needed
				let base64Data = result.imageData
				if (base64Data.startsWith("data:image/")) {
					const matches = base64Data.match(/^data:image\/\w+;base64,(.+)$/)
					if (matches && matches[1]) {
						base64Data = matches[1]
					}
				}

				const buffer = Buffer.from(base64Data, "base64")
				await fs.writeFile(absolutePath, buffer)

				const successMsg = t("tools:generateImage.success", {
					path: getReadablePath(relPath, task.cwd),
				})
				pushToolResult(successMsg)
			} else {
				throw new Error("No image data returned from API")
			}
		} catch (error) {
			const errorMsg = error instanceof Error ? error.message : String(error)
			pushToolResult(formatResponse.toolError(errorMsg))
		}
	}

	/**
	 * Get the current image generation configuration
	 */
	private async getImageGenerationConfig(task: Task): Promise<ImageGenerationConfig | null> {
		const provider = task.providerRef.deref()
		if (!provider) {
			return null
		}

		const stateValues = provider.contextProxy.getValues()
		const configName = stateValues.currentImageGenerationConfigName

		if (!configName) {
			return null
		}

		const configMeta = stateValues.listImageGenerationConfigMeta?.find(
			(c: { name: string }) => c.name === configName,
		)

		if (!configMeta) {
			return null
		}

		// Build full config from metadata
		// Note: In a full implementation, we would load the full config from storage
		// For now, we'll use the metadata and get API key from secrets
		return {
			id: configMeta.id,
			name: configMeta.name,
			provider: configMeta.provider as ImageGenerationProvider,
			modelId: configMeta.modelId || "",
		}
	}

	/**
	 * Get API key from secret storage
	 */
	private async getApiKey(task: Task, config: ImageGenerationConfig): Promise<string | null> {
		if (!config.id) {
			return null
		}

		const provider = task.providerRef.deref()
		if (!provider) {
			return null
		}

		const secretKey = `imageGenerationApiKey_${config.id}`
		const apiKey = await provider.contextProxy.getSecret(secretKey)

		return apiKey || null
	}
}

// Export singleton instance
export const generateImageTool = new GenerateImageTool()
