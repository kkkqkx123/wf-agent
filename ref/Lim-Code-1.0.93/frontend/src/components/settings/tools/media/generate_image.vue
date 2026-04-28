<script setup lang="ts">
/**
 * 图像生成工具设置面板
 */
import MediaToolConfig from './MediaToolConfig.vue'
import { t } from '@/i18n'
</script>

<template>
  <MediaToolConfig
    tool-name="generate_image"
    :display-name="t('components.settings.toolSettings.media.generateImage.title')"
    icon="codicon-sparkle"
    :description="t('components.settings.toolSettings.media.generateImage.description')"
  />
</template>