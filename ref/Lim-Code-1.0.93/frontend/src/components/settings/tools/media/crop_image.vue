<script setup lang="ts">
/**
 * 裁切图片工具设置面板
 */
import MediaToolConfig from './MediaToolConfig.vue'
import { t } from '@/i18n'
</script>

<template>
  <MediaToolConfig
    tool-name="crop_image"
    :display-name="t('components.settings.toolSettings.media.cropImage.title')"
    icon="codicon-symbol-keyword"
    :description="t('components.settings.toolSettings.media.cropImage.description')"
  />
</template>