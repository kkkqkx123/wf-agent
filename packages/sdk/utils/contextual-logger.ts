/**
 * Contextual Logger
 *
 * Design Purpose:
 * - To replace error throws at the warning and info levels
 * - To provide structured contextual logging
 * - To reduce unnecessary stack trace overhead
 * - To maintain compatibility with existing logging systems
 *
 * Use Cases:
 * - For recording warning messages that do not need to interrupt execution
 * - For recording debugging information with context
 * - In scenarios where structured log output is required
 *
 * Design Principles:
 * - By default, use the SDK's global logger instance to maintain consistency in log configuration
 * - Support for custom logger instances to provide flexibility
 * - Provide convenient factory functions to simplify usage
 */

import type { Logger } from "@wf-agent/common-utils";
import type { ErrorContext } from "@wf-agent/types";
import { sdkLogger } from "./logger.js";

/**
 * Log recording options
 */
export interface LogOptions {
  /** Log Level */
  level: "debug" | "info" | "warn" | "error";
  /** Log message */
  message: string;
  /** Error context */
  context?: ErrorContext;
  /** Additional contextual data */
  data?: Record<string, unknown>;
  /** Original error object (optional) */
  error?: Error;
}

/**
 * Context Logger Class
 * Provides structured logging capabilities, supporting error context.
 */
export class ContextualLogger {
  constructor(
    private readonly logger: Logger,
    private readonly baseContext: ErrorContext = {},
  ) {}

  /**
   * Record debugging information
   */
  debug(message: string, context?: ErrorContext, data?: Record<string, unknown>): void {
    this.log({
      level: "debug",
      message,
      context,
      data,
    });
  }

  /**
   * Record information
   */
  info(message: string, context?: ErrorContext, data?: Record<string, unknown>): void {
    this.log({
      level: "info",
      message,
      context,
      data,
    });
  }

  /**
   * Record the warning.
   */
  warn(
    message: string,
    context?: ErrorContext,
    data?: Record<string, unknown>,
    error?: Error,
  ): void {
    this.log({
      level: "warn",
      message,
      context,
      data,
      error,
    });
  }

  /**
   * Record Errors
   * For scenarios where errors are logged but not thrown.
   */
  error(
    message: string,
    context?: ErrorContext,
    data?: Record<string, unknown>,
    error?: Error,
  ): void {
    this.log({
      level: "error",
      message,
      context,
      data,
      error,
    });
  }

  /**
   * Record verification warnings
   * Replace ConfigurationValidationError with a severity of 'warning'
   */
  validationWarning(message: string, field: string, value: unknown, context?: ErrorContext): void {
    this.warn(message, {
      ...context,
      field,
      value,
      operation: "validation",
    });
  }

  /**
   * Resource not found warning
   * Replace NotFoundError with severity: 'warning'
   */
  resourceNotFoundWarning(resourceType: string, resourceId: string, context?: ErrorContext): void {
    this.warn(`${resourceType} not found: ${resourceId}`, {
      ...context,
      resourceType,
      resourceId,
      operation: "resource_lookup",
    });
  }

  /**
   * Log network warnings
   * Replace NetworkError/HttpError with a severity of 'warning'
   */
  networkWarning(
    message: string,
    statusCode?: number,
    context?: ErrorContext,
    error?: Error,
  ): void {
    this.warn(
      message,
      {
        ...context,
        statusCode,
        operation: "network_request",
      },
      undefined,
      error,
    );
  }

  /**
   * Record execution warnings
   * Replace ExecutionError with severity: 'warning'
   */
  executionWarning(message: string, nodeId?: string, context?: ErrorContext, error?: Error): void {
    this.warn(
      message,
      {
        ...context,
        nodeId,
        operation: "node_execution",
      },
      undefined,
      error,
    );
  }

  /**
   * Core logging methods
   */
  private log(options: LogOptions): void {
    const { level, message, context, data, error } = options;

    // Merge the base context with the incoming context.
    const mergedContext = {
      ...this.baseContext,
      ...context,
      ...data,
    };

    // Construct log data
    const logData: Record<string, unknown> = {
      ...mergedContext,
    };

    // If there is an error object, add error information.
    if (error) {
      logData["error"] = {
        name: error.name,
        message: error.message,
        // Include the stack trace only at the error level.
        stack: level === "error" ? error.stack : undefined,
      };
    }

    // Call the corresponding logging method based on the level.
    switch (level) {
      case "debug":
        this.logger.debug(message, logData);
        break;
      case "info":
        this.logger.info(message, logData);
        break;
      case "warn":
        this.logger.warn(message, logData);
        break;
      case "error":
        this.logger.error(message, logData);
        break;
    }
  }

  /**
   * Create a sublogger
   * Inheriting from the base context, additional context can be added
   */
  child(additionalContext: ErrorContext): ContextualLogger {
    return new ContextualLogger(this.logger, {
      ...this.baseContext,
      ...additionalContext,
    });
  }
}

/**
 * 创建上下文日志记录器工厂函数
 *
 * 使用 SDK 全局 logger 实例，保持日志配置一致性
 *
 * @param baseContext 基础错误上下文
 * @returns ContextualLogger 实例
 *
 * @example
 * // 创建带工作流上下文的日志器
 * const logger = createContextualLogger({ workflowId: 'wf-123' });
 *
 * // 创建带执行上下文的日志器
 * const executionLogger = createContextualLogger({
 *   workflowId: 'wf-123',
 *   executionId: 'exec-456'
 * });
 */
export function createContextualLogger(baseContext?: ErrorContext): ContextualLogger {
  return new ContextualLogger(sdkLogger, baseContext);
}
