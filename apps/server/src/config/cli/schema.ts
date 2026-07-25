/**
 * Server Configuration Schema
 * Zod schema definitions for configuration validation.
 *
 * Reuses DefaultAppConfigSchema from @wf-agent/runtime to eliminate
 * identical schema duplication between cli-app and server.
 */

import { z } from "zod";
import { DefaultAppConfigSchema } from "@wf-agent/runtime";
import type { ServerConfig } from "./types.js";

/**
 * Complete Server Configuration Schema
 */
export const ServerConfigSchema = DefaultAppConfigSchema satisfies z.ZodType<ServerConfig>;

/**
 * Type inference from schema (for runtime validation)
 */
export type ServerConfigValidated = z.infer<typeof ServerConfigSchema>;
