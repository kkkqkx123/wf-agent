/**
 * Zod Schema for EmbedGraph Node Configuration
 * Provides runtime validation that is synchronized with TypeScript type definitions
 */

import { z } from "zod";

/**
 * EmbedGraph node configuration schema
 */
export const EmbedGraphNodeConfigSchema = z.object({
  embedId: z.string().min(1, "Embed ID is required"),
});
