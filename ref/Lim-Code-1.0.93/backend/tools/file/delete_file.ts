/**
 * 删除文件/目录工具
 *
 * 支持删除单个或多个文件和目录（包括非空目录）
 * 支持多工作区（Multi-root Workspaces）
 */

import * as vscode from 'vscode';
import type { Tool, ToolResult } from '../types';
import { resolveUri, getAllWorkspaces } from '../utils';

/**
 * 删除结果
 */
interface DeleteResult {
    path: string;
    success: boolean;
    error?: string;
}

/**
 * 创建删除文件工具
 */
export function createDeleteFileTool(): Tool {
    // 获取工作区信息
    const workspaces = getAllWorkspaces();
    const isMultiRoot = workspaces.length > 1;
    
    // 数组格式强调说明
    const arrayFormatNote = '\n\n**IMPORTANT**: The `paths` parameter MUST be an array, even for a single file. Example: `{"paths": ["file.txt"]}`, NOT `{"path": "file.txt"}`.';
    
    // 根据工作区数量生成描述
    let description = 'Delete one or more files or directories. Supports deleting non-empty directories.' + arrayFormatNote;
    let pathsDescription = 'Array of file or directory paths to delete (relative to workspace root). MUST be an array even for single file, e.g., ["file.txt"]';

    if (isMultiRoot) {
        description += `\n\nMulti-root workspace: Must use "workspace_name/path" format. Available workspaces: ${workspaces.map(w => w.name).join(', ')}`;
        pathsDescription = `Array of file or directory paths to delete, must use "workspace_name/path" format. MUST be an array even for single file.`;
    }
    
    return {
        declaration: {
            name: 'delete_file',
            description,
            category: 'file',
            parameters: {
                type: 'object',
                properties: {
                    paths: {
                        type: 'array',
                        items: {
                            type: 'string'
                        },
                        description: pathsDescription
                    }
                },
                required: ['paths']
            }
        },
        handler: async (args): Promise<ToolResult> => {
            const pathList = args.paths as string[];
            if (!pathList || !Array.isArray(pathList) || pathList.length === 0) {
                return { success: false, error: 'paths is required' };
            }

            const results: DeleteResult[] = [];
            let successCount = 0;
            let failCount = 0;

            for (const filePath of pathList) {
                const uri = resolveUri(filePath);
                if (!uri) {
                    results.push({
                        path: filePath,
                        success: false,
                        error: 'No workspace folder open'
                    });
                    failCount++;
                    continue;
                }

                try {
                    // 使用 recursive: true 支持删除非空目录
                    await vscode.workspace.fs.delete(uri, { recursive: true });
                    results.push({
                        path: filePath,
                        success: true
                    });
                    successCount++;
                } catch (error) {
                    results.push({
                        path: filePath,
                        success: false,
                        error: error instanceof Error ? error.message : String(error)
                    });
                    failCount++;
                }
            }

            // 返回简洁的结果消息
            const allSuccess = failCount === 0;
            const deletedPaths = results.filter(r => r.success).map(r => r.path);
            const failedPaths = results.filter(r => !r.success).map(r => `${r.path}: ${r.error}`);
            
            let message: string;
            if (allSuccess) {
                message = `Deleted: ${deletedPaths.join(', ')}`;
            } else if (successCount > 0) {
                message = `Deleted: ${deletedPaths.join(', ')}\nFailed: ${failedPaths.join(', ')}`;
            } else {
                message = `Delete failed: ${failedPaths.join(', ')}`;
            }

            return {
                success: allSuccess,
                data: {
                    message,
                    deletedPaths,
                    failedPaths: results.filter(r => !r.success).map(r => r.path)
                },
                error: allSuccess ? undefined : `${failCount} deletions failed`
            };
        }
    };
}

/**
 * 注册删除文件工具
 */
export function registerDeleteFile(): Tool {
    return createDeleteFileTool();
}