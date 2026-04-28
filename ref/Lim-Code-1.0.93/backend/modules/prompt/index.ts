/**
 * LimCode - 系统提示词模块
 * 
 * 导出系统提示词管理器和相关类型
 */

export { PromptManager, getPromptManager, setPromptManager } from './PromptManager'
export { getWorkspaceFileTree, getWorkspaceRoot } from './fileTree'
export type { PromptConfig, PromptContext, PromptSection } from './types'
export { DEFAULT_PROMPT_CONFIG } from './types'