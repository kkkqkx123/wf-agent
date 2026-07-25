/**
 * Runtime Configuration Schema
 * Shared Zod schema definitions for application configuration validation.
 *
 * Provides DefaultAppConfigSchema that both cli-app and server can import
 * directly, eliminating identical schema duplication.
 */

import { z } from "zod";
import type { DefaultAppConfig } from "./types.js";

// Import shared schemas from @wf-agent/types
import {
  StorageConfigSchema,
  OutputConfigSchema,
  PresetsConfigSchema,
} from "@wf-agent/types";

/**
 * Default Application Configuration Schema.
 * Covers the common fields shared by cli-app and server.
 * Each app can extend or alias this for its own schema.
 */
export const DefaultAppConfigSchema = z.object({
  defaultTimeout: z.number().positive().default(30000),
  verbose: z.boolean().default(false),
  debug: z.boolean().default(false),
  logLevel: z.enum(["error", "warn", "info", "debug"]).default("warn"),
  outputFormat: z.enum(["json", "table", "plain"]).default("table"),
  maxConcurrentExecutions: z.number().positive().default(5),
  executionMode: z.enum(["interactive", "headless", "programmatic"]).optional(),
  storage: StorageConfigSchema.optional(),
  output: OutputConfigSchema.optional(),
  presets: PresetsConfigSchema.optional(),
}) satisfies z.ZodType<DefaultAppConfig>;

/**
 * Type inference from the default schema (for runtime validation).
 */
export type DefaultAppConfigValidated = z.infer<typeof DefaultAppConfigSchema>;
