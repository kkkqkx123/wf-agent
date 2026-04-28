/**
 * VariableAccessor - 统一的变量访问器
 * 提供统一的变量访问接口，支持嵌套路径解析
 *
 * 支持的路径格式：
 * - 简单变量名：userName（按作用域优先级查找）
 * - 嵌套对象：user.profile.name
 * - 数组索引：items[0].name
 * - 特殊命名空间：
 *   - input.userName：输入数据
 *   - output.result：输出数据
 *   - global.config：全局作用域变量
 *   - thread.state：线程作用域变量
 *   - subgraph.temp：子图作用域变量
 *   - loop.item：循环作用域变量
 *
 * 使用示例：
 * - accessor.get('userName') - 按作用域优先级获取变量值
 * - accessor.get('user.profile.name') - 获取嵌套属性
 * - accessor.get('items[0].name') - 获取数组元素
 * - accessor.get('input.userName') - 获取输入数据
 * - accessor.get('output.result') - 获取输出数据
 * - accessor.get('global.config') - 获取全局变量
 * - accessor.get('thread.state') - 获取线程变量
 * - accessor.get('subgraph.temp') - 获取子图变量
 * - accessor.get('loop.item') - 获取循环变量
 */

import type { ThreadEntity } from "../../entities/workflow-execution-entity.js";
import type { VariableScope } from "@wf-agent/types";
import { resolvePath } from "@wf-agent/common-utils";

/**
 * Variable Naming Space
 */
export type VariableNamespace =
  | "input" /** Input data */
  | "output" /** Output data */
  | "global" /** Global scope */
  | "thread" /** Thread scope */
  | "local" /** Local scope */
  | "loop"; /** Loop Scope */

/**
 * VariableAccessor - A unified variable accessor
 */
export class VariableAccessor {
  /**
   * Constructor
   * @param threadEntity Thread entity
   */
  constructor(private readonly threadEntity: ThreadEntity) {}

  /**
   * 获取变量值
   * @param path 变量路径，支持嵌套和命名空间
   * @returns 变量值，如果不存在则返回 undefined
   *
   * @example
   * // 简单变量（按作用域优先级查找）
   * accessor.get('userName')
   *
   * // 嵌套路径
   * accessor.get('user.profile.name')
   *
   * // 数组索引
   * accessor.get('items[0].name')
   *
   * // 命名空间
   * accessor.get('input.userName')
   * accessor.get('output.result')
   * accessor.get('global.config')
   * accessor.get('thread.state')
   * accessor.get('subgraph.temp')
   * accessor.get('loop.item')
   */
  get(path: string): unknown {
    if (!path) {
      return undefined;
    }

    // Parse namespaces
    const parts = path.split(".");
    const namespace = parts[0];
    const remainingPath = parts.slice(1).join(".");

    // Handling namespaces
    switch (namespace) {
      case "input":
        return this.getFromInput(remainingPath);

      case "output":
        return this.getFromOutput(remainingPath);

      case "global":
        return this.getFromScope(remainingPath || path, "global");

      case "thread":
        return this.getFromScope(remainingPath || path, "thread");

      case "local":
        return this.getFromScope(remainingPath || path, "local");

      case "loop":
        return this.getFromScope(remainingPath || path, "loop");

      default:
        // Without a namespace prefix, the search is done based on the scope priority.
        return this.getFromScopedVariables(path);
    }
  }

  /**
   * Check if the variable exists
   * @param path Path to the variable
   * @returns Whether the variable exists
   */
  has(path: string): boolean {
    return this.get(path) !== undefined;
  }

  /**
   * Get a value from the input data
   * @param path The path (relative to input)
   * @returns The value
   */
  private getFromInput(path: string): unknown {
    const input = this.threadEntity.getInput();
    if (!path) {
      return input;
    }
    return resolvePath(path, input);
  }

  /**
   * Retrieve a value from the output data
   * @param path The path (relative to the output)
   * @returns The value
   */
  private getFromOutput(path: string): unknown {
    const output = this.threadEntity.getOutput();
    if (!path) {
      return output;
    }
    return resolvePath(path, output);
  }

  /**
   * Get a value from the specified scope
   * @param path Path
   * @param scope Scope
   * @returns Value
   */
  private getFromScope(path: string, scope: VariableScope): unknown {
    const thread = this.workflowExecutionEntity.getThread();
    const scopes = thread.variableScopes;

    let scopeData: Record<string, unknown> | undefined;

    switch (scope) {
      case "global":
        scopeData = scopes.global;
        break;
      case "thread":
        scopeData = scopes.thread;
        break;
      case "local":
        if (scopes.local.length > 0) {
          scopeData = scopes.local[scopes.local.length - 1];
        }
        break;
      case "loop":
        if (scopes.loop.length > 0) {
          scopeData = scopes.loop[scopes.loop.length - 1];
        }
        break;
    }

    if (!scopeData) {
      return undefined;
    }

    // Extract the root variable name
    const pathParts = path.split(".");
    const rootVarName = pathParts[0];

    if (!rootVarName) {
      return undefined;
    }

    const rootValue = scopeData[rootVarName];

    if (rootValue === undefined) {
      return undefined;
    }

    // If the path contains nesting, use resolvePath to parse the remaining path.
    if (pathParts.length > 1) {
      const remainingPath = pathParts.slice(1).join(".");
      return resolvePath(remainingPath, rootValue);
    }

    return rootValue;
  }

  /**
   * Retrieve a value from a scope variable (searched in order of priority)
   * Priority: loop > subgraph > thread > global
   * @param path The path to the variable
   * @returns The value of the variable
   */
  private getFromScopedVariables(path: string): unknown {
    // Extract the root variable name
    const pathParts = path.split(".");
    const rootVarName = pathParts[0];

    if (!rootVarName) {
      return undefined;
    }

    const rootValue = this.threadEntity.getVariable(rootVarName);

    if (rootValue === undefined) {
      return undefined;
    }

    // If the path contains nesting, use resolvePath to parse the remaining path.
    if (pathParts.length > 1) {
      const remainingPath = pathParts.slice(1).join(".");
      return resolvePath(remainingPath, rootValue);
    }

    return rootValue;
  }
}
