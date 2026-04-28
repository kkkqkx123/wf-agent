/**
 * 列出文件工具
 *
 * 支持列出单个或多个目录，同时返回文件和子目录
 * 支持多工作区（Multi-root Workspaces）
 */

import * as vscode from 'vscode';
import * as path from 'path';
import type { Tool, ToolResult } from '../types';
import { getWorkspaceRoot, resolveUri, getAllWorkspaces, parseWorkspacePath, resolveUriWithInfo } from '../utils';
import { getGlobalSettingsManager } from '../../core/settingsContext';

/**
 * 默认忽略的目录和文件
 */
const DEFAULT_IGNORED = ['.git'];

/**
 * 获取忽略列表
 *
 * 从设置管理器获取用户配置的忽略列表，如果未配置则使用默认值
 */
function getIgnorePatterns(): string[] {
    const settingsManager = getGlobalSettingsManager();
    if (settingsManager) {
        const config = settingsManager.getListFilesConfig();
        return config.ignorePatterns || DEFAULT_IGNORED;
    }
    return DEFAULT_IGNORED;
}

/**
 * 检查是否应该忽略
 *
 * 支持通配符匹配：
 * - *.ext 匹配任意以 .ext 结尾的文件
 * - prefix* 匹配任意以 prefix 开头的文件
 * - 精确匹配
 */
function shouldIgnore(name: string, ignorePatterns: string[]): boolean {
    for (const pattern of ignorePatterns) {
        // 通配符匹配
        if (pattern.startsWith('*') && pattern.length > 1) {
            // *.ext 匹配
            const suffix = pattern.slice(1);
            if (name.endsWith(suffix)) {
                return true;
            }
        } else if (pattern.endsWith('*') && pattern.length > 1) {
            // prefix* 匹配
            const prefix = pattern.slice(0, -1);
            if (name.startsWith(prefix)) {
                return true;
            }
        } else {
            // 精确匹配
            if (name === pattern) {
                return true;
            }
        }
    }
    return false;
}

/**
 * 条目类型
 */
interface Entry {
    name: string;
    type: 'file' | 'directory';
}

/**
 * 单个目录的列出结果
 */
interface ListResult {
    path: string;
    workspace?: string;
    entries: Entry[];
    fileCount: number;
    dirCount: number;
    success: boolean;
    error?: string;
}

/**
 * 递归列出目录内容
 */
async function listDirectoryRecursive(
    dirUri: vscode.Uri,
    basePath: string,
    entries: Entry[],
    ignorePatterns: string[]
): Promise<void> {
    const items = await vscode.workspace.fs.readDirectory(dirUri);
    
    for (const [name, type] of items) {
        // 跳过忽略的目录和文件
        if (shouldIgnore(name, ignorePatterns)) {
            continue;
        }
        
        const relativePath = basePath ? path.join(basePath, name) : name;
        
        if (type === vscode.FileType.Directory) {
            entries.push({ name: relativePath + '/', type: 'directory' });
            // 递归进入子目录
            const subDirUri = vscode.Uri.joinPath(dirUri, name);
            await listDirectoryRecursive(subDirUri, relativePath, entries, ignorePatterns);
        } else if (type === vscode.FileType.File) {
            entries.push({ name: relativePath, type: 'file' });
        }
    }
}

/**
 * 创建列出文件工具
 */
export function createListFilesTool(): Tool {
    const workspaces = getAllWorkspaces();
    const isMultiRoot = workspaces.length > 1;
    
    // 数组格式强调说明
    const arrayFormatNote = ' MUST be an array even for single directory, e.g., ["src"]';
    
    let pathsDescription = 'Array of directory paths to list (relative to workspace root).' + arrayFormatNote;
    if (isMultiRoot) {
        pathsDescription = `Array of directory paths to list, must use "workspace_name/path" format.${arrayFormatNote} Available workspaces: ${workspaces.map(w => w.name).join(', ')}`;
    }
    
    return {
        declaration: {
            name: 'list_files',
            description: isMultiRoot
                ? `List files and subdirectories in one or more directories. Multi-root workspace: Must use "workspace_name/path" format. Available workspaces: ${workspaces.map(w => w.name).join(', ')}`
                : 'List files and subdirectories in one or more directories. Supports batch listing.',
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
                    },
                    recursive: {
                        type: 'boolean',
                        description: 'Whether to list subdirectories recursively',
                        default: false
                    }
                },
                required: ['paths']
            }
        },
        handler: async (args): Promise<ToolResult> => {
            // 支持 paths 数组或单个 path（向后兼容）
            let pathList: string[] = [];
            
            if (args.paths && Array.isArray(args.paths)) {
                pathList = args.paths as string[];
            } else if (args.path && typeof args.path === 'string') {
                // 向后兼容单个 path 参数
                pathList = [args.path];
            }
            
            if (pathList.length === 0) {
                pathList = ['.']; // 默认为根目录
            }
            
            const recursive = (args.recursive as boolean) || false;

            const workspaces = getAllWorkspaces();
            if (workspaces.length === 0) {
                return { success: false, error: 'No workspace folder open' };
            }
            
            const isMultiRoot = workspaces.length > 1;

            // 获取忽略列表配置
            const ignorePatterns = getIgnorePatterns();

            const results: ListResult[] = [];
            let totalFiles = 0;
            let totalDirs = 0;

            for (const dirPath of pathList) {
                try {
                    const { uri: dirUri, workspace, relativePath, isExplicit } = resolveUriWithInfo(dirPath);
                    if (!dirUri) {
                        results.push({
                            path: dirPath,
                            entries: [],
                            fileCount: 0,
                            dirCount: 0,
                            success: false,
                            error: 'No workspace folder open'
                        });
                        continue;
                    }
                    
                    const entries: Entry[] = [];
                    
                    if (recursive) {
                        // 递归列出
                        await listDirectoryRecursive(dirUri, '', entries, ignorePatterns);
                    } else {
                        // 只列出顶层
                        const items = await vscode.workspace.fs.readDirectory(dirUri);
                        
                        for (const [name, type] of items) {
                            // 跳过忽略的目录和文件
                            if (shouldIgnore(name, ignorePatterns)) {
                                continue;
                            }
                            
                            if (type === vscode.FileType.Directory) {
                                entries.push({ name: name + '/', type: 'directory' });
                            } else if (type === vscode.FileType.File) {
                                entries.push({ name, type: 'file' });
                            }
                        }
                    }
                    
                    // 排序：目录在前，文件在后，各自按名称排序
                    entries.sort((a, b) => {
                        if (a.type !== b.type) {
                            return a.type === 'directory' ? -1 : 1;
                        }
                        return a.name.localeCompare(b.name);
                    });
                    
                    const fileCount = entries.filter(e => e.type === 'file').length;
                    const dirCount = entries.filter(e => e.type === 'directory').length;

                    results.push({
                        path: dirPath,
                        workspace: isMultiRoot ? workspace?.name : undefined,
                        entries,
                        fileCount,
                        dirCount,
                        success: true
                    });
                    totalFiles += fileCount;
                    totalDirs += dirCount;
                } catch (error) {
                    results.push({
                        path: dirPath,
                        entries: [],
                        fileCount: 0,
                        dirCount: 0,
                        success: false,
                        error: error instanceof Error ? error.message : String(error)
                    });
                }
            }

            const allSuccess = results.every(r => r.success);
            return {
                success: allSuccess,
                data: {
                    results,
                    totalFiles,
                    totalDirs,
                    totalPaths: pathList.length
                },
                error: allSuccess ? undefined : 'Some directories failed to list'
            };
        }
    };
}

/**
 * 注册列出文件工具
 */
export function registerListFiles(): Tool {
    return createListFilesTool();
}