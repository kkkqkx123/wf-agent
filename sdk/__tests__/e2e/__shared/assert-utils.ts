/**
 * Common assertion utilities for SDK E2E tests.
 */

import { expect } from "vitest";
import type { ExecutionResult } from "@/api/index.js";
import type { WorkflowExecutionResult, AgentLoopResult } from "@wf-agent/types";

/**
 * Assert that a workflow execution completed successfully.
 */
export function assertWorkflowCompleted(result: ExecutionResult<WorkflowExecutionResult>): void {
  expect(result.result.isOk()).toBe(true);
  if (result.result.isOk()) {
    const data = result.result.value;
    expect(data.metadata?.status).toBe("COMPLETED");
    expect(data.output).toBeDefined();
    expect(data.executionId).toBeDefined();
  }
}

/**
 * Assert that the workflow execution has a specific status.
 */
export function assertWorkflowStatus(
  result: ExecutionResult<WorkflowExecutionResult>,
  status: string,
): void {
  expect(result.result.isOk()).toBe(true);
  if (result.result.isOk()) {
    expect(result.result.value.metadata?.status).toBe(status);
  }
}

/**
 * Assert that an agent loop execution completed successfully.
 */
export function assertAgentLoopCompleted(result: AgentLoopResult): void {
  expect(result.success).toBe(true);
  expect(result.agentLoopId).toBeDefined();
}

/**
 * Assert that an agent loop has a specific number of iterations.
 */
export function assertAgentLoopIterations(result: AgentLoopResult, expectedCount: number): void {
  expect(result.iterations).toBe(expectedCount);
}

/**
 * Assert that an ExecutionResult indicates success and returns the data.
 */
export function expectOk<T>(result: ExecutionResult<T>): T {
  expect(result.result.isOk()).toBe(true);
  return (result.result as any).value as T;
}

/**
 * Assert that an ExecutionResult indicates failure and returns the error.
 */
export function expectErr<T>(result: ExecutionResult<T>): unknown {
  expect(result.result.isErr()).toBe(true);
  return (result.result as any).error;
}

/**
 * Wait for a condition to be true, with a timeout.
 */
export async function waitForCondition(
  condition: () => boolean | Promise<boolean>,
  timeoutMs: number = 5000,
  intervalMs: number = 100,
): Promise<void> {
  const startTime = Date.now();
  while (Date.now() - startTime < timeoutMs) {
    if (await condition()) {
      return;
    }
    await new Promise(resolve => setTimeout(resolve, intervalMs));
  }
  throw new Error(`Condition not met within ${timeoutMs}ms`);
}
