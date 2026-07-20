/**
 * Express Server
 *
 * Core HTTP server implementation using Express.js 5.2.1
 * Handles middleware configuration, route setup, and lifecycle management.
 */

import express, { type Express, type Request, type Response, type NextFunction } from "express";
import http from "http";
import type { ServerDependencyContainer } from "./services/container.js";
import {
  createWorkflowRoutes,
  createExecutionRoutes,
  createEventRoutes,
  createWorkflowVersionRoutes,
  createWorkflowGraphRoutes,
  createCheckpointRoutes,
  createToolRoutes,
  createTemplateRoutes,
  createScriptRoutes,
  createVariableRoutes,
  createTriggerRoutes,
  createMessageRoutes,
  createAgentLoopRoutes,
  createIterationRoutes,
  createAgentProfileRoutes,
  createLLMProfileRoutes,
  createSkillRoutes,
  createProgressRoutes,
  createComparisonRoutes,
  createMetricsRoutes,
  createSearchRoutes,
  createStorageRoutes,
  createSSERoutes,
  createInteractionRoutes,
} from "./routes/index.js";
import { WSManager } from "./services/ws-manager.js";
import { SSEBroadcaster } from "./services/sse-broadcaster.js";
import { EventManager } from "./services/event-manager.js";
import { InteractionService } from "./services/interaction-service.js";
import { authMiddleware } from "./middleware/auth.js";
import { rateLimitMiddleware } from "./middleware/rate-limit.js";
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
  private startTime: number = 0;

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
  private wsManager: WSManager | null = null;
  private sseBroadcaster: SSEBroadcaster | null = null;
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

    // Auth middleware
    this.app.use(authMiddleware);

    // Rate limiting middleware
    this.app.use(rateLimitMiddleware);

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
  private setupRoutes(interactionService?: InteractionService): void {
    // Health check endpoint
    this.app.get("/health", (_req: Request, res: Response) => {
      const uptime = this.startTime ? Math.floor((Date.now() - this.startTime) / 1000) : 0;
      const memoryUsage = process.memoryUsage();
      res.json({
        status: "ok",
        timestamp: new Date().toISOString(),
        uptime: `${uptime}s`,
        version: "1.0.0",
        wsConnections: this.wsManager?.getClientCount() || 0,
        memory: {
          rss: `${Math.round(memoryUsage.rss / 1024 / 1024)}MB`,
          heapTotal: `${Math.round(memoryUsage.heapTotal / 1024 / 1024)}MB`,
          heapUsed: `${Math.round(memoryUsage.heapUsed / 1024 / 1024)}MB`,
        },
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
          "sse/stream (SSE)": "/api/v1/sse/stream?executionId=<id>",
          "websocket (WS)": "/api/v1/ws",
        },
      });
    });

    // API v1 routes
    this.app.use("/api/v1/workflows", createWorkflowRoutes(this.container));
    this.app.use("/api/v1/executions", createExecutionRoutes(this.container));
    this.app.use("/api/v1/events", createEventRoutes(this.container));
    this.app.use("/api/v1/workflows/versions", createWorkflowVersionRoutes(this.container));
    this.app.use("/api/v1/workflows/graph", createWorkflowGraphRoutes(this.container));
    this.app.use("/api/v1/checkpoints", createCheckpointRoutes(this.container));
    this.app.use("/api/v1/tools", createToolRoutes(this.container));
    this.app.use("/api/v1/templates", createTemplateRoutes(this.container));
    this.app.use("/api/v1/scripts", createScriptRoutes(this.container));
    this.app.use("/api/v1/variables", createVariableRoutes(this.container));
    this.app.use("/api/v1/triggers", createTriggerRoutes(this.container));
    this.app.use("/api/v1/messages", createMessageRoutes(this.container));
    this.app.use("/api/v1/agent-loops", createAgentLoopRoutes(this.container));
    this.app.use("/api/v1/agent-loops/iterations", createIterationRoutes(this.container));
    this.app.use("/api/v1/agent-profiles", createAgentProfileRoutes(this.container));
    this.app.use("/api/v1/llm-profiles", createLLMProfileRoutes(this.container));
    this.app.use("/api/v1/skills", createSkillRoutes(this.container));
    this.app.use("/api/v1/executions/progress", createProgressRoutes(this.container));
    this.app.use("/api/v1/executions/compare", createComparisonRoutes(this.container));
    this.app.use("/api/v1/metrics", createMetricsRoutes(this.container));
    this.app.use("/api/v1/search", createSearchRoutes(this.container));
    this.app.use("/api/v1/storage", createStorageRoutes(this.container));
    this.app.use("/api/v1/sse/stream", createSSERoutes(this.sseBroadcaster!));
    this.app.use("/api/v1/interactions", createInteractionRoutes(interactionService!));

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
      // Initialize EventManager (DI-managed, no longer singleton)
      const eventManager = new EventManager();

      // Initialize SSE broadcaster before routes so createSSERoutes can use it
      this.sseBroadcaster = new SSEBroadcaster(eventManager);

      // Initialize InteractionService before routes so createInteractionRoutes can use it
      const interactionService = new InteractionService(eventManager);

      this.setupRoutes(interactionService);

      // Create HTTP server wrapping Express app
      this.httpServer = http.createServer(this.app);

      // Initialize WebSocket server on /api/v1/ws
      this.wsManager = new WSManager();
      this.wsManager.setPath("/api/v1/ws");
      this.wsManager.setup(this.httpServer);

      // Register broadcasters with EventManager
      eventManager.registerBroadcaster(this.wsManager);
      eventManager.registerBroadcaster(this.sseBroadcaster);
      this.logger.debugLog("Broadcasters registered with EventManager (WS, SSE)");

      // Register services in the DI container
      this.container.registerService("eventManager", eventManager);
      this.container.registerService("wsManager", this.wsManager);

      // Listen on configured port and host
      await new Promise<void>((resolve) => {
        this.httpServer!.listen(this.config.port, this.config.host, () => {
          this.startTime = Date.now();
          this.logger.infoLog(
            `🚀 Server started on http://${this.config.host}:${this.config.port}`
          );
          this.logger.infoLog(`🔌 WebSocket available at ws://${this.config.host}:${this.config.port}/api/v1/ws`);
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
   * Shutdown the server gracefully
   */
  async shutdown(): Promise<void> {
    this.logger.infoLog("🛑 Shutting down server...");

    // Force shutdown after timeout if graceful shutdown hangs
    const forceExitTimer = setTimeout(() => {
      this.logger.errorLog("⚠️ Graceful shutdown timeout, forcing exit");
      process.exit(1);
    }, 30000);

    try {
      // Stop accepting new connections
      if (this.httpServer) {
        await new Promise<void>((resolve, reject) => {
          this.httpServer!.close((error?: Error) => {
            if (error) reject(error);
            else resolve();
          });
        });
        this.logger.infoLog("✓ HTTP server closed");
      }

      // Cleanup WebSocket connections
      if (this.wsManager) {
        this.wsManager.cleanup();
        this.logger.infoLog("✓ WebSocket server closed");
      }

      // Cleanup SSE connections
      if (this.sseBroadcaster) {
        this.sseBroadcaster.cleanup();
        this.logger.infoLog("✓ SSE broadcaster closed");
      }

      await this.container.cleanup();
      this.logger.infoLog("✓ Container cleaned up");

      clearTimeout(forceExitTimer);
      this.logger.infoLog("✅ Server shutdown complete");
      process.exit(0);
    } catch (error) {
      clearTimeout(forceExitTimer);
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
