/**
 * Hook Creator Tool (General Part)
 * Provides a convenient function for creating Hook configurations
 *
 * Note: The SDK fully trusts the user's configurations and does not include any default validation logic.
 * The application layer should implement custom validation logic based on actual requirements.
 */

import type { NodeHook } from "@wf-agent/types";

/**
 * 创建自定义验证Hook
 *
 * 应用层可以传入自定义的验证函数来实现任何验证逻辑。
 * SDK不预设任何验证规则，完全信任应用层的实现。
 *
 * @param validator 自定义验证函数
 * @param eventName 事件名称（默认为 'validation.custom_check'）
 * @param weight Hook权重（默认为150）
 * @returns NodeHook配置
 *
 * @example
 * // 应用层自定义验证逻辑
 * const customHook = createCustomValidationHook(
 *   async (context) => {
 *     const config = context.node.config as ScriptNodeConfig;
 *     // 实现自定义验证逻辑
 *     if (config.scriptName.includes('..')) {
 *       throw new ExecutionError('Invalid script path', context.node.id);
 *     }
 *   },
 *   'security.path_check',
 *   150
 * );
 */
export function createCustomValidationHook(
  validator: (context: unknown) => Promise<void> | void,
  eventName: string = "validation.custom_check",
  weight: number = 150,
): NodeHook {
  return {
    hookType: "BEFORE_EXECUTE",
    eventName,
    weight,
    eventPayload: {
      handler: validator,
    },
  };
}
