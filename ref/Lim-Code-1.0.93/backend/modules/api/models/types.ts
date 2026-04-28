/**
 * 模型管理 API 类型定义
 */

import type { ModelInfo } from '../../channel/modelList';

// ========== 获取可用模型列表 ==========

export interface GetModelsRequest {
    configId: string;
}

export interface GetModelsResponse {
    success: boolean;
    models?: ModelInfo[];
    error?: string;
}

// ========== 添加模型到配置 ==========

export interface AddModelsRequest {
    configId: string;
    models: ModelInfo[];
}

export interface AddModelsResponse {
    success: boolean;
    error?: string;
}

// ========== 从配置移除模型 ==========

export interface RemoveModelRequest {
    configId: string;
    modelId: string;
}

export interface RemoveModelResponse {
    success: boolean;
    error?: string;
}

// ========== 设置当前激活模型 ==========

export interface SetActiveModelRequest {
    configId: string;
    modelId: string;
}

export interface SetActiveModelResponse {
    success: boolean;
    error?: string;
}