<script setup lang="ts">
/**
 * 缩放图片工具设置面板
 */
import MediaToolConfig from './MediaToolConfig.vue'
import { t } from '@/i18n'
</script>

<template>
  <MediaToolConfig
    tool-name="resize_image"
    :display-name="t('components.settings.toolSettings.media.resizeImage.title')"
    icon="codicon-screen-full"
    :description="t('components.settings.toolSettings.media.resizeImage.description')"
  />
</template>