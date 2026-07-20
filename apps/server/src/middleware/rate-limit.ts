/**
 * Rate Limiting Middleware
 *
 * Simple in-memory rate limiter for the server.
 * Limits requests per IP address within a configurable time window.
 */

import type { Request, Response, NextFunction } from "express";

interface RateLimitEntry {
  count: number;
  resetAt: number;
}

export interface RateLimitConfig {
  enabled: boolean;
  windowMs: number;
  maxRequests: number;
  excludedPaths: string[];
}

const DEFAULT_CONFIG: RateLimitConfig = {
  enabled: process.env["RATE_LIMIT_ENABLED"] === "true",
  windowMs: parseInt(process.env["RATE_LIMIT_WINDOW_MS"] || "60000", 10),
  maxRequests: parseInt(process.env["RATE_LIMIT_MAX_REQUESTS"] || "100", 10),
  excludedPaths: ["/health", "/api/v1/info", "/", "/ws", "/api/v1/events/stream"],
};

let config: RateLimitConfig = { ...DEFAULT_CONFIG };
const clients: Map<string, RateLimitEntry> = new Map();

// Periodic cleanup every 5 minutes
setInterval(() => {
  const now = Date.now();
  for (const [key, entry] of clients) {
    if (entry.resetAt <= now) {
      clients.delete(key);
    }
  }
}, 5 * 60 * 1000);

/**
 * Update rate limit configuration at runtime
 */
export function configureRateLimit(cfg: Partial<RateLimitConfig>): void {
  config = { ...config, ...cfg };
}

/**
 * Get current rate limit configuration
 */
export function getRateLimitConfig(): RateLimitConfig {
  return { ...config };
}

/**
 * Rate limiting middleware factory
 */
export function rateLimitMiddleware(req: Request, res: Response, next: NextFunction): void {
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

  const key = req.ip || req.socket.remoteAddress || "unknown";
  const now = Date.now();

  let entry = clients.get(key);
  if (!entry || entry.resetAt <= now) {
    entry = { count: 0, resetAt: now + config.windowMs };
    clients.set(key, entry);
  }

  entry.count++;

  // Set rate limit headers
  res.setHeader("X-RateLimit-Limit", config.maxRequests);
  res.setHeader("X-RateLimit-Remaining", Math.max(0, config.maxRequests - entry.count));
  res.setHeader("X-RateLimit-Reset", Math.ceil(entry.resetAt / 1000));

  if (entry.count > config.maxRequests) {
    res.status(429).json({
      success: false,
      error: {
        code: "RATE_LIMITED",
        message: "Too many requests. Please try again later.",
        retryAfter: Math.ceil((entry.resetAt - now) / 1000),
      },
    });
    return;
  }

  next();
}

/**
 * Get current rate limit stats for monitoring
 */
export function getRateLimitStats(): {
  totalClients: number;
  config: RateLimitConfig;
} {
  return {
    totalClients: clients.size,
    config: { ...config },
  };
}