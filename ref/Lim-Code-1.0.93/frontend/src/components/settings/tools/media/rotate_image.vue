<script setup lang="ts">
/**
 * 旋转图片工具设置面板
 */
import MediaToolConfig from './MediaToolConfig.vue'
import { t } from '@/i18n'
</script>

<template>
  <MediaToolConfig
    tool-name="rotate_image"
    :display-name="t('components.settings.toolSettings.media.rotateImage.title')"
    icon="codicon-sync"
    :description="t('components.settings.toolSettings.media.rotateImage.description')"
  />
</template>