#!/usr/bin/env node

/**
 * Modular Agent Framework - Unified Server
 *
 * HTTP Server providing unified API for Web and VSCode WebView frontends.
 * Implements multi-adapter architecture from CLI-app, exposing resources via REST API.
 *
 * Features:
 * - Complete SDK initialization and lifecycle management
 * - Express.js 5.2.1 HTTP server
 * - Dependency injection container
 * - Graceful shutdown handling (via @wf-agent/runtime/lifecycle)
 * - Structured logging
 * - CORS support
 */

import "dotenv/config.js";
import { getOutput } from "./utils/output.js";
import { initializeFormatter } from "./utils/formatter.js";
import { loadConfigWithEnvOverride } from "./config/index.js";
import { bootstrapSDK, gracefulShutdown } from "./sdk-bootstrap.js";
import { registerShutdownHandlers } from "@wf-agent/runtime/lifecycle";
import { initializeContainer } from "./services/container.js";
import { WorkflowAdapter } from "./adapters/workflow-adapter.js";
import { WorkflowVersionAdapter } from "./adapters/workflow-version-adapter.js";
import { WorkflowGraphAdapter } from "./adapters/workflow-graph-adapter.js";
import { WorkflowExecutionCheckpointAdapter } from "./adapters/workflow-execution-checkpoint-adapter.js";
import { ToolAdapter } from "./adapters/tool-adapter.js";
import { TemplateAdapter } from "./adapters/template-adapter.js";
import { ScriptAdapter } from "./adapters/script-adapter.js";
import { VariableAdapter } from "./adapters/variable-adapter.js";
import { TriggerAdapter } from "./adapters/trigger-adapter.js";
import { EventAdapter } from "./adapters/event-adapter.js";
import { MessageAdapter } from "./adapters/message-adapter.js";
import { AgentLoopAdapter } from "./adapters/agent-loop-adapter.js";
import { IterationAnalysisAdapter } from "./adapters/iteration-analysis-adapter.js";
import { AgentProfileAdapter } from "./adapters/agent-profile-adapter.js";
import { LLMProfileAdapter } from "./adapters/llm-profile-adapter.js";
import { SkillAdapter } from "./adapters/skill-adapter.js";
import { ProgressTrackingAdapter } from "./adapters/progress-tracking-adapter.js";
import { ExecutionComparisonAdapter } from "./adapters/execution-comparison-adapter.js";
import { MetricsAdapter } from "./adapters/metrics-adapter.js";
import { SearchAdapter } from "./adapters/search-adapter.js";
import { StorageDiagnosticsAdapter } from "./adapters/storage-diagnostics-adapter.js";
import { ExecutionService } from "./services/execution-service.js";
import { EventManager } from "./services/event-manager.js";
import { Server } from "./server.js";

// Global instances
let sdkInstance: any = null;
let server: Server | null = null;

/**
 * Main bootstrap function
 */
async function bootstrap(): Promise<void> {
  try {
    // 0. Initialize output system
    const output = getOutput();
    output.infoLog(
      "🌟 Starting Modular Agent Framework Server (Phase 1: Bootstrap)"
    );

    // 1. Load configuration with environment overrides
    output.infoLog("📋 Loading configuration...");
    const config = await loadConfigWithEnvOverride(
      process.env["CONFIG_PATH"]
    );
    output.debugLog(`Configuration loaded: ${JSON.stringify(config, null, 2)}`);

    // 2. Initialize formatter
    initializeFormatter(output.colorEnabled);

    // 3. Bootstrap SDK
    output.infoLog("🔧 Bootstrapping SDK...");
    const sdkResult = await bootstrapSDK(config);
    sdkInstance = sdkResult.sdk;

    // 4. Initialize dependency container
    output.infoLog("📦 Initializing dependency container...");
    const container = initializeContainer(sdkInstance, config);

    // Register adapters
    container.registerAdapter("workflow", new WorkflowAdapter(sdkInstance));
    container.registerAdapter("workflow-version", new WorkflowVersionAdapter(sdkInstance));
    container.registerAdapter("workflow-graph", new WorkflowGraphAdapter(sdkInstance));
    container.registerAdapter("checkpoint", new WorkflowExecutionCheckpointAdapter(sdkInstance));
    container.registerAdapter("tool", new ToolAdapter(sdkInstance));
    container.registerAdapter("template", new TemplateAdapter(sdkInstance));
    container.registerAdapter("script", new ScriptAdapter(sdkInstance));
    container.registerAdapter("variable", new VariableAdapter(sdkInstance));
    container.registerAdapter("trigger", new TriggerAdapter(sdkInstance));
    container.registerAdapter("event", new EventAdapter(sdkInstance));
    container.registerAdapter("message", new MessageAdapter(sdkInstance));
    container.registerAdapter("agent-loop", new AgentLoopAdapter(sdkInstance));
    container.registerAdapter("iteration-analysis", new IterationAnalysisAdapter(sdkInstance));
    container.registerAdapter("agent-profile", new AgentProfileAdapter(sdkInstance));
    container.registerAdapter("llm-profile", new LLMProfileAdapter(sdkInstance));
    container.registerAdapter("skill", new SkillAdapter(sdkInstance));
    container.registerAdapter("progress", new ProgressTrackingAdapter(sdkInstance));
    container.registerAdapter("comparison", new ExecutionComparisonAdapter(sdkInstance));
    container.registerAdapter("metrics", new MetricsAdapter(sdkInstance));
    container.registerAdapter("search", new SearchAdapter(sdkInstance));
    container.registerAdapter("storage", new StorageDiagnosticsAdapter(sdkInstance));

    // Register services
    container.registerService("execution", new ExecutionService(sdkInstance, new EventManager()));

    output.debugLog(
      `Container initialized with adapters: ${container.getAdapterNames().join(", ")}`
    );
    output.debugLog(
      `Container initialized with services: ${container.getServiceNames().join(", ")}`
    );

    // 5. Create and start HTTP server
    output.infoLog("🚀 Creating HTTP server...");
    const serverPort = parseInt(process.env["SERVER_PORT"] || "3000", 10);
    const serverHost = process.env["SERVER_HOST"] || "0.0.0.0";

    server = new Server(container, {
      port: serverPort,
      host: serverHost,
      corsOrigins: process.env["CORS_ORIGINS"]?.split(",") || ["*"],
    });

    await server.start();

    output.infoLog("✅ Server fully initialized and ready");
    output.infoLog(
      `   API Base URL: http://${serverHost}:${serverPort}/api/v1`
    );
    output.infoLog(
      `   Health Check: http://${serverHost}:${serverPort}/health`
    );
  } catch (error) {
    const output = getOutput();
    output.errorLog(
      `Bootstrap failed: ${error instanceof Error ? error.message : String(error)}`
    );
    if (error instanceof Error && error.stack) {
      output.debugLog(error.stack);
    }
    process.exit(1);
  }
}

/**
 * Perform complete shutdown
 * Uses runtime's gracefulShutdown which triggers the onDestroy hook
 * (closing storage manager) and accepts an onBeforeShutdown callback
 * for HTTP server shutdown.
 */
async function performShutdown(): Promise<void> {
  try {
    if (sdkInstance) {
      // gracefulShutdown will:
      // 1. Call onBeforeShutdown (stop HTTP server)
      // 2. Destroy SDK (triggers onDestroy hook which closes storage)
      await gracefulShutdown(sdkInstance, async () => {
        if (server) {
          await server.shutdown();
        }
      });
    }
  } catch (error) {
    const output = getOutput();
    output.errorLog(
      `Shutdown error: ${error instanceof Error ? error.message : String(error)}`
    );
    process.exit(1);
  }
}

// Register shutdown handlers using runtime lifecycle
registerShutdownHandlers(performShutdown);

/**
 * Handle uncaught exceptions
 */
process.on("uncaughtException", (error) => {
  const output = getOutput();
  output.errorLog(`Uncaught exception: ${error.message}`);
  output.errorLog(error.stack || "");
  process.exit(1);
});

/**
 * Handle unhandled rejections
 */
process.on("unhandledRejection", (reason, promise) => {
  const output = getOutput();
  output.errorLog(`Unhandled rejection in ${promise}: ${reason}`);
  process.exit(1);
});

// Start the server
bootstrap().catch((error) => {
  const output = getOutput();
  output.errorLog(
    `Fatal error: ${error instanceof Error ? error.message : String(error)}`
  );
  process.exit(1);
});

export {};
