<script setup lang="ts">
/**
 * 抠图工具设置面板
 */
import MediaToolConfig from './MediaToolConfig.vue'
import { t } from '@/i18n'
</script>

<template>
  <MediaToolConfig
    tool-name="remove_background"
    :display-name="t('components.settings.toolSettings.media.removeBackground.title')"
    icon="codicon-paintcan"
    :description="t('components.settings.toolSettings.media.removeBackground.description')"
  />
</template>