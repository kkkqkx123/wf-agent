/**
 * 多媒体工具模块
 *
 * 导出所有多媒体相关的工具
 */

import type { Tool } from '../types';

// 导出各个工具的创建函数
export { registerGenerateImage, createGenerateImageTool } from './generate_image';
export { registerRemoveBackground, createRemoveBackgroundTool } from './remove_background';
export { registerCropImage, createCropImageTool } from './crop_image';
export { registerResizeImage, createResizeImageTool } from './resize_image';
export { registerRotateImage, createRotateImageTool } from './rotate_image';

// 导出图像生成管理函数（类似终端管理）
export {
    cancelImageGeneration,
    onImageGenOutput,
    generateToolId,
    getActiveImageTasks
} from './generate_image';
export type { ImageGenOutputEvent } from './generate_image';

// 导出抠图管理函数
export {
    cancelRemoveBackground,
    onRemoveBgOutput
} from './remove_background';
export type { RemoveBgOutputEvent } from './remove_background';

// 导出裁切管理函数
export {
    cancelCropImage,
    onCropImageOutput
} from './crop_image';
export type { CropImageOutputEvent } from './crop_image';

// 导出缩放管理函数
export {
    cancelResizeImage,
    onResizeImageOutput
} from './resize_image';
export type { ResizeImageOutputEvent } from './resize_image';

// 导出旋转管理函数
export {
    cancelRotateImage,
    onRotateImageOutput
} from './rotate_image';
export type { RotateImageOutputEvent } from './rotate_image';

/**
 * 获取所有多媒体工具
 * @returns 所有多媒体工具的数组
 */
export function getAllMediaTools(): Tool[] {
    const { registerGenerateImage } = require('./generate_image');
    const { registerRemoveBackground } = require('./remove_background');
    const { registerCropImage } = require('./crop_image');
    const { registerResizeImage } = require('./resize_image');
    const { registerRotateImage } = require('./rotate_image');
    
    return [
        registerGenerateImage(),
        registerRemoveBackground(),
        registerCropImage(),
        registerResizeImage(),
        registerRotateImage()
    ];
}

/**
 * 获取所有多媒体工具的注册函数
 * @returns 注册函数数组
 */
export function getMediaToolRegistrations() {
    const { registerGenerateImage } = require('./generate_image');
    const { registerRemoveBackground } = require('./remove_background');
    const { registerCropImage } = require('./crop_image');
    const { registerResizeImage } = require('./resize_image');
    const { registerRotateImage } = require('./rotate_image');
    
    return [
        registerGenerateImage,
        registerRemoveBackground,
        registerCropImage,
        registerResizeImage,
        registerRotateImage
    ];
}