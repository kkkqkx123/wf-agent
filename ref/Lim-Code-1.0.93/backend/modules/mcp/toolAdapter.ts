/**
 * LimCode - MCP 工具适配器
 *
 * 将 MCP 工具转换为内置工具格式，支持 XML/JSON/Function Call
 */

import type { ToolDeclaration, Tool, ToolResult, MultimodalData } from '../../tools/types';
import type { McpToolDefinition, McpToolCallResult } from './types';

/**
 * MCP 工具参数 JSON Schema
 */
export interface McpToolSchema {
    type: 'object';
    properties?: Record<string, any>;
    required?: string[];
    [key: string]: any;
}

/**
 * 将 MCP 工具定义转换为 ToolDeclaration
 *
 * @param tool MCP 工具定义
 * @param serverId 服务器 ID（用于区分不同服务器的工具）
 * @returns 标准工具声明
 */
export function mcpToolToDeclaration(
    tool: McpToolDefinition,
    serverId: string
): ToolDeclaration {
    // MCP 工具名称格式：mcp_<serverId>_<toolName>
    // 或者简化为：mcp_<toolName>（如果只有一个服务器）
    const toolName = `mcp_${serverId}_${tool.name}`;

    // 将 MCP 的 inputSchema 转换为 ToolDeclaration 的 parameters
    const parameters = convertInputSchemaToParameters(tool.inputSchema);

    return {
        name: toolName,
        description: tool.description || `MCP Tool: ${tool.name}`,
        category: 'mcp',
        parameters
    };
}

/**
 * 将 MCP inputSchema 转换为 ToolDeclaration parameters
 */
function convertInputSchemaToParameters(inputSchema?: McpToolSchema): ToolDeclaration['parameters'] {
    if (!inputSchema) {
        return {
            type: 'object',
            properties: {},
            required: []
        };
    }

    return {
        type: 'object',
        properties: inputSchema.properties || {},
        required: inputSchema.required || []
    };
}

/**
 * 将 MCP 工具调用结果转换为 ToolResult
 *
 * MCP 支持返回多种内容类型：
 * - TextContent: { type: 'text', text: string }
 * - ImageContent: { type: 'image', data: string, mimeType: string }
 * - EmbeddedResource: { type: 'resource', uri: string, ... }
 *
 * @param mcpResult MCP 工具调用结果
 * @returns 标准工具结果
 */
export function mcpResultToToolResult(mcpResult: McpToolCallResult): ToolResult {
    // 处理错误情况
    if (mcpResult.isError || !mcpResult.success) {
        const errorText = mcpResult.error ||
            mcpResult.content
                ?.filter(c => c.type === 'text')
                .map(c => c.text)
                .join('\n') ||
            'Unknown error';
        
        return {
            success: false,
            error: errorText
        };
    }

    // 处理成功响应
    const textContents: string[] = [];
    const multimodalData: MultimodalData[] = [];

    if (mcpResult.content) {
        for (const content of mcpResult.content) {
            switch (content.type) {
                case 'text':
                    if (content.text) {
                        textContents.push(content.text);
                    }
                    break;

                case 'image':
                    // MCP 图片内容
                    if (content.data) {
                        multimodalData.push({
                            mimeType: content.mimeType || 'image/png',
                            data: content.data,
                            name: content.uri
                        });
                    }
                    break;

                case 'resource':
                    // 嵌入资源 - 可能包含文本或二进制数据
                    if (content.text) {
                        textContents.push(content.text);
                    } else if (content.data) {
                        multimodalData.push({
                            mimeType: content.mimeType || 'application/octet-stream',
                            data: content.data,
                            name: content.uri
                        });
                    }
                    break;
            }
        }
    }

    return {
        success: true,
        data: textContents.length > 0 ? textContents.join('\n') : undefined,
        multimodal: multimodalData.length > 0 ? multimodalData : undefined
    };
}

/**
 * 从工具名称中提取 MCP 服务器 ID 和原始工具名
 *
 * @param toolName 完整工具名称（如 mcp_server1_read_file）
 * @returns [serverId, originalToolName] 或 null
 */
export function parseMcpToolName(toolName: string): [string, string] | null {
    if (!toolName.startsWith('mcp_')) {
        return null;
    }

    // 格式：mcp_<serverId>_<toolName>
    const parts = toolName.substring(4).split('_');
    if (parts.length < 2) {
        return null;
    }

    const serverId = parts[0];
    const originalToolName = parts.slice(1).join('_');

    return [serverId, originalToolName];
}

/**
 * 创建 MCP 工具的 Tool 对象（用于注册到 ToolRegistry）
 *
 * @param tool MCP 工具定义
 * @param serverId 服务器 ID
 * @param mcpManager MCP 管理器实例
 * @returns Tool 对象
 */
export function createMcpTool(
    tool: McpToolDefinition,
    serverId: string,
    callTool: (serverId: string, toolName: string, args: Record<string, any>) => Promise<McpToolCallResult>
): Tool {
    const declaration = mcpToolToDeclaration(tool, serverId);

    return {
        declaration,
        handler: async (args) => {
            try {
                const result = await callTool(serverId, tool.name, args);
                return mcpResultToToolResult(result);
            } catch (error) {
                return {
                    success: false,
                    error: error instanceof Error ? error.message : String(error)
                };
            }
        }
    };
}

/**
 * 批量将 MCP 工具转换为 ToolDeclaration
 *
 * @param tools MCP 工具列表
 * @param serverId 服务器 ID
 * @returns 工具声明数组
 */
export function mcpToolsToDeclarations(
    tools: McpToolDefinition[],
    serverId: string
): ToolDeclaration[] {
    return tools.map(tool => mcpToolToDeclaration(tool, serverId));
}

/**
 * 从多个 MCP 服务器收集所有工具声明
 *
 * @param serversTools 服务器 ID 到工具列表的映射
 * @returns 所有工具声明数组
 */
export function collectAllMcpToolDeclarations(
    serversTools: Map<string, McpToolDefinition[]>
): ToolDeclaration[] {
    const declarations: ToolDeclaration[] = [];

    for (const [serverId, tools] of serversTools) {
        declarations.push(...mcpToolsToDeclarations(tools, serverId));
    }

    return declarations;
}

/**
 * 生成 MCP 工具的简短名称（不带服务器 ID）
 * 用于当只有一个 MCP 服务器时简化工具名称
 *
 * @param toolName 原始工具名称
 * @returns 简短工具名称
 */
export function mcpToolSimpleName(toolName: string): string {
    return `mcp_${toolName}`;
}

/**
 * MCP 工具注册选项
 */
export interface McpToolRegistrationOptions {
    /** 是否使用简短名称（省略服务器 ID） */
    useSimpleName?: boolean;
    /** 工具名称前缀 */
    prefix?: string;
}

/**
 * 将 MCP 工具定义转换为标准工具声明（带选项）
 */
export function mcpToolToDeclarationWithOptions(
    tool: McpToolDefinition,
    serverId: string,
    options: McpToolRegistrationOptions = {}
): ToolDeclaration {
    const { useSimpleName = false, prefix = 'mcp' } = options;

    // 根据选项决定工具名称格式
    const toolName = useSimpleName
        ? `${prefix}_${tool.name}`
        : `${prefix}_${serverId}_${tool.name}`;

    const parameters = convertInputSchemaToParameters(tool.inputSchema);

    return {
        name: toolName,
        description: tool.description || `MCP Tool: ${tool.name}`,
        category: 'mcp',
        parameters
    };
}