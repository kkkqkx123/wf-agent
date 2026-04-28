import * as vscode from "vscode"
import path from "path"

import { Task } from "../task/Task"
import { CodeIndexManager } from "../../services/code-index/manager"
import { getWorkspacePath } from "../../utils/path"
import { formatResponse } from "../prompts/responses"
import { VectorStoreSearchResult } from "../../services/code-index/interfaces"
import type { ToolUse } from "../../shared/tools"
import type { ToolParamsMap } from "./schemas/registry"

import { BaseTool, ToolCallbacks } from "./core/BaseTool"

// Type alias for codebase_search parameters
type CodebaseSearchParams = ToolParamsMap["codebase_search"]

interface NormalizedQuery {
	query: string
	path?: string
}

interface AggregatedResult {
	id: string | number
	score: number
	payload?: any
	matchCount: number
	baseScore: number
	finalScore: number
}

export class CodebaseSearchTool extends BaseTool<"codebase_search"> {
	readonly name = "codebase_search" as const

	async execute(params: CodebaseSearchParams, task: Task, callbacks: ToolCallbacks): Promise<void> {
		const { askApproval, handleError, pushToolResult } = callbacks

		// 1. 参数标准化
		let normalizedQueries: NormalizedQuery[]
		try {
			normalizedQueries = this.normalizeParams(params)
		} catch (error) {
			task.didToolFailInCurrentTurn = true
			const err = error as Error
			task.recordToolError("codebase_search", err.message)
			pushToolResult(formatResponse.toolError(err.message))
			return
		}

		const workspacePath = task.cwd && task.cwd.trim() !== "" ? task.cwd : getWorkspacePath()

		if (!workspacePath) {
			await handleError("codebase_search", new Error("Could not determine workspace path."))
			return
		}

		// 准备审批消息
		const queryDescriptions = normalizedQueries.map((q) => ({
			query: q.query,
			path: q.path,
		}))

		const sharedMessageProps = {
			tool: "codebaseSearch",
			queries: queryDescriptions,
			isOutsideWorkspace: false,
		}

		const didApprove = await askApproval("tool", JSON.stringify(sharedMessageProps))
		if (!didApprove) {
			pushToolResult(formatResponse.toolDenied())
			return
		}

		try {
			const context = task.providerRef.deref()?.context
			if (!context) {
				throw new Error("Extension context is not available.")
			}

			const manager = CodeIndexManager.getInstance(context)

			if (!manager) {
				throw new Error("CodeIndexManager is not available.")
			}

			if (!manager.isFeatureEnabled) {
				throw new Error("Code Indexing is disabled in the settings.")
			}
			if (!manager.isFeatureConfigured) {
				throw new Error("Code Indexing is not configured (Missing OpenAI Key or Qdrant URL).")
			}

			// 2. 批量搜索
			const allResults: VectorStoreSearchResult[][] = []
			for (const normalizedQuery of normalizedQueries) {
				const results = await manager.searchIndex(normalizedQuery.query, normalizedQuery.path)
				allResults.push(results || [])
			}

			// 3. 聚合结果
			const aggregatedResults = this.aggregateResults(allResults, 20)

			if (aggregatedResults.length === 0) {
				const queryTexts = normalizedQueries.map((q) => q.query).join(", ")
				pushToolResult(`No relevant code snippets found for the queries: "${queryTexts}"`)
				return
			}

			// 4. 格式化结果
			const jsonResult = {
				queries: normalizedQueries.map((q) => ({ query: q.query, path: q.path })),
				results: [],
			} as {
				queries: Array<{ query: string; path?: string }>
				results: Array<{
					filePath: string
					score: number
					matchCount?: number
					startLine: number
					endLine: number
					codeChunk: string
				}>
			}

			aggregatedResults.forEach((result) => {
				if (!result.payload) return
				if (!("filePath" in result.payload)) return

				const relativePath = vscode.workspace.asRelativePath(result.payload.filePath, false)

				jsonResult.results.push({
					filePath: relativePath,
					score: result.finalScore,
					matchCount: result.matchCount,
					startLine: result.payload.startLine,
					endLine: result.payload.endLine,
					codeChunk: result.payload.codeChunk.trim(),
				})
			})

			const payload = { tool: "codebaseSearch", content: jsonResult }
			await task.say("codebase_search_result", JSON.stringify(payload))

			// 5. 生成输出
			const output = this.formatResult(aggregatedResults, normalizedQueries)
			pushToolResult(output)
		} catch (error: any) {
			await handleError("codebase_search", error)
		}
	}

	/**
	 * 参数标准化：将 queries 数组转换为标准化的查询对象数组
	 */
	private normalizeParams(params: CodebaseSearchParams): NormalizedQuery[] {
		// queries 是必需参数
		if (!params.queries || !Array.isArray(params.queries) || params.queries.length === 0) {
			throw new Error('Invalid parameters: "queries" must be a non-empty array')
		}

		return params.queries.map((q) => {
			if (typeof q === "string") {
				// 字符串数组格式
				return { query: q }
			} else {
				// 对象数组格式
				return { query: q.query, path: q.path }
			}
		})
	}

	/**
	 * 结果聚合：去重 + 多次匹配加成
	 * - 使用多次查询中最高的分数作为 baseScore
	 * - 对数增长加成，最高 50% 提升
	 */
	private aggregateResults(allResults: VectorStoreSearchResult[][], maxResults: number): AggregatedResult[] {
		const resultMap = new Map<string, AggregatedResult>()

		// 遍历所有查询结果
		allResults.forEach((queryResults) => {
			queryResults.forEach((result) => {
				const key = this.getResultKey(result)
				const existing = resultMap.get(key)

				if (existing) {
					// 已存在：更新匹配次数和得分
					existing.matchCount++
					// 更新 baseScore 为最高分数
					existing.baseScore = Math.max(existing.baseScore, result.score)
					// 对数加成：匹配次数越多，加成比例越小（递减增长）
					// 最高加成 30%，通过 Math.log2 实现对数增长，系数 0.2 控制增长速度
					const boost = Math.min(0.3, Math.log2(existing.matchCount) * 0.2)
					existing.finalScore = existing.baseScore * (1 + boost)
				} else {
					// 新结果
					resultMap.set(key, {
						id: result.id,
						score: result.score,
						payload: result.payload,
						matchCount: 1,
						baseScore: result.score,
						finalScore: result.score,
					})
				}
			})
		})

		// 按最终得分排序并限制数量
		return Array.from(resultMap.values())
			.sort((a, b) => b.finalScore - a.finalScore)
			.slice(0, maxResults)
	}

	/**
	 * 生成结果唯一标识
	 */
	private getResultKey(result: VectorStoreSearchResult): string {
		const payload = result.payload
		return `${payload?.filePath}:${payload?.startLine}-${payload?.endLine}`
	}

	/**
	 * 格式化输出结果
	 * 注意：匹配分数仅用于排序和UI显示，不添加到给LLM的提示词中
	 */
	private formatResult(aggregatedResults: AggregatedResult[], normalizedQueries: NormalizedQuery[]): string {
		const isBatchQuery = normalizedQueries.length > 1

		let output = ""

		if (isBatchQuery) {
			// 批量查询格式
			output += `Batch Query Results (${normalizedQueries.length} queries):\n`
			normalizedQueries.forEach((q, index) => {
				output += `- "${q.query}"`
				if (q.path) {
					output += ` (path: ${q.path})`
				}
				output += "\n"
			})
			output += "\nResults:\n\n"
		} else {
			// 单查询格式
			output += `Query: ${normalizedQueries[0]?.query}\nResults:\n\n`
		}

		aggregatedResults.forEach((result) => {
			const payload = result.payload
			const relativePath = vscode.workspace.asRelativePath(payload?.filePath, false)

			output += `File path: ${relativePath}\n`
			// 匹配分数仅用于内部排序，不显示在给LLM的提示词中
			output += `Lines: ${payload?.startLine}-${payload?.endLine}\n`
			output += `Code Chunk: ${payload?.codeChunk?.trim()}\n\n`
		})

		return output
	}

	override async handlePartial(task: Task, block: ToolUse<"codebase_search">): Promise<void> {
		// 尝试解析参数以获取查询信息
		let queryDisplay = ""

		try {
			// 优先使用 nativeArgs（如果可用）
			if ((block.nativeArgs as any)?.queries && Array.isArray((block.nativeArgs as any).queries)) {
				const queryTexts = (block.nativeArgs as any).queries.map((q: string | { query: string }) => {
					if (typeof q === "string") {
						return q
					}
					return q.query
				})
				queryDisplay = queryTexts.join(", ")
			} else {
				// 回退到 params（用于流式传输早期阶段）
				const queriesParam = block.params.queries
				if (queriesParam) {
					try {
						const queries = JSON.parse(queriesParam)
						if (Array.isArray(queries)) {
							const queryTexts = queries.map((q: string | { query: string }) => {
								if (typeof q === "string") {
									return q
								}
								return q.query
							})
							queryDisplay = queryTexts.join(", ")
						}
					} catch {
						queryDisplay = queriesParam
					}
				}
			}
		} catch {
			// 解析失败，使用原始参数
			queryDisplay = JSON.stringify(block.params)
		}

		const sharedMessageProps = {
			tool: "codebaseSearch",
			query: queryDisplay,
			isOutsideWorkspace: false,
		}

		await task.ask("tool", JSON.stringify(sharedMessageProps), block.partial).catch(() => { })
	}
}

export const codebaseSearchTool = new CodebaseSearchTool()
