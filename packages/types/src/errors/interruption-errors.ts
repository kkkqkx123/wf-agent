/**
 * Defining Interrupt-Related Error Types
 * Defines the types of errors associated with execution interrupts and operation aborts
 */

import { SDKError, ErrorSeverity } from "./base.js";

/**
 * Interrupt Type
 */
export type InterruptionType = "PAUSE" | "STOP" | null;

/**
 * Interrupt Exception Base Class
 *
 * Description:
 * 1. generic execution interrupt exception base class
 * 2. This is a control flow exception, not a real error.
 * 3. interrupt type: PAUSE (pause, resume) or STOP (stop, not resume)
 * 4. subclasses can add module-specific context information
 *
 * Usage Scenario:
 * - WorkflowExecutionInterruptedException: Workflow execution interruption in Graph module.
 * - AgentInterruptedException: Session interruption in the Agent module.
 */
export class InterruptedException extends SDKError {
  constructor(
    message: string,
    public readonly interruptionType: InterruptionType,
    context?: Record<string, unknown>,
  ) {
    super(message, "info", { ...context, interruptionType });
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "info";
  }
}

/**
 * Workflow Execution Interrupt Exception Type (Graph Module)
 *
 * Description:
 * 1. Used to indicate that workflow execution in the Graph module is interrupted by user request (pause or stop)
 * 2. Inherits from InterruptedException, adding Graph module specific context
 * 3. After the executor catches this exception, it will handle it according to the interruption type
 *
 * Usage Scenario:
 * - When user calls pauseWorkflowExecution(), the executor throws this exception at a safe point
 * - When user calls stopWorkflowExecution(), the executor throws this exception at a safe point
 * - WorkflowExecutionCoordinator throws when detecting interruption flag
 * - ToolCallExecutor converts AbortError to WorkflowExecutionInterruptedException
 */
export class WorkflowExecutionInterruptedException extends InterruptedException {
  constructor(
    message: string,
    interruptionType: InterruptionType,
    public readonly executionId?: string,
    public readonly nodeId?: string,
    context?: Record<string, unknown>,
  ) {
    super(message, interruptionType, { ...context, executionId, nodeId });
  }
}

/**
 * Agent 中断异常类型（Agent 模块）
 *
 * 说明：
 * 1. 用于表示 Agent 模块中执行被用户请求中断（暂停或停止）
 * 2. 继承自 InterruptedException，添加 Agent 模块特有的上下文
 * 3. AgentLoopExecutor 检测到中断标志时抛出
 *
 * 使用场景：
 * - 用户调用 pauseConversation() 时，执行器在安全点抛出此异常
 * - 用户调用 stopConversation() 时，执行器在安全点抛出此异常
 * - AgentLoopExecutor 检测到中断标志时抛出
 */
export class AgentInterruptedException extends InterruptedException {
  constructor(
    message: string,
    interruptionType: InterruptionType,
    public readonly conversationId?: string,
    public readonly sessionId?: string,
    context?: Record<string, unknown>,
  ) {
    super(message, interruptionType, { ...context, conversationId, sessionId });
  }
}

/**
 * AbortError - Operation Abort Error
 *
 * Description:
 * 1. thrown when AbortSignal is triggered
 * 2. this is a control flow exception, not a real error
 * 3. contains the original cause of the interruption (InterruptedException or its subclasses)
 *
 * Usage Scenario:
 * - Thrown when TimeoutController detects AbortSignal.
 * - Thrown when an HTTP request is aborted
 * - Thrown when LLM call is aborted
 * - Thrown when tool execution is aborted
 */
export class AbortError extends Error {
  public override readonly name = "AbortError";

  constructor(
    message: string,
    public override readonly cause?: InterruptedException,
  ) {
    super(message);
    // Maintaining the right prototype chain
    Object.setPrototypeOf(this, AbortError.prototype);
  }
}
