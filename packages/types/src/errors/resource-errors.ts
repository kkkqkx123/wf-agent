/**
 * 资源相关错误类型定义
 * 定义资源未找到相关的错误类型
 *
 * 注意：所有资源未找到错误默认为严重错误（error级别）
 * 如果需要记录警告但不中断执行，请使用 ContextualLogger.resourceNotFoundWarning()
 */

import { NotFoundError, ErrorSeverity } from "./base.js";

/**
 * Workflow not found error type
 *
 * Specialized for workflow not found scenarios
 * Inherited from NotFoundError to maintain backward compatibility
 */
export class WorkflowNotFoundError extends NotFoundError {
  constructor(
    message: string,
    workflowId: string,
    context?: Record<string, unknown>,
    severity?: ErrorSeverity,
  ) {
    super(message, "Workflow", workflowId, context, severity);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * Node not found error type
 *
 * Specialized for node not found scenarios
 * Inherited from NotFoundError to maintain backward compatibility
 */
export class NodeNotFoundError extends NotFoundError {
  constructor(
    message: string,
    nodeId: string,
    context?: Record<string, unknown>,
    severity?: ErrorSeverity,
  ) {
    super(message, "Node", nodeId, context, severity);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * Tool not found error type
 *
 * Specialized for tool not found scenarios
 * Inherited from NotFoundError to maintain backward compatibility
 */
export class ToolNotFoundError extends NotFoundError {
  constructor(
    message: string,
    toolId: string,
    context?: Record<string, unknown>,
    severity?: ErrorSeverity,
  ) {
    super(message, "Tool", toolId, context, severity);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * Script not found error type
 *
 * Specialized for script not found scenarios
 * Inherited from NotFoundError to maintain backward compatibility
 */
export class ScriptNotFoundError extends NotFoundError {
  constructor(
    message: string,
    scriptName: string,
    context?: Record<string, unknown>,
    severity?: ErrorSeverity,
  ) {
    super(message, "Script", scriptName, context, severity);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * Thread context not found error type
 *
 * Specialized for thread context not found scenarios
 * Inherited from NotFoundError for backward compatibility.
 */
export class ThreadContextNotFoundError extends NotFoundError {
  constructor(
    message: string,
    threadId: string,
    context?: Record<string, unknown>,
    severity?: ErrorSeverity,
  ) {
    super(message, "ThreadContext", threadId, context, severity);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * Checkpoint not found error type
 *
 * Specialized for checkpoint not found scenarios
 * Inherited from NotFoundError to maintain backward compatibility
 */
export class CheckpointNotFoundError extends NotFoundError {
  constructor(
    message: string,
    checkpointId: string,
    context?: Record<string, unknown>,
    severity?: ErrorSeverity,
  ) {
    super(message, "Checkpoint", checkpointId, context, severity);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * Trigger template error type not found
 *
 * Specialized for trigger template not found scenarios
 * Inherited from NotFoundError to maintain backward compatibility
 */
export class TriggerTemplateNotFoundError extends NotFoundError {
  constructor(
    message: string,
    templateName: string,
    context?: Record<string, unknown>,
    severity?: ErrorSeverity,
  ) {
    super(message, "TriggerTemplate", templateName, context, severity);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}

/**
 * Node template not found error type
 *
 * Specialized for node template not found scenarios
 * Inherited from NotFoundError to maintain backward compatibility
 */
export class NodeTemplateNotFoundError extends NotFoundError {
  constructor(
    message: string,
    templateName: string,
    context?: Record<string, unknown>,
    severity?: ErrorSeverity,
  ) {
    super(message, "NodeTemplate", templateName, context, severity);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }
}
