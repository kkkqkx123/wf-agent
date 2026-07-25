/**
 * CLI Configuration Schema
 * Zod schema definitions for configuration validation.
 *
 * Reuses DefaultAppConfigSchema from @wf-agent/runtime to eliminate
 * identical schema duplication between cli-app and server.
 */

import { z } from "zod";
import { DefaultAppConfigSchema } from "@wf-agent/runtime";
import type { CLIConfig } from "./types.js";

/**
 * Complete CLI Configuration Schema
 */
export const CLIConfigSchema = DefaultAppConfigSchema satisfies z.ZodType<CLIConfig>;

/**
 * Type inference from schema (for runtime validation)
 */
export type CLIConfigValidated = z.infer<typeof CLIConfigSchema>;
