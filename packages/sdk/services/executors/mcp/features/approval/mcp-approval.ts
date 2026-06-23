/**
 * Enhanced MCP Approval System
 *
 * Provides fine-grained approval rules for MCP tool calls and resource access.
 * Extends the basic approval mechanism with parameter validation, rate limiting,
 * and user/role-based access control.
 */

import { createContextualLogger } from "../../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "McpApprovalEnhanced" });

/**
 * Parameter approval rule
 *
 * Allows fine-grained control over which parameters are allowed.
 * Example: Only allow reading from specific directories.
 */
export interface ParameterApprovalRule {
  /** Parameter name */
  paramName: string;
  /** Allowed values (regex pattern or literal) */
  allowedValues?: string[];
  /** Denied values (regex pattern or literal) */
  deniedValues?: string[];
  /** Custom validation function */
  validate?: (value: unknown) => boolean;
}

/**
 * Rate limiting rule
 */
export interface RateLimitingRule {
  /** Maximum calls per time window */
  maxCalls: number;
  /** Time window in milliseconds */
  windowMs: number;
  /** User/role identifier (if empty, applies globally) */
  userId?: string;
}

/**
 * User/role-based access rule
 */
export interface AccessControlRule {
  /** User or role identifier */
  userId: string;
  /** Allowed servers (if empty, allows all) */
  allowedServers?: string[];
  /** Denied servers */
  deniedServers?: string[];
  /** Allowed tools (if empty, allows all) */
  allowedTools?: string[];
  /** Denied tools */
  deniedTools?: string[];
  /** Allowed operations */
  allowedOperations?: ("tool_call" | "resource_read")[];
}

/**
 * Tool call approval context
 */
export interface ToolCallApprovalContext {
  serverName: string;
  toolName: string;
  arguments?: Record<string, unknown>;
  userId?: string;
  timestamp?: number;
}

/**
 * Resource access approval context
 */
export interface ResourceAccessApprovalContext {
  serverName: string;
  resourceUri: string;
  userId?: string;
  timestamp?: number;
}

/**
 * Approval result
 */
export interface ApprovalResult {
  /** Whether approval was granted */
  approved: boolean;
  /** Reason for approval/denial */
  reason: string;
  /** Risk level of the operation */
  riskLevel: "low" | "medium" | "high";
  /** Metadata about the decision */
  metadata?: Record<string, unknown>;
}

/**
 * Enhanced MCP Approval System
 *
 * Provides fine-grained approval control for MCP operations.
 */
export class EnhancedMcpApprovalSystem {
  private parameterRules = new Map<string, ParameterApprovalRule[]>();
  private rateLimitingRules: RateLimitingRule[] = [];
  private accessControlRules: AccessControlRule[] = [];
  private callHistory = new Map<string, Array<{ timestamp: number; count: number }>>();

  /**
   * Add parameter approval rule
   *
   * @param toolId - Tool identifier (serverName/toolName)
   * @param rule - Parameter approval rule
   */
  addParameterRule(toolId: string, rule: ParameterApprovalRule): void {
    if (!this.parameterRules.has(toolId)) {
      this.parameterRules.set(toolId, []);
    }
    this.parameterRules.get(toolId)!.push(rule);
    logger.debug("Parameter rule added", { toolId, paramName: rule.paramName });
  }

  /**
   * Add rate limiting rule
   *
   * @param rule - Rate limiting rule
   */
  addRateLimitingRule(rule: RateLimitingRule): void {
    this.rateLimitingRules.push(rule);
    logger.debug("Rate limiting rule added", { maxCalls: rule.maxCalls, windowMs: rule.windowMs });
  }

  /**
   * Add access control rule
   *
   * @param rule - Access control rule
   */
  addAccessControlRule(rule: AccessControlRule): void {
    this.accessControlRules.push(rule);
    logger.debug("Access control rule added", { userId: rule.userId });
  }

  /**
   * Validate parameters against rules
   *
   * @param toolId - Tool identifier
   * @param parameters - Parameters to validate
   * @returns Validation result
   */
  private validateParameters(toolId: string, parameters: Record<string, unknown>): ApprovalResult {
    const rules = this.parameterRules.get(toolId);
    if (!rules || rules.length === 0) {
      return {
        approved: true,
        reason: "No parameter restrictions",
        riskLevel: "low",
      };
    }

    for (const rule of rules) {
      const value = parameters[rule.paramName];

      // Check allowed values
      if (rule.allowedValues && rule.allowedValues.length > 0) {
        const valueStr = String(value);
        const isAllowed = rule.allowedValues.some(allowed => {
          // Try as regex first, then literal match
          try {
            return new RegExp(allowed).test(valueStr);
          } catch {
            return allowed === valueStr;
          }
        });

        if (!isAllowed) {
          return {
            approved: false,
            reason: `Parameter '${rule.paramName}' value not allowed: ${value}`,
            riskLevel: "high",
          };
        }
      }

      // Check denied values
      if (rule.deniedValues && rule.deniedValues.length > 0) {
        const valueStr = String(value);
        const isDenied = rule.deniedValues.some(denied => {
          try {
            return new RegExp(denied).test(valueStr);
          } catch {
            return denied === valueStr;
          }
        });

        if (isDenied) {
          return {
            approved: false,
            reason: `Parameter '${rule.paramName}' value denied: ${value}`,
            riskLevel: "high",
          };
        }
      }

      // Custom validation
      if (rule.validate) {
        if (!rule.validate(value)) {
          return {
            approved: false,
            reason: `Parameter '${rule.paramName}' failed custom validation`,
            riskLevel: "high",
          };
        }
      }
    }

    return {
      approved: true,
      reason: "All parameters passed validation",
      riskLevel: "low",
    };
  }

  /**
   * Check rate limiting
   *
   * @param toolId - Tool identifier
   * @param userId - User identifier
   * @returns Rate limiting result
   */
  private checkRateLimit(toolId: string, userId?: string): ApprovalResult {
    const now = Date.now();
    const key = userId ? `${userId}:${toolId}` : `global:${toolId}`;

    for (const rule of this.rateLimitingRules) {
      // Check if rule applies
      if (rule.userId && rule.userId !== userId) {
        continue;
      }

      // Get call history
      if (!this.callHistory.has(key)) {
        this.callHistory.set(key, []);
      }

      const history = this.callHistory.get(key)!;

      // Clean up old entries
      const cutoffTime = now - rule.windowMs;
      const recentCalls = history.filter(h => h.timestamp > cutoffTime);

      // Count recent calls
      const callCount = recentCalls.reduce((sum, h) => sum + h.count, 0);

      if (callCount >= rule.maxCalls) {
        return {
          approved: false,
          reason: `Rate limit exceeded: ${callCount}/${rule.maxCalls} calls in ${rule.windowMs}ms`,
          riskLevel: "medium",
          metadata: { callCount, limit: rule.maxCalls },
        };
      }

      // Record this call
      const lastEntry = recentCalls[recentCalls.length - 1];
      if (lastEntry && lastEntry.timestamp === now) {
        lastEntry.count++;
      } else {
        recentCalls.push({ timestamp: now, count: 1 });
      }

      this.callHistory.set(key, recentCalls);
    }

    return {
      approved: true,
      reason: "Rate limit check passed",
      riskLevel: "low",
    };
  }

  /**
   * Check access control
   *
   * @param context - Approval context
   * @returns Access control result
   */
  private checkAccessControl(
    serverName: string,
    toolName: string,
    userId?: string,
    operation: "tool_call" | "resource_read" = "tool_call",
  ): ApprovalResult {
    // Find applicable rules for user
    const userRules = this.accessControlRules.filter(r => !userId || r.userId === userId || r.userId === "*");

    for (const rule of userRules) {
      // Check operation type
      if (rule.allowedOperations && !rule.allowedOperations.includes(operation)) {
        return {
          approved: false,
          reason: `Operation '${operation}' not allowed for user '${userId}'`,
          riskLevel: "medium",
        };
      }

      // Check server access
      if (rule.allowedServers && rule.allowedServers.length > 0) {
        if (!rule.allowedServers.includes(serverName)) {
          return {
            approved: false,
            reason: `Server '${serverName}' not in allowed list for user '${userId}'`,
            riskLevel: "medium",
          };
        }
      }

      if (rule.deniedServers && rule.deniedServers.includes(serverName)) {
        return {
          approved: false,
          reason: `Server '${serverName}' is denied for user '${userId}'`,
          riskLevel: "medium",
        };
      }

      // Check tool access
      if (rule.allowedTools && rule.allowedTools.length > 0) {
        if (!rule.allowedTools.includes(toolName)) {
          return {
            approved: false,
            reason: `Tool '${toolName}' not in allowed list for user '${userId}'`,
            riskLevel: "medium",
          };
        }
      }

      if (rule.deniedTools && rule.deniedTools.includes(toolName)) {
        return {
          approved: false,
          reason: `Tool '${toolName}' is denied for user '${userId}'`,
          riskLevel: "medium",
        };
      }
    }

    return {
      approved: true,
      reason: "Access control check passed",
      riskLevel: "low",
    };
  }

  /**
   * Check tool call approval
   *
   * @param context - Tool call approval context
   * @returns Approval result
   */
  checkToolCallApproval(context: ToolCallApprovalContext): ApprovalResult {
    const toolId = `${context.serverName}/${context.toolName}`;

    // Check access control first
    const accessResult = this.checkAccessControl(
      context.serverName,
      context.toolName,
      context.userId,
      "tool_call",
    );
    if (!accessResult.approved) {
      return accessResult;
    }

    // Check rate limiting
    const rateLimitResult = this.checkRateLimit(toolId, context.userId);
    if (!rateLimitResult.approved) {
      return rateLimitResult;
    }

    // Check parameters
    if (context.arguments) {
      const paramResult = this.validateParameters(toolId, context.arguments);
      if (!paramResult.approved) {
        return paramResult;
      }
    }

    logger.debug("Tool call approved", {
      serverName: context.serverName,
      toolName: context.toolName,
      userId: context.userId,
    });

    return {
      approved: true,
      reason: "All approval checks passed",
      riskLevel: "low",
    };
  }

  /**
   * Check resource access approval
   *
   * @param context - Resource access approval context
   * @returns Approval result
   */
  checkResourceAccessApproval(context: ResourceAccessApprovalContext): ApprovalResult {
    // Check access control
    const accessResult = this.checkAccessControl(
      context.serverName,
      "resource_read",
      context.userId,
      "resource_read",
    );
    if (!accessResult.approved) {
      return accessResult;
    }

    // Check rate limiting for the server
    const rateLimitResult = this.checkRateLimit(`${context.serverName}/resource`, context.userId);
    if (!rateLimitResult.approved) {
      return rateLimitResult;
    }

    logger.debug("Resource access approved", {
      serverName: context.serverName,
      uri: context.resourceUri,
      userId: context.userId,
    });

    return {
      approved: true,
      reason: "All approval checks passed",
      riskLevel: "low",
    };
  }

  /**
   * Get approval statistics
   *
   * @returns Statistics about approval decisions
   */
  getStatistics(): {
    totalRules: number;
    parameterRules: number;
    rateLimitingRules: number;
    accessControlRules: number;
    callHistorySize: number;
  } {
    return {
      totalRules:
        this.parameterRules.size + this.rateLimitingRules.length + this.accessControlRules.length,
      parameterRules: this.parameterRules.size,
      rateLimitingRules: this.rateLimitingRules.length,
      accessControlRules: this.accessControlRules.length,
      callHistorySize: this.callHistory.size,
    };
  }

  /**
   * Clear call history
   *
   * Useful for testing or reset scenarios.
   */
  clearCallHistory(): void {
    this.callHistory.clear();
    logger.debug("Call history cleared");
  }
}
