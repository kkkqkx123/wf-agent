/**
 * Timeout Configuration Zod Schemas
 * Provides runtime validation schemas for Timeout configuration
 */

import { z } from "zod";
import type { TimeoutConfig } from "./timeout.js";

/**
 * Timeout Configuration Schema
 */
export const TimeoutConfigSchema: z.ZodType<TimeoutConfig> = z.object({
  workflowExecutionCompletion: z.number().int("Must be an integer").optional(),
  workflowExecutionPause: z.number().int("Must be an integer").optional(),
  workflowExecutionCancel: z.number().int("Must be an integer").optional(),
  workflowExecutionResume: z.number().int("Must be an integer").optional(),
  childExecutionWait: z.number().int("Must be an integer").optional(),
  cascadeCancel: z.number().int("Must be an integer").optional(),
  nodeCompletion: z.number().int("Must be an integer").optional(),
  nodeFailed: z.number().int("Must be an integer").optional(),
  syncBranchWait: z.number().int("Must be an integer").optional(),
  joinCompletion: z.number().int("Must be an integer").optional(),
  lifecycleEvent: z.number().int("Must be an integer").optional(),
  pollingWait: z.number().int("Must be an integer").optional(),
  pollingInterval: z.number().int("Must be an integer").optional(),
  default: z.number().int("Must be an integer").optional(),
  maxAllowed: z.number().int("Must be an integer").optional(),
});

/**
 * Type guard for TimeoutConfig
 */
export function isTimeoutConfig(value: unknown): value is TimeoutConfig {
  return TimeoutConfigSchema.safeParse(value).success;
}
