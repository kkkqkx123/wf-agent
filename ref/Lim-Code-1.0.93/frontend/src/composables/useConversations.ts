/**
 * useConversations - 对话列表管理 Composable
 * 
 * 功能：
 * - 获取对话列表
 * - 创建新对话
 * - 删除对话
 * - 切换对话
 */

import { ref, computed, onMounted } from 'vue'
import { sendToExtension } from '../utils/vscode'
import type { ErrorInfo } from '../types'
import { useI18n } from './useI18n'

/**
 * 对话摘要信息
 */
export interface ConversationSummary {
  id: string
  title: string
  createdAt: number
  updatedAt: number
  messageCount: number
  preview?: string
}

export function useConversations() {
  // i18n
  const { t } = useI18n()
  
  // 状态
  const conversations = ref<ConversationSummary[]>([])
  const currentConversationId = ref<string | null>(null)
  const isLoading = ref(false)
  const error = ref<ErrorInfo | null>(null)

  // 计算属性
  const hasConversations = computed(() => conversations.value.length > 0)
  
  const currentConversation = computed(() => 
    conversations.value.find(c => c.id === currentConversationId.value) || null
  )

  const sortedConversations = computed(() => 
    [...conversations.value].sort((a, b) => b.updatedAt - a.updatedAt)
  )

  /**
   * 加载对话列表
   */
  async function loadConversations(): Promise<void> {
    isLoading.value = true
    error.value = null

    try {
      const ids = await sendToExtension<string[]>('conversation.listConversations', {})
      
      // 获取每个对话的元数据
      const summaries: ConversationSummary[] = []
      for (const id of ids) {
        try {
          const metadata = await sendToExtension<any>('conversation.getConversationMetadata', { conversationId: id })
          summaries.push({
            id,
            title: metadata?.title || `${t('composables.useConversations.defaultTitle')} ${id.slice(0, 8)}`,
            createdAt: metadata?.createdAt || Date.now(),
            updatedAt: metadata?.updatedAt || Date.now(),
            messageCount: 0,
            preview: metadata?.preview
          })
        } catch {
          // 如果获取元数据失败，使用默认值
          summaries.push({
            id,
            title: `${t('composables.useConversations.defaultTitle')} ${id.slice(0, 8)}`,
            createdAt: Date.now(),
            updatedAt: Date.now(),
            messageCount: 0
          })
        }
      }
      
      conversations.value = summaries
    } catch (err: any) {
      error.value = {
        code: err.code || 'LOAD_ERROR',
        message: err.message || t('composables.useConversations.errors.loadFailed')
      }
    } finally {
      isLoading.value = false
    }
  }

  /**
   * 创建新对话
   */
  async function createConversation(title?: string): Promise<string | null> {
    error.value = null

    try {
      const id = `conv_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`
      
      await sendToExtension('conversation.createConversation', { conversationId: id })
      
      // 设置标题
      if (title) {
        await sendToExtension('conversation.setMetadata', {
          conversationId: id,
          key: 'title',
          value: title
        })
      }

      const newConversation: ConversationSummary = {
        id,
        title: title || t('composables.useConversations.newChatTitle'),
        createdAt: Date.now(),
        updatedAt: Date.now(),
        messageCount: 0
      }

      conversations.value.unshift(newConversation)
      currentConversationId.value = id

      return id
    } catch (err: any) {
      error.value = {
        code: err.code || 'CREATE_ERROR',
        message: err.message || t('composables.useConversations.errors.createFailed')
      }
      return null
    }
  }

  /**
   * 删除对话
   */
  async function deleteConversation(id: string): Promise<boolean> {
    error.value = null

    try {
      await sendToExtension('conversation.deleteConversation', { conversationId: id })
      
      conversations.value = conversations.value.filter(c => c.id !== id)
      
      // 如果删除的是当前对话，切换到第一个或创建新的
      if (currentConversationId.value === id) {
        if (conversations.value.length > 0) {
          currentConversationId.value = conversations.value[0].id
        } else {
          currentConversationId.value = null
        }
      }

      return true
    } catch (err: any) {
      error.value = {
        code: err.code || 'DELETE_ERROR',
        message: err.message || t('composables.useConversations.errors.deleteFailed')
      }
      return false
    }
  }

  /**
   * 切换对话
   */
  function switchConversation(id: string): void {
    if (conversations.value.some(c => c.id === id)) {
      currentConversationId.value = id
    }
  }

  /**
   * 更新对话标题
   */
  async function updateConversationTitle(id: string, title: string): Promise<boolean> {
    error.value = null

    try {
      await sendToExtension('conversation.setMetadata', {
        conversationId: id,
        key: 'title',
        value: title
      })

      const conv = conversations.value.find(c => c.id === id)
      if (conv) {
        conv.title = title
        conv.updatedAt = Date.now()
      }

      return true
    } catch (err: any) {
      error.value = {
        code: err.code || 'UPDATE_ERROR',
        message: err.message || t('composables.useConversations.errors.updateTitleFailed')
      }
      return false
    }
  }

  /**
   * 格式化时间
   */
  function formatTime(timestamp: number): string {
    const now = Date.now()
    const diff = now - timestamp
    
    const minute = 60 * 1000
    const hour = 60 * minute
    const day = 24 * hour
    
    if (diff < minute) {
      return t('composables.useConversations.relativeTime.justNow')
    } else if (diff < hour) {
      const minutes = Math.floor(diff / minute)
      return t('composables.useConversations.relativeTime.minutesAgo', { minutes })
    } else if (diff < day) {
      const hours = Math.floor(diff / hour)
      return t('composables.useConversations.relativeTime.hoursAgo', { hours })
    } else if (diff < 7 * day) {
      const days = Math.floor(diff / day)
      return t('composables.useConversations.relativeTime.daysAgo', { days })
    } else {
      return new Date(timestamp).toLocaleDateString()
    }
  }

  // 初始化时加载对话列表
  onMounted(() => {
    loadConversations()
  })

  return {
    // 状态
    conversations,
    currentConversationId,
    isLoading,
    error,
    hasConversations,
    currentConversation,
    sortedConversations,

    // 方法
    loadConversations,
    createConversation,
    deleteConversation,
    switchConversation,
    updateConversationTitle,
    formatTime
  }
}