/**
 * Zod Schemas for Interaction Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";

/**
 * Variable update configuration schema
 */
const variableUpdateConfigSchema = z.object({
  variableName: z.string().min(1, { message: "Variable name is required" }),
  expression: z.string().min(1, { message: "Expression is required" }),
  scope: z.enum(["global", "workflowExecution", "subgraph", "loop"], {
    message: "Variable scope must be one of: global, workflowExecution, subgraph, loop",
  }),
});

/**
 * Message configuration schema
 */
const messageConfigSchema = z.object({
  role: z.literal("user", { message: 'Message role must be "user"' }),
  contentTemplate: z.string().min(1, { message: "Content template is required" }),
});

/**
 * User interaction node configuration schema
 *
 * Description: Define the business semantics of user interactions without application layer implementation details
 */
export const UserInteractionNodeConfigSchema = z
  .object({
    operationType: z.enum(["UPDATE_VARIABLES", "ADD_MESSAGE"], {
      message: "Operation type must be one of: UPDATE_VARIABLES, ADD_MESSAGE",
    }),
    variables: z.array(variableUpdateConfigSchema).optional(),
    message: messageConfigSchema.optional(),
    prompt: z.string().min(1, { message: "Prompt is required" }),
    timeout: z.number().positive().optional(),
    metadata: z.record(z.string(), z.any()).optional(),
  })
  .refine(
    (data) => {
      // Verification: UPDATE_VARIABLES must have variables
      if (data.operationType === "UPDATE_VARIABLES") {
        return data.variables && data.variables.length > 0;
      }
      // Verification: ADD_MESSAGE must have a message parameter.
      if (data.operationType === "ADD_MESSAGE") {
        return data.message !== undefined;
      }
      return true;
    },
    {
      message:
        "Configuration must match operation type: UPDATE_VARIABLES requires variables, ADD_MESSAGE requires message",
      path: ["operationType"],
    },
  );

/**
 * Type guard for runtime type checking
 */
export const isUserInteractionNodeConfig = (
  config: unknown,
): config is z.infer<typeof UserInteractionNodeConfigSchema> => {
  return UserInteractionNodeConfigSchema.safeParse(config).success;
};