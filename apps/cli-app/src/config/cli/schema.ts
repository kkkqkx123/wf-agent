/**
 * CLI Configuration Schema
 * Zod schema definitions for configuration validation.
 * 
 * This file defines only CLI-specific schemas and composes shared schemas
 * from @wf-agent/types to maintain a single source of truth.
 */

import { z } from "zod";
import type { CLIConfig } from "./types.js";

// Import shared schemas from types package
import {
  StorageConfigSchema,
  OutputConfigSchema,
  PresetsConfigSchema,
} from "@wf-agent/types";

/**
 * Complete Configuration Schema
 */
export const CLIConfigSchema = z.object({
  apiUrl: z.string().url().optional(),
  apiKey: z.string().optional(),
  defaultTimeout: z.number().positive().default(30000),
  verbose: z.boolean().default(false),
  debug: z.boolean().default(false),
  logLevel: z.enum(["error", "warn", "info", "debug"]).default("warn"),
  outputFormat: z.enum(["json", "table", "plain"]).default("table"),
  maxConcurrentExecutions: z.number().positive().default(5),
  storage: StorageConfigSchema.optional(),
  output: OutputConfigSchema.optional(),
  presets: PresetsConfigSchema.optional(),
}) satisfies z.ZodType<CLIConfig>;

/**
 * Type inference from schema (for runtime validation)
 */
export type CLIConfigValidated = z.infer<typeof CLIConfigSchema>;
