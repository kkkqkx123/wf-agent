/**
 * System Context Builder
 *
 * Builds stable system context that rarely changes during execution:
 * - Current time and timezone
 * - Environment information (OS, workspaces, language)
 *
 * These fragments are injected into the system message and enable
 * KV cache hits because they change infrequently.
 */

import type { DynamicContextConfig } from "@wf-agent/types";
import { generateCurrentTimeSection } from "./fragments/current-time.js";
import { generateEnvironmentSection, getDefaultEnvironmentInfo } from "./fragments/environment.js";
import { cleanupEmptyLines } from "./fragments/utils.js";

/**
 * Cache entry for system context prompt
 */
interface CacheEntry {
  /** Timestamp when the entry was created */
  createdAt: number;
  /** Cached content */
  content: string;
}

/**
 * In-memory cache for system context prompts
 *
 * Since current time and environment info rarely change, caching them
 * across iterations preserves KV cache efficiency and reduces CPU overhead.
 * The cache is keyed by a hash of the config and invalidated after TTL.
 */
const contextCache = new Map<string, CacheEntry>();

/** Default TTL for cache entries in milliseconds (60 seconds) */
const DEFAULT_CACHE_TTL_MS = 60_000;

/**
 * Generate a cache key from the config
 */
function getCacheKey(config?: DynamicContextConfig): string {
  if (!config) {
    return "__default__";
  }
  // Only include config fields that affect the output
  return JSON.stringify({
    includeCurrentTime: config.includeCurrentTime,
    includeEnvironmentInfo: config.includeEnvironmentInfo,
    customSections: config.customSections,
  });
}

/**
 * Build system context prompt
 *
 * Generates stable content for system message that can be cached.
 * Includes: time, environment information.
 *
 * Results are cached with a configurable TTL (default 60s) since
 * the content rarely changes between iterations.
 *
 * Note: Tool documentation should be injected by the application layer
 * when tools are available, as it's configuration-dependent.
 *
 * @param config Dynamic context configuration (controls which sections are included)
 * @param cacheTtlMs Cache TTL in milliseconds (default: 60000, set to 0 to disable caching)
 * @returns System context prompt string
 */
export async function buildSystemContextPrompt(
  config?: DynamicContextConfig,
  cacheTtlMs: number = DEFAULT_CACHE_TTL_MS,
): Promise<string> {
  // Check cache if TTL > 0
  if (cacheTtlMs > 0) {
    const cacheKey = getCacheKey(config);
    const cached = contextCache.get(cacheKey);
    if (cached && Date.now() - cached.createdAt < cacheTtlMs) {
      return cached.content;
    }
  }

  const sections: string[] = [];

  // 1. Add current time (disabled by default, see DynamicContextConfig)
  if (config?.includeCurrentTime === true) {
    sections.push(generateCurrentTimeSection());
  }

  // 2. Add environment information (disabled by default, see DynamicContextConfig)
  if (config?.includeEnvironmentInfo === true) {
    const envInfo = getDefaultEnvironmentInfo();
    const envSection = generateEnvironmentSection(envInfo);
    if (envSection) {
      sections.push(envSection);
    }
  }

  // 3. Custom sections
  if (config?.customSections) {
    for (const value of Object.values(config.customSections)) {
      sections.push(value);
    }
  }

  // Combine all sections
  const combinedPrompt = sections.filter(Boolean).join("\n\n");
  const result = cleanupEmptyLines(combinedPrompt);

  // Update cache if TTL > 0
  if (cacheTtlMs > 0) {
    const cacheKey = getCacheKey(config);
    contextCache.set(cacheKey, {
      createdAt: Date.now(),
      content: result,
    });
  }

  return result;
}
