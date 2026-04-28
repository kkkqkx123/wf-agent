/**
 * 搜索工具模块
 *
 * 导出所有搜索相关的工具
 */

import type { Tool } from '../types';

// 导出各个工具的创建函数
export { registerSearchInFiles } from './search_in_files';
export { registerFindFiles } from './find_files';

/**
 * 获取所有搜索工具
 * @returns 所有搜索工具的数组
 */
export function getAllSearchTools(): Tool[] {
    const { registerSearchInFiles } = require('./search_in_files');
    const { registerFindFiles } = require('./find_files');
    
    return [
        registerSearchInFiles(),
        registerFindFiles()
    ];
}

/**
 * 获取所有搜索工具的注册函数
 * @returns 注册函数数组
 */
export function getSearchToolRegistrations() {
    const { registerSearchInFiles } = require('./search_in_files');
    const { registerFindFiles } = require('./find_files');
    
    return [
        registerSearchInFiles,
        registerFindFiles
    ];
}