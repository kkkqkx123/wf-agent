<script setup lang="ts">
/**
 * 自定义勾选框组件
 * 支持 v-model 双向绑定
 */

defineProps<{
  modelValue: boolean
  label?: string
  hint?: string
  disabled?: boolean
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
}>()

function toggle(event: Event) {
  const target = event.target as HTMLInputElement
  emit('update:modelValue', target.checked)
}
</script>

<template>
  <div class="checkbox-wrapper">
    <label :class="['custom-checkbox', { disabled }]">
      <input
        type="checkbox"
        :checked="modelValue"
        :disabled="disabled"
        @change="toggle"
      />
      <span class="checkmark"></span>
      <span v-if="label" class="checkbox-text">{{ label }}</span>
    </label>
    <span v-if="hint" class="checkbox-hint">{{ hint }}</span>
  </div>
</template>

<style scoped>
.checkbox-wrapper {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.custom-checkbox {
  display: flex;
  align-items: center;
  cursor: pointer;
  font-size: 13px;
  font-weight: normal;
  position: relative;
  padding-left: 26px;
  user-select: none;
}

.custom-checkbox.disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.custom-checkbox input {
  position: absolute;
  opacity: 0;
  cursor: pointer;
  height: 0;
  width: 0;
}

.custom-checkbox .checkmark {
  position: absolute;
  left: 0;
  top: 50%;
  transform: translateY(-50%);
  height: 16px;
  width: 16px;
  background: var(--vscode-input-background);
  border: 1.5px solid var(--vscode-foreground);
  border-radius: 3px;
  transition: all 0.15s;
  opacity: 0.6;
}

.custom-checkbox:hover:not(.disabled) .checkmark {
  opacity: 1;
}

.custom-checkbox:focus-within .checkmark {
  border-color: var(--vscode-focusBorder);
  opacity: 1;
}

.custom-checkbox input:checked ~ .checkmark {
  background: var(--vscode-button-background);
  border-color: var(--vscode-button-background);
}

.custom-checkbox .checkmark::after {
  content: '';
  position: absolute;
  display: none;
  left: 50%;
  top: 50%;
  width: 4px;
  height: 8px;
  border: solid var(--vscode-button-foreground);
  border-width: 0 2px 2px 0;
  transform: translate(-50%, -60%) rotate(45deg);
}

.custom-checkbox input:checked ~ .checkmark::after {
  display: block;
}

.checkbox-text {
  margin-left: 4px;
}

.checkbox-hint {
  font-size: 11px;
  color: var(--vscode-descriptionForeground);
  margin-left: 26px;
}
</style>