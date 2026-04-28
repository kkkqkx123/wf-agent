/**
 * Workflow Variable Type Definitions
 */

import type { VariableScope } from "../thread/scopes.js";
import type { VariableValueType } from "../thread/variables.js";

/**
 * 工作流变量定义类型
 * 用于在工作流定义阶段声明变量，提供类型安全和初始值
 *
 * 说明：
 * - 在工作流定义时声明变量，提供类型信息和默认值
 * - 执行时转换为 ThreadVariable，存储在 Thread.variableScopes.thread 中
 * - 通过 VARIABLE 节点修改，通过表达式访问（{{variableName}}）
 *
 * 示例：
 * ```typescript
 * workflow.variables = [
 *   { name: 'userName', type: 'string', defaultValue: 'Alice' },
 *   { name: 'userAge', type: 'number', defaultValue: 25 }
 * ]
 *
 * // 执行时
 * thread.variableScopes.thread = {
 *   userName: 'Alice',
 *   userAge: 25
 * }
 *
 * // 在表达式中访问
 * {{userName}}  // 'Alice'
 * {{userAge}}  // 25
 * ```
 */
export interface WorkflowVariable {
  /** variable name */
  name: string;
  /** Variable type */
  type: VariableValueType;
  /** variable initial value */
  defaultValue?: unknown;
  /** Variable Description */
  description?: string;
  /** Required or not */
  required?: boolean;
  /** Read-only or not */
  readonly?: boolean;
  /** variable scope */
  scope?: VariableScope;
}
