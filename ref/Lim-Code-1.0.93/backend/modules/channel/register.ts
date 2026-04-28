/**
 * LimCode - 渠道调用模块注册
 * 
 * 注册渠道调用模块到 ModuleRegistry
 */

import type { ModuleDefinition } from '../../core/registry/types';
import type { ConfigManager } from '../config/ConfigManager';
import { ChannelManager } from './ChannelManager';
import type {
    GenerateRequest,
    GenerateResponse,
    StreamChunk
} from './types';

/**
 * 注册渠道调用模块
 * 
 * @param configManager 配置管理器
 * @returns 模块定义
 */
export function registerChannelModule(
    configManager: ConfigManager
): ModuleDefinition {
    const manager = new ChannelManager(configManager);
    
    return {
        id: 'channel',
        name: 'Channel Manager',
        version: '1.0.5',
        description: '管理 LLM 渠道调用，支持 Gemini、OpenAI、Anthropic 等多种格式',
        
        apis: [
            // ========== 生成操作 ==========
            
            {
                name: 'generate',
                description: '生成内容（非流式）',
                parameters: [
                    {
                        name: 'request',
                        type: 'object',
                        required: true,
                        description: '生成请求'
                    }
                ],
                returnType: 'GenerateResponse',
                handler: async (params) => {
                    return await manager.generate(params.request as GenerateRequest);
                }
            },
            
            {
                name: 'generateStream',
                description: '生成内容（流式）',
                parameters: [
                    {
                        name: 'request',
                        type: 'object',
                        required: true,
                        description: '生成请求'
                    }
                ],
                returnType: 'AsyncGenerator<StreamChunk>',
                handler: async (params) => {
                    return manager.generateStream(params.request as GenerateRequest);
                }
            }
        ]
    };
}