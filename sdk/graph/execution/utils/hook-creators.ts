/**
 * Hook Creator Tool (Thread-specific section)
 * Provides functions for creating Thread-related Hook configurations
 *
 * Note: The SDK fully trusts user configurations and does not implement any default validation logic.
 * The application layer should implement custom validation logic based on actual requirements.
 */

import type { NodeHook, ScriptNodeConfig } from "@wf-agent/types";

import type { HookExecutionContext } from "../handlers/hook-handlers/hook-handler.js";

import { ExecutionError } from "@wf-agent/types";

// Reexport the generic Hook creator
export { createCustomValidationHook } from "../../../core/utils/hook/creators.js";

/**
 * Create a thread status check Hook
 * @param allowedStates List of allowed thread statuses
 * @returns NodeHook configuration
 */
export function createThreadStateCheckHook(allowedStates: string[] = ["RUNNING"]): NodeHook {
  return {
    hookType: "BEFORE_EXECUTE",
    eventName: "validation.thread_status_check",
    weight: 200,
    eventPayload: {
      allowedStates,
      handler: async (context: HookExecutionContext) => {
        const status = context.threadEntity.getStatus();
        if (!allowedStates.includes(status)) {
          throw new ExecutionError(
            `Thread is in ${status} state, expected: ${allowedStates.join(", ")}`,
            context.node.id,
          );
        }
      },
    },
  };
}

/**
 * Create a permission check Hook
 * @param requiredPermissions List of required permissions
 * @returns NodeHook configuration
 */
export function createPermissionCheckHook(requiredPermissions: string[]): NodeHook {
  return {
    hookType: "BEFORE_EXECUTE",
    eventName: "business.permission_check",
    weight: 100,
    eventPayload: {
      requiredPermissions,
      handler: async (context: HookExecutionContext) => {
        const thread = context.threadEntity.getThread();
        const userPermissions = (thread.variableScopes.thread?.["permissions"] || []) as string[];
        const missing = requiredPermissions.filter(p => !userPermissions.includes(p));

        if (missing.length > 0) {
          throw new ExecutionError(`Missing permissions: ${missing.join(", ")}`, context.node.id);
        }
      },
    },
  };
}

/**
 * Create an audit log Hook
 * @param auditService: Audit service instance
 * @returns: NodeHook configuration
 */
export function createAuditLoggingHook(auditService: {
  log: (event: Record<string, unknown>) => Promise<void>;
}): NodeHook {
  return {
    hookType: "BEFORE_EXECUTE",
    eventName: "monitoring.execution_audit",
    weight: 50,
    eventPayload: {
      handler: async (context: HookExecutionContext) => {
        const config = context.node.config as ScriptNodeConfig;
        const thread = context.threadEntity.getThread();

        await auditService.log({
          eventType: "NODE_EXECUTION_ATTEMPT",
          timestamp: new Date(),
          threadId: context.threadEntity.id,
          nodeId: context.node.id,
          nodeName: context.node.name,
          nodeType: context.node.type,
          userId: thread.variableScopes.thread?.["userId"],
          scriptName: config.scriptName,
          riskLevel: config.risk,
        });
      },
    },
  };
}
