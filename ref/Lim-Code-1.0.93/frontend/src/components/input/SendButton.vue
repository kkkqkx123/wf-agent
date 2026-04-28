<script setup lang="ts">
/**
 * SendButton - 发送按钮
 * 使用纸飞机图标，扁平化设计
 * loading 状态下显示停止图标，点击可取消请求
 */

import { useI18n } from '../../i18n'

const { t } = useI18n()

defineProps<{
  disabled?: boolean
  loading?: boolean
}>()

const emit = defineEmits<{
  click: []
  cancel: []
}>()

// 处理点击
function handleClick() {
  emit('click')
}

// 处理取消
function handleCancel() {
  emit('cancel')
}
</script>

<template>
  <!-- 取消按钮 - loading 状态下显示 -->
  <button
    v-if="loading"
    class="send-button"
    :title="t('components.input.stopGenerating')"
    @click="handleCancel"
  >
    <i class="codicon codicon-primitive-square stop-icon"></i>
  </button>
  
  <!-- 发送按钮 - 正常状态下显示 -->
  <button
    v-else
    class="send-button"
    :disabled="disabled"
    :title="t('components.input.send')"
    @click="handleClick"
  >
    <i class="codicon codicon-send send-icon"></i>
  </button>
</template>

<style scoped>
.send-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  background: transparent;
  color: var(--vscode-foreground);
  border: none;
  border-radius: var(--radius-sm, 2px);
  cursor: pointer;
  transition: background-color var(--transition-fast, 0.1s), opacity var(--transition-fast, 0.1s);
  flex-shrink: 0;
}

.send-button:hover:not(:disabled) {
  background: var(--vscode-toolbar-hoverBackground);
}

.send-button:active:not(:disabled) {
  background: var(--vscode-toolbar-activeBackground);
}

.send-button:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

.send-icon {
  font-size: 16px;
}

.stop-icon {
  font-size: 25px;
}
</style>