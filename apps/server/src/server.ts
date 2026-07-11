/**
 * Express Server
 *
 * Core HTTP server implementation using Express.js 5.2.1
 * Handles middleware configuration, route setup, and lifecycle management.
 */

import express, { type Express, type Request, type Response, type NextFunction } from "express";
import http from "http";
import type { ServerDependencyContainer } from "./services/container.js";
import { createWorkflowRoutes, createExecutionRoutes, createEventsRoutes } from "./routes/index.js";
import { getOutput } from "./utils/output.js";

export interface ServerConfig {
  port?: number;
  host?: string;
  corsOrigins?: string[];
}

/**
 * Express Server for Modular Agent Framework
 */
export class Server {
  private app: Express;
  private httpServer: http.Server | null = null;
  private container: ServerDependencyContainer;
  private config: ServerConfig;
  private logger = getOutput();

  constructor(container: ServerDependencyContainer, config?: ServerConfig) {
    this.container = container;
    this.config = {
      port: config?.port || 3000,
      host: config?.host || "0.0.0.0",
      corsOrigins: config?.corsOrigins || ["*"],
    };

    this.app = express();
    this.setupMiddleware();
  }

  /**
   * Setup middleware
   */
  private setupMiddleware(): void {
    // Body parser middleware
    this.app.use(express.json());
    this.app.use(express.urlencoded({ extended: true }));

    // CORS middleware
    this.app.use((req: Request, res: Response, next: NextFunction): void => {
      const origin = req.headers.origin;
      if (
        this.config.corsOrigins?.includes("*") ||
        (origin && this.config.corsOrigins?.includes(origin))
      ) {
        res.header("Access-Control-Allow-Origin", origin || "*");
      }
      res.header("Access-Control-Allow-Methods", "GET,POST,PUT,DELETE,OPTIONS");
      res.header("Access-Control-Allow-Headers", "Content-Type,Authorization");
      if (req.method === "OPTIONS") {
        res.sendStatus(200);
        return;
      }
      next();
    });

    // Request logging middleware
    this.app.use((req: Request, res: Response, next: NextFunction): void => {
      const startTime = Date.now();

      res.on("finish", () => {
        const duration = Date.now() - startTime;
        const level =
          res.statusCode >= 400
            ? res.statusCode >= 500
              ? "error"
              : "warn"
            : "debug";
        const emoji =
          res.statusCode >= 400
            ? res.statusCode >= 500
              ? "❌"
              : "⚠️"
            : "✓";
        this.logger[level === "error" ? "errorLog" : level === "warn" ? "warn" : "debugLog"](
          `${emoji} ${req.method} ${req.path} - ${res.statusCode} (${duration}ms)`
        );
      });

      next();
    });
  }

  /**
   * Setup routes
   */
  private setupRoutes(): void {
    // Health check endpoint
    this.app.get("/health", (_req: Request, res: Response) => {
      res.json({
        status: "ok",
        timestamp: new Date().toISOString(),
      });
    });

    // API version endpoint
    this.app.get("/api/v1/info", (_req: Request, res: Response) => {
      res.json({
        name: "Modular Agent Framework Server",
        version: "1.0.0",
        apiVersion: "v1",
        timestamp: new Date().toISOString(),
      });
    });

    // Root endpoint
    this.app.get("/", (_req: Request, res: Response) => {
      res.json({
        message: "Modular Agent Framework Server",
        endpoints: {
          health: "/health",
          info: "/api/v1/info",
          workflows: "/api/v1/workflows",
          executions: "/api/v1/executions",
          events: "/api/v1/events",
        },
      });
    });

    // API v1 routes
    this.app.use("/api/v1/workflows", createWorkflowRoutes(this.container));
    this.app.use("/api/v1/executions", createExecutionRoutes(this.container));
    this.app.use("/api/v1/events", createEventsRoutes(this.container));

    // 404 handler
    this.app.use((_req: Request, res: Response) => {
      res.status(404).json({
        success: false,
        error: {
          code: "NOT_FOUND",
          message: `Endpoint not found: ${_req.method} ${_req.path}`,
        },
      });
    });

    // Error handler (must be last)
    this.app.use(
      (
        err: Error,
        _req: Request,
        res: Response,
        _next: NextFunction
      ) => {
        this.logger.errorLog(
          `Server error: ${err.message}\n${err.stack || ""}`
        );
        res.status(500).json({
          success: false,
          error: {
            code: "INTERNAL_ERROR",
            message:
              process.env["NODE_ENV"] === "production"
                ? "Internal server error"
                : err.message,
          },
        });
      }
    );
  }

  /**
   * Start the HTTP server
   */
  async start(): Promise<void> {
    try {
      this.setupRoutes();

      // Create HTTP server wrapping Express app
      this.httpServer = http.createServer(this.app);

      // Listen on configured port and host
      await new Promise<void>((resolve) => {
        this.httpServer!.listen(this.config.port, this.config.host, () => {
          this.logger.infoLog(
            `🚀 Server started on http://${this.config.host}:${this.config.port}`
          );
          resolve();
        });
      });

      // Setup graceful shutdown signals
      process.on("SIGTERM", () => this.shutdown());
      process.on("SIGINT", () => this.shutdown());
    } catch (error) {
      this.logger.errorLog(
        `Failed to start server: ${error instanceof Error ? error.message : String(error)}`
      );
      throw error;
    }
  }

  /**
   * Shutdown the server
   */
  async shutdown(): Promise<void> {
    try {
      this.logger.infoLog("🛑 Shutting down server...");

      if (this.httpServer) {
        await new Promise<void>((resolve, reject) => {
          this.httpServer!.close((error?: Error) => {
            if (error) reject(error);
            else resolve();
          });
        });
        this.logger.infoLog("✓ HTTP server closed");
      }

      await this.container.cleanup();
      this.logger.infoLog("✓ Container cleaned up");

      this.logger.infoLog("✅ Server shutdown complete");
      process.exit(0);
    } catch (error) {
      this.logger.errorLog(
        `Shutdown error: ${error instanceof Error ? error.message : String(error)}`
      );
      process.exit(1);
    }
  }

  /**
   * Get the Express app
   */
  getApp(): Express {
    return this.app;
  }

  /**
   * Get the HTTP server
   */
  getHttpServer(): http.Server | null {
    return this.httpServer;
  }

  /**
   * Get the container
   */
  getContainer(): ServerDependencyContainer {
    return this.container;
  }
}
