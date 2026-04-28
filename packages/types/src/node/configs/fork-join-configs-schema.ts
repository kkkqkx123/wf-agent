/**
 * Zod Schemas for Fork/Join Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";

/**
 * Fork path schema
 */
const forkPathSchema = z.object({
  pathId: z.string().min(1, "Path ID is required"),
  childNodeId: z.string().min(1, "Child node ID is required"),
});

/**
 * Fork node configuration schema
 */
export const ForkNodeConfigSchema = z
  .object({
    forkPaths: z.array(forkPathSchema).min(1, "Fork paths must be a non-empty array"),
    forkStrategy: z.enum(["serial", "parallel"], {
      message: "Fork strategy must be one of: serial, parallel",
    }),
  })
  .refine(
    (data) => {
      // Verify the uniqueness of pathId
      const pathIds = data.forkPaths.map((p) => p.pathId);
      const uniquePathIds = new Set(pathIds);
      return pathIds.length === uniquePathIds.size;
    },
    { message: "Fork path IDs must be unique", path: ["forkPaths"] },
  );

/**
 * Join node configuration schema
 *
 * Description:
 * - `childThreadIds` is not defined in the schema because the child thread IDs are dynamic values that are generated at runtime.
 *   They are stored in the execution context when the FORK node is executed and are read from the execution context when the JOIN node is executed.
 * - `timeout` can be 0 (no timeout) or a positive number. A value of 0 indicates that the node will always wait without setting a timeout.
 * - `forkPathIds` must match exactly the `pathId` in the `forkPaths` of the paired FORK node, including the order.
 * - `mainPathId` specifies the path of the main thread and must be one of the values in `forkPathIds`.
 */
export const JoinNodeConfigSchema = z
  .object({
    forkPathIds: z.array(z.string()).min(1, "Fork path IDs must be a non-empty array"),
    joinStrategy: z.enum(
      ["ALL_COMPLETED", "ANY_COMPLETED", "ALL_FAILED", "ANY_FAILED", "SUCCESS_COUNT_THRESHOLD"],
      {
        message: "Join strategy must be one of: ALL_COMPLETED, ANY_COMPLETED, ALL_FAILED, ANY_FAILED, SUCCESS_COUNT_THRESHOLD",
      },
    ),
    threshold: z.number().positive("Threshold must be positive").optional(),
    timeout: z.number().nonnegative("Timeout must be non-negative").optional(),
    mainPathId: z.string().min(1, "Main path ID is required"),
  })
  .refine(
    (data) => {
      if (data.joinStrategy === "SUCCESS_COUNT_THRESHOLD" && data.threshold === undefined) {
        return false;
      }
      return true;
    },
    {
      message: "Join node must have a valid threshold when using SUCCESS_COUNT_THRESHOLD strategy",
      path: ["threshold"],
    },
  )
  .refine(
    (data) => {
      return data.forkPathIds.includes(data.mainPathId);
    },
    { message: "mainPathId must be one of the forkPathIds", path: ["mainPathId"] },
  );

/**
 * Type guards for runtime type checking
 */
export const isForkNodeConfig = (config: unknown): config is z.infer<typeof ForkNodeConfigSchema> => {
  return ForkNodeConfigSchema.safeParse(config).success;
};

export const isJoinNodeConfig = (config: unknown): config is z.infer<typeof JoinNodeConfigSchema> => {
  return JoinNodeConfigSchema.safeParse(config).success;
};