/**
 * Auth Middleware
 *
 * API Key authentication middleware for the server.
 * Reads API keys from environment variables and validates incoming requests.
 * Supports both header-based (X-API-Key) and query-parameter-based auth.
 */

import type { Request, Response, NextFunction } from "express";

export interface AuthConfig {
  enabled: boolean;
  apiKeys: string[];
  headerName: string;
  allowQueryParam: boolean;
  queryParamName: string;
  excludedPaths: string[];
}

const DEFAULT_CONFIG: AuthConfig = {
  enabled: process.env["AUTH_ENABLED"] === "true",
  apiKeys: (process.env["API_KEYS"] || "").split(",").filter(Boolean),
  headerName: "x-api-key",
  allowQueryParam: true,
  queryParamName: "api_key",
  excludedPaths: ["/health", "/api/v1/info", "/", "/api/v1/ws", "/api/v1/sse/stream"],
};

let config: AuthConfig = { ...DEFAULT_CONFIG };

/**
 * Update auth configuration at runtime
 */
export function configureAuth(cfg: Partial<AuthConfig>): void {
  config = { ...config, ...cfg };
}

/**
 * Get current auth configuration
 */
export function getAuthConfig(): AuthConfig {
  return { ...config };
}

/**
 * Auth middleware factory
 * Returns Express middleware that validates API keys.
 */
export function authMiddleware(req: Request, res: Response, next: NextFunction): void {
  // Skip auth if disabled
  if (!config.enabled) {
    next();
    return;
  }

  // Skip excluded paths
  const path = req.path;
  for (const excludedPath of config.excludedPaths) {
    if (path === excludedPath || path.startsWith(excludedPath + "/")) {
      next();
      return;
    }
  }

  // Extract API key from header or query parameter
  let apiKey: string | undefined = req.headers[config.headerName] as string | undefined;

  if (!apiKey && config.allowQueryParam) {
    apiKey = req.query[config.queryParamName] as string | undefined;
  }

  // Validate
  if (!apiKey) {
    res.status(401).json({
      success: false,
      error: {
        code: "UNAUTHORIZED",
        message: "API key is required. Provide it via X-API-Key header or api_key query parameter.",
      },
    });
    return;
  }

  if (!config.apiKeys.includes(apiKey)) {
    res.status(403).json({
      success: false,
      error: {
        code: "FORBIDDEN",
        message: "Invalid API key.",
      },
    });
    return;
  }

  next();
}