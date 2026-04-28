/**
 * 查找文件工具
 *
 * 支持单个或多个 glob 模式查找
 * 支持多工作区（Multi-root Workspaces）
 */

import * as vscode from 'vscode';
import type { Tool, ToolResult } from '../types';
import { getWorkspaceRoot, getAllWorkspaces, toRelativePath } from '../utils';
import { getGlobalSettingsManager } from '../../core/settingsContext';

/**
 * 默认排除模式
 */
const DEFAULT_EXCLUDE = '**/node_modules/**';

/**
 * 获取排除模式
 *
 * 从设置管理器获取用户配置的排除模式，如果未配置则使用默认值
 * 将多个模式合并为单个 glob 模式（用大括号语法）
 */
function getExcludePattern(): string {
    const settingsManager = getGlobalSettingsManager();
    if (settingsManager) {
        const config = settingsManager.getFindFilesConfig();
        if (config.excludePatterns && config.excludePatterns.length > 0) {
            // 多个模式用 {} 语法组合
            if (config.excludePatterns.length === 1) {
                return config.excludePatterns[0];
            }
            return `{${config.excludePatterns.join(',')}}`;
        }
    }
    return DEFAULT_EXCLUDE;
}

/**
 * 单个模式的查找结果
 */
interface FindResult {
    pattern: string;
    workspace?: string;
    success: boolean;
    files?: string[];
    count?: number;
    truncated?: boolean;
    error?: string;
}

/**
 * 在单个工作区中执行模式查找
 */
async function findInWorkspace(
    workspace: { name: string; uri: vscode.Uri },
    pattern: string,
    exclude: string,
    maxResults: number,
    includeWorkspacePrefix: boolean
): Promise<FindResult> {
    try {
        // 创建相对于工作区的模式
        const relativePattern = new vscode.RelativePattern(workspace.uri, pattern);
        const files = await vscode.workspace.findFiles(relativePattern, exclude, maxResults);
        
        const relativePaths = files
            .map((f: vscode.Uri) => toRelativePath(f, includeWorkspacePrefix))
            .sort();

        return {
            pattern,
            workspace: includeWorkspacePrefix ? workspace.name : undefined,
            success: true,
            files: relativePaths,
            count: relativePaths.length,
            truncated: relativePaths.length >= maxResults
        };
    } catch (error) {
        return {
            pattern,
            workspace: includeWorkspacePrefix ? workspace.name : undefined,
            success: false,
            error: error instanceof Error ? error.message : String(error)
        };
    }
}

/**
 * 执行单个模式的查找（支持多工作区）
 */
async function findWithPattern(
    pattern: string,
    exclude: string,
    maxResults: number
): Promise<FindResult> {
    const workspaces = getAllWorkspaces();
    if (workspaces.length === 0) {
        return {
            pattern,
            success: false,
            error: 'No workspace folder open'
        };
    }
    
    // 单工作区模式
    if (workspaces.length === 1) {
        return findInWorkspace(workspaces[0], pattern, exclude, maxResults, false);
    }
    
    // 多工作区模式：在所有工作区中查找
    const allFiles: string[] = [];
    let truncated = false;
    
    for (const ws of workspaces) {
        if (allFiles.length >= maxResults) {
            truncated = true;
            break;
        }
        
        const remaining = maxResults - allFiles.length;
        const result = await findInWorkspace(ws, pattern, exclude, remaining, true);
        
        if (result.success && result.files) {
            allFiles.push(...result.files);
        }
    }
    
    return {
        pattern,
        success: true,
        files: allFiles.sort(),
        count: allFiles.length,
        truncated: truncated || allFiles.length >= maxResults
    };
}

/**
 * 创建查找文件工具
 */
export function createFindFilesTool(): Tool {
    const workspaces = getAllWorkspaces();
    const isMultiRoot = workspaces.length > 1;
    
    return {
        declaration: {
            name: 'find_files',
            description: isMultiRoot
                ? `Find files in multiple workspaces based on one or more glob patterns. Results include workspace prefixes. Available workspaces: ${workspaces.map(w => w.name).join(', ')}\n\n**IMPORTANT**: The \`patterns\` parameter MUST be an array, even for a single pattern. Example: \`{"patterns": ["*.ts"]}\`, NOT \`{"pattern": "*.ts"}\`.`
                : 'Find files based on one or more glob patterns.\n\n**IMPORTANT**: The `patterns` parameter MUST be an array, even for a single pattern. Example: `{"patterns": ["*.ts"]}`, NOT `{"pattern": "*.ts"}`.',
            category: 'search',
            parameters: {
                type: 'object',
                properties: {
                    patterns: {
                        type: 'array',
                        items: {
                            type: 'string'
                        },
                        description: 'Array of glob patterns to search for files. MUST be an array even for single pattern, e.g., ["**/*.ts", "src/**/*.js"]'
                    },
                    exclude: {
                        type: 'string',
                        description: 'Exclude pattern, e.g., "**/node_modules/**"',
                        default: '**/node_modules/**'
                    },
                    maxResults: {
                        type: 'number',
                        description: 'Maximum number of results per pattern',
                        default: 500
                    }
                },
                required: ['patterns']
            }
        },
        handler: async (args): Promise<ToolResult> => {
            // 支持 patterns 数组或单个 pattern（向后兼容）
            let patternList: string[] = [];
            
            if (args.patterns && Array.isArray(args.patterns)) {
                patternList = args.patterns as string[];
            } else if (args.pattern && typeof args.pattern === 'string') {
                // 向后兼容单个 pattern 参数
                patternList = [args.pattern];
            }
            
            if (patternList.length === 0) {
                return { success: false, error: 'patterns is required' };
            }

            // 如果用户指定了 exclude 参数则使用，否则使用配置的默认值
            const exclude = (args.exclude as string) || getExcludePattern();
            const maxResults = (args.maxResults as number) || 500;

            const results: FindResult[] = [];
            let successCount = 0;
            let failCount = 0;
            let totalFiles = 0;

            for (const pattern of patternList) {
                const result = await findWithPattern(pattern, exclude, maxResults);
                results.push(result);
                
                if (result.success) {
                    successCount++;
                    totalFiles += result.count || 0;
                } else {
                    failCount++;
                }
            }

            const allSuccess = failCount === 0;
            return {
                success: allSuccess,
                data: {
                    results,
                    successCount,
                    failCount,
                    totalCount: patternList.length,
                    totalFiles
                },
                error: allSuccess ? undefined : `${failCount} patterns failed to search`
            };
        }
    };
}

/**
 * 注册查找文件工具
 */
export function registerFindFiles(): Tool {
    return createFindFilesTool();
}
