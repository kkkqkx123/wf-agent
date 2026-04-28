/**
 * Zod Schemas for Subgraph Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";
import { TruncateMessageOperationSchema, FilterMessageOperationSchema } from "../../message/message-operations-schema.js";

/**
 * Subgraph node configuration schema
 */
export const SubgraphNodeConfigSchema = z.object({
  subgraphId: z.string().min(1, "Subgraph ID is required"),
  async: z.boolean(),
});

/**
 * Start from trigger node configuration schema
 * Empty configuration, for identification only
 */
export const StartFromTriggerNodeConfigSchema = z.object({});

/**
 * Continue from trigger node configuration schema
 */
export const ContinueFromTriggerNodeConfigSchema = z.object({
  variableCallback: z.object({
    includeVariables: z.array(z.string()).optional(),
    includeAll: z.boolean().optional(),
  }).optional(),
  conversationHistoryCallback: z.object({
    operation: z.enum(["TRUNCATE", "FILTER"]),
    truncate: TruncateMessageOperationSchema.and(
      z.object({
        lastN: z.number().int().positive().optional(),
        lastNByRole: z.object({
          role: z.enum(["system", "user", "assistant", "tool"]),
          count: z.number().int().positive(),
        }).optional(),
      })
    ).optional(),
    filter: FilterMessageOperationSchema.and(
      z.object({
        byRole: z.enum(["system", "user", "assistant", "tool"]).optional(),
        range: z.object({
          start: z.number().int().nonnegative(),
          end: z.number().int().positive(),
        }).optional(),
      })
    ).optional(),
  }).optional(),
  callbackOptions: z.object({
    visibleOnly: z.boolean().optional(),
  }).optional(),
});

/**
 * Type guards for runtime type checking
 */
export const isSubgraphNodeConfig = (config: unknown): config is z.infer<typeof SubgraphNodeConfigSchema> => {
  return SubgraphNodeConfigSchema.safeParse(config).success;
};

export const isStartFromTriggerNodeConfig = (config: unknown): config is z.infer<typeof StartFromTriggerNodeConfigSchema> => {
  return StartFromTriggerNodeConfigSchema.safeParse(config).success;
};

export const isContinueFromTriggerNodeConfig = (config: unknown): config is z.infer<typeof ContinueFromTriggerNodeConfigSchema> => {
  return ContinueFromTriggerNodeConfigSchema.safeParse(config).success;
};
