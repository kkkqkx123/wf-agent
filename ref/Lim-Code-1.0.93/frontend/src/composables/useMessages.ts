/**
 * useMessages - 消息管理 Composable
 * 
 * 功能：
 * - 消息格式化
 * - 消息搜索
 * - 消息过滤
 * - 消息统计
 */

import { computed } from 'vue'
import type { Message, ContentPart } from '../types'
import { formatTime, formatRelativeTime } from '../utils/format'

export function useMessages(messages: Message[]) {
  /**
   * 用户消息数量
   */
  const userMessageCount = computed(() => 
    messages.filter(m => m.role === 'user').length
  )

  /**
   * AI 消息数量
   */
  const assistantMessageCount = computed(() => 
    messages.filter(m => m.role === 'assistant').length
  )

  /**
   * 总消息数
   */
  const totalMessageCount = computed(() => messages.length)

  /**
   * 是否有思考内容
   */
  const hasThoughts = computed(() => 
    messages.some(m => m.parts?.some(p => p.thought))
  )

  /**
   * 是否有附件
   */
  const hasAttachments = computed(() => 
    messages.some(m => m.attachments && m.attachments.length > 0)
  )

  /**
   * Token 统计
   */
  const tokenStats = computed(() => {
    let totalThoughts = 0
    let totalCandidates = 0

    messages.forEach(m => {
      if (m.metadata?.thoughtsTokenCount) {
        totalThoughts += m.metadata.thoughtsTokenCount
      }
      if (m.metadata?.candidatesTokenCount) {
        totalCandidates += m.metadata.candidatesTokenCount
      }
    })

    return {
      thoughts: totalThoughts,
      candidates: totalCandidates,
      total: totalThoughts + totalCandidates
    }
  })

  /**
   * 格式化消息时间
   */
  function formatMessageTime(message: Message, relative = false): string {
    if (relative) {
      return formatRelativeTime(message.timestamp)
    }
    return formatTime(message.timestamp, 'HH:mm')
  }

  /**
   * 获取消息文本内容（排除思考部分）
   */
  function getMessageText(message: Message): string {
    if (!message.parts) {
      return message.content
    }

    const textParts = message.parts
      .filter(p => p.text && !p.thought)
      .map(p => p.text)
      .join('\n')

    return textParts || message.content
  }

  /**
   * 获取消息的思考内容
   */
  function getMessageThoughts(message: Message): string[] {
    if (!message.parts) {
      return []
    }

    return message.parts
      .filter(p => p.text && p.thought)
      .map(p => p.text!)
  }

  /**
   * 检查消息是否包含特定内容
   */
  function messageContains(message: Message, query: string): boolean {
    const lowerQuery = query.toLowerCase()
    
    // 检查主要内容
    if (message.content.toLowerCase().includes(lowerQuery)) {
      return true
    }

    // 检查 parts 中的文本
    if (message.parts) {
      return message.parts.some(p => 
        p.text && p.text.toLowerCase().includes(lowerQuery)
      )
    }

    return false
  }

  /**
   * 搜索消息
   */
  function searchMessages(query: string): Message[] {
    if (!query.trim()) {
      return messages
    }

    return messages.filter(m => messageContains(m, query))
  }

  /**
   * 按角色过滤消息
   */
  function filterByRole(role: 'user' | 'assistant'): Message[] {
    return messages.filter(m => m.role === role)
  }

  /**
   * 获取消息的多模态内容
   */
  function getMultimediaContent(message: Message): ContentPart[] {
    if (!message.parts) {
      return []
    }

    return message.parts.filter(p => 
      p.inlineData || p.fileData
    )
  }

  /**
   * 检查消息是否包含图片
   */
  function hasImages(message: Message): boolean {
    if (message.attachments?.some(a => a.type === 'image')) {
      return true
    }

    if (!message.parts) {
      return false
    }

    return message.parts.some(p => {
      if (p.inlineData) {
        return p.inlineData.mimeType.startsWith('image/')
      }
      if (p.fileData) {
        return p.fileData.mimeType.startsWith('image/')
      }
      return false
    })
  }

  /**
   * 检查消息是否包含函数调用
   */
  function hasFunctionCalls(message: Message): boolean {
    if (!message.parts) {
      return false
    }

    return message.parts.some(p => p.functionCall || p.functionResponse)
  }

  /**
   * 提取消息中的代码块
   */
  function extractCodeBlocks(message: Message): Array<{ language: string; code: string }> {
    const text = getMessageText(message)
    const codeBlockRegex = /```(\w+)?\n([\s\S]*?)```/g
    const blocks: Array<{ language: string; code: string }> = []
    
    let match
    while ((match = codeBlockRegex.exec(text)) !== null) {
      blocks.push({
        language: match[1] || 'text',
        code: match[2].trim()
      })
    }

    return blocks
  }

  /**
   * 检查消息是否为流式中
   */
  function isStreaming(message: Message): boolean {
    return message.streaming === true
  }

  /**
   * 获取消息索引
   */
  function getMessageIndex(messageId: string): number {
    return messages.findIndex(m => m.id === messageId)
  }

  /**
   * 获取上一条消息
   */
  function getPreviousMessage(messageId: string): Message | null {
    const index = getMessageIndex(messageId)
    if (index > 0) {
      return messages[index - 1]
    }
    return null
  }

  /**
   * 获取下一条消息
   */
  function getNextMessage(messageId: string): Message | null {
    const index = getMessageIndex(messageId)
    if (index >= 0 && index < messages.length - 1) {
      return messages[index + 1]
    }
    return null
  }

  return {
    // 统计信息
    userMessageCount,
    assistantMessageCount,
    totalMessageCount,
    hasThoughts,
    hasAttachments,
    tokenStats,

    // 格式化方法
    formatMessageTime,
    getMessageText,
    getMessageThoughts,
    getMultimediaContent,
    extractCodeBlocks,

    // 查询方法
    messageContains,
    searchMessages,
    filterByRole,
    hasImages,
    hasFunctionCalls,
    isStreaming,

    // 导航方法
    getMessageIndex,
    getPreviousMessage,
    getNextMessage
  }
}