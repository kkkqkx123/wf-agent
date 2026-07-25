/**
 * Child Executor
 *
 * Entry point for subprocess workflow execution (foreground/background modes).
 * Spawned via child_process.fork() by ExecutionService.
 *
 * IPC Protocol:
 *   Parent -> Child (process.argv): workflowId, input (JSON), mode, storageDir
 *   Child  -> Parent (process.send): { type, ... }
 *     - { type: 'status',   status: 'starting'|'running'|'completed'|'failed' }
 *     - { type: 'event',    event: BaseEvent }
 *     - { type: 'result',   result: WorkflowExecutionResult }
 *     - { type: 'error',    error: { message, code } }
 */

/* eslint-disable no-console */

import { createSDK, ExecuteWorkflowCommand, isSuccess, getData, getError, createExecutionScopedSubscription } from "@wf-agent/sdk/api";
import type { SDKInstance } from "@wf-agent/sdk/api";
import type { WorkflowExecutionResult, BaseEvent, NodeCompletedEvent, WorkflowExecutionFailedEvent } from "@wf-agent/types";
import * as fs from "fs";
import * as path from "path";

// ============================================
// Constants
// ============================================

const ENV_STORAGE_DIR = "CHILD_STORAGE_DIR";
const ENV_LOG_FILE = "CHILD_LOG_FILE";
const ENV_DEBUG = "CHILD_DEBUG";

// ============================================
// Types
// ============================================

interface ChildExecutorOptions {
  workflowId: string;
  input: Record<string, unknown>;
  mode: "foreground" | "background";
  storageDir?: string;
  logFile?: string;
  debug?: boolean;
}

// ============================================
// Logger
// ============================================

function log(...args: unknown[]): void {
  if (process.env[ENV_DEBUG]) {
    console.error("[child-executor]", ...args);
  }
}

// ============================================
// SDK Setup
// ============================================

/**
 * Create a lightweight SDK instance for the child process.
 * Uses memory storage by default (the parent process's storage is shared
 * at the execution/event level — the child needs only to read/write
 * execution state).
 */
async function createChildSDK(options: ChildExecutorOptions): Promise<SDKInstance> {
  log("Creating child SDK for workflow:", options.workflowId);

  const sdk = createSDK({
    debug: !!options.debug,
    logging: { level: options.debug ? "debug" : "warn" },
    // Use SDK defaults — the child only needs to execute the workflow
    // and report events. Persistent storage is the parent's responsibility.
    enableCheckpoints: false,
    enableValidation: false,
    presets: {
      contextCompression: { enabled: false },
      predefinedTools: { enabled: false },
      predefinedPrompts: { enabled: false },
    },
    mcp: { enabled: false },
    gracefulShutdown: { enabled: false },
  });

  await sdk.waitForReady();
  log("Child SDK ready");
  return sdk;
}

// ============================================
// Execution
// ============================================

/**
 * Execute workflow and send events via IPC.
 * Returns the final execution result.
 */
async function executeWorkflow(
  sdk: SDKInstance,
  workflowId: string,
  input: Record<string, unknown>,
  mode: "foreground" | "background",
): Promise<WorkflowExecutionResult> {
  sendIPCMessage({ type: "status", status: "starting" });

  const factory = sdk.getFactory();
  const dependencies = factory.getDependencies();

  // Step 1: Execute workflow
  log("Executing workflow:", workflowId);
  const command = new ExecuteWorkflowCommand({ workflowId, options: { input } }, dependencies);
  const result = await sdk.executeCommand(command);

  if (!isSuccess(result)) {
    const error = getError(result);
    sendIPCMessage({
      type: "error",
      error: { message: error?.message || "Workflow execution failed", code: "EXECUTION_FAILED" },
    });
    throw error;
  }

  const executionResult = getData(result) as WorkflowExecutionResult;
  const executionId = executionResult?.executionId || "unknown";

  sendIPCMessage({ type: "status", status: "running", executionId });

  // Step 2: Listen for events
  await new Promise<void>((resolve, reject) => {
    let nodeStartedUnsub: (() => void) | undefined;
    let nodeCompletedUnsub: (() => void) | undefined;

    // Subscribe to node started events
    nodeStartedUnsub = createExecutionScopedSubscription(
      executionId,
      "NODE_STARTED",
      (event: BaseEvent) => {
        sendIPCMessage({ type: "event", event: { ...event, eventType: "NODE_STARTED" } });
        if (mode === "foreground") {
          log(`→ Node started: ${(event as any).nodeId}`);
        }
      },
      dependencies,
    ).subscribe();

    // Subscribe to node completed events
    nodeCompletedUnsub = createExecutionScopedSubscription(
      executionId,
      "NODE_COMPLETED",
      (event: BaseEvent) => {
        const nodeEvent = event as NodeCompletedEvent;
        sendIPCMessage({ type: "event", event: { ...event, eventType: "NODE_COMPLETED" } });
        if (mode === "foreground") {
          log(`✓ Node completed: ${nodeEvent.nodeId}`);
        }
      },
      dependencies,
    ).subscribe();

    // Subscribe to workflow completed
    createExecutionScopedSubscription(
      executionId,
      "WORKFLOW_EXECUTION_COMPLETED",
      (event: BaseEvent) => {
        sendIPCMessage({ type: "event", event: { ...event, eventType: "WORKFLOW_EXECUTION_COMPLETED" } });
        if (nodeStartedUnsub) nodeStartedUnsub();
        if (nodeCompletedUnsub) nodeCompletedUnsub();
        sendIPCMessage({ type: "status", status: "completed", executionId });
        resolve();
      },
      dependencies,
    ).subscribe();

    // Subscribe to workflow failed
    createExecutionScopedSubscription(
      executionId,
      "WORKFLOW_EXECUTION_FAILED",
      (event: BaseEvent) => {
        sendIPCMessage({ type: "event", event: { ...event, eventType: "WORKFLOW_EXECUTION_FAILED" } });
        if (nodeStartedUnsub) nodeStartedUnsub();
        if (nodeCompletedUnsub) nodeCompletedUnsub();
        sendIPCMessage({ type: "status", status: "failed", executionId });
        reject(new Error(String((event as WorkflowExecutionFailedEvent).error) || "Execution failed"));
      },
      dependencies,
    ).subscribe();
  });

  return executionResult;
}

// ============================================
// IPC Helpers
// ============================================

/**
 * Send a message to the parent process via IPC.
 * Falls back to console.log if no parent IPC available.
 */
function sendIPCMessage(msg: Record<string, unknown>): void {
  try {
    if (process.send) {
      process.send(msg);
    }
  } catch {
    // IPC channel may be closed; fall back to stdout
    console.log(JSON.stringify(msg));
  }
}

// ============================================
// Logging Helpers (background mode)
// ============================================

function writeToLogFile(logFile: string | undefined, message: string): void {
  if (!logFile) return;
  try {
    const dir = path.dirname(logFile);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
    fs.appendFileSync(logFile, message + "\n", "utf-8");
  } catch {
    // Silently ignore write errors in child process
  }
}

// ============================================
// Arguments Parsing
// ============================================

function parseArgs(): ChildExecutorOptions {
  // args: [node, child-executor.js, workflowId, inputJSON, mode, storageDir?, logFile?]
  const [, , workflowId, inputRaw, mode, storageDir, logFile] = process.argv;

  let input: Record<string, unknown> = {};
  if (inputRaw) {
    try {
      input = JSON.parse(inputRaw);
    } catch {
      // Invalid JSON, use empty input
    }
  }

  return {
    workflowId: workflowId || "",
    input,
    mode: (mode as "foreground" | "background") || "foreground",
    storageDir: storageDir || process.env[ENV_STORAGE_DIR],
    logFile: logFile || process.env[ENV_LOG_FILE],
    debug: !!process.env[ENV_DEBUG],
  };
}

// ============================================
// Main
// ============================================

async function main(): Promise<void> {
  const options = parseArgs();

  log("Child executor started:", {
    workflowId: options.workflowId,
    mode: options.mode,
    storageDir: options.storageDir,
    logFile: options.logFile,
  });

  if (!options.workflowId) {
    sendIPCMessage({ type: "error", error: { message: "workflowId is required", code: "INVALID_ARGS" } });
    process.exit(1);
  }

  try {
    // Step 1: Create SDK
    const sdk = await createChildSDK(options);

    // Step 2: Execute workflow
    log("Starting workflow execution...");
    const execResult = await executeWorkflow(sdk, options.workflowId, options.input, options.mode);

    // Step 3: Send final result
    sendIPCMessage({ type: "result", result: execResult });

    // Step 4: For background mode, also write to log file
    if (options.mode === "background" && options.logFile) {
      writeToLogFile(options.logFile, `[${new Date().toISOString()}] Workflow execution completed: ${execResult?.executionId}`);
    }

    // Step 5: Cleanup
    await sdk.destroy();

    log("Child executor completed successfully");
    process.exit(0);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    log("Child executor failed:", message);
    sendIPCMessage({ type: "error", error: { message, code: "CHILD_EXECUTOR_ERROR" } });
    process.exit(1);
  }
}

// Run only when not imported (i.e., only when spawned as a child process)
const isMainModule = process.argv[1]?.includes("child-executor");
if (isMainModule) {
  main().catch((error) => {
    console.error("[child-executor] Fatal error:", error);
    process.exit(1);
  });
}
