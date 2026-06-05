#!/usr/bin/env node

/**
 * Modular Agent CLI Application
 * Modular Agent Framework command-line application
 */

import { Command } from "commander";
import { initializeOutput, getOutput } from "./utils/output.js";
import { initializeFormatter } from "./utils/formatter.js";
import { initLogger, initSDKLogger } from "./utils/logger.js";
import { loadConfigWithEnvOverride } from "./config/index.js";
import { createSDK } from "@wf-agent/sdk/api";
import { ExitManager, isHeadlessMode, detectExecutionMode } from "./utils/exit-manager.js";
import {
  initializeStorageManager,
  closeStorageManager,
  getStorageManager,
} from "./storage/index.js";
import { createWorkflowCommands } from "./commands/workflow/index.js";
import { createWorkflowExecutionCommands } from "./commands/workflow-execution/index.js";
import { createCheckpointCommands } from "./commands/checkpoint/index.js";
import { createTemplateCommands } from "./commands/template/index.js";
import { createLLMProfileCommands } from "./commands/llm-profile/index.js";
import { createScriptCommands } from "./commands/script/index.js";
import { createToolCommands } from "./commands/tool/index.js";
import { createTriggerCommands } from "./commands/trigger/index.js";
import { createMessageCommands } from "./commands/message/index.js";
import { createVariableCommands } from "./commands/variable/index.js";
import { createEventCommands } from "./commands/event/index.js";
import { createHumanRelayCommands } from "./commands/human-relay/index.js";
import { createAgentCommands } from "./commands/agent/index.js";
import { createSkillCommands } from "./commands/skill/index.js";
import { createAgentProfileCommands } from "./commands/agent-profile/index.js";
import { createMetricsCommands } from "./commands/metrics/index.js";
import { CLIUserInteractionManager } from "./handlers/user-interaction/index.js";
import { initializeContainer, getContainer } from "./services/container.js";

// Create an instance of the main program.
const program = new Command();

// Global SDK instance (initialized in preAction hook)
let sdkInstance: import("@wf-agent/sdk").SDKInstance | null = null;

// Configure basic program information
program
  .name("modular-agent")
  .description("Modular Agent Framework - Command-line tool for the Modular Agent Framework")
  .version("1.0.0")
  .option("-v, --verbose", "Enable detailed output mode.")
  .option("-d, --debug", "Enable debug mode")
  .option("-l, --log-file <path>", "Specify the path to the log file.")
  .option("-c, --config <path>", "Specify the path to the configuration file.")
  .hook("preAction", async thisCommand => {
    // Initialize the output system before executing any command.
    const options = thisCommand.opts() as {
      verbose?: boolean;
      debug?: boolean;
      logFile?: string;
      config?: string;
    };

    // 0. Initialize TOML parser first (required for config loading)
    try {
      const { initializeTomlParser } = await import("@wf-agent/sdk/api");
      await initializeTomlParser();
    } catch (_error) {
      // Continue without TOML parser - will use JSON or defaults
    }

    // 1. Load the global configuration with environment variable overrides
    const config = await loadConfigWithEnvOverride(options.config);

    // 2. Initialize the output system (for unified management of stdout/stderr/log)
    const output = initializeOutput({
      logFile: options.logFile,
      verbose: options.verbose,
      debug: options.debug,
      outputDir: config.output?.dir,
      logFilePattern: config.output?.logFilePattern,
      enableLogTerminal: config.output?.enableLogTerminal,
      enableSDKLogs: config.output?.enableSDKLogs,
      sdkLogLevel: config.output?.sdkLogLevel,
    });

    // 3. Initialize the formatter
    initializeFormatter(output.colorEnabled);

    // 4. Initialize the log (CLI + SDK)
    initLogger({
      verbose: options.verbose,
      debug: options.debug,
      logFile: options.logFile,
      outputDir: config.output?.dir,
      logFilePattern: config.output?.logFilePattern,
      enableLogTerminal: config.output?.enableLogTerminal,
      enableSDKLogs: config.output?.enableSDKLogs,
      sdkLogLevel: config.output?.sdkLogLevel,
    });

    // 5. Initialize SDK logger (this also sets all loggers level)
    initSDKLogger({
      verbose: options.verbose,
      debug: options.debug,
      logFile: options.logFile,
      outputDir: config.output?.dir,
      logFilePattern: config.output?.logFilePattern,
      enableLogTerminal: config.output?.enableLogTerminal,
      enableSDKLogs: config.output?.enableSDKLogs,
      sdkLogLevel: config.output?.sdkLogLevel,
    });

    // 6. Initialize storage manager
    await initializeStorageManager(config);
    const storageManager = getStorageManager();

    // 7. Initialize the SDK with storage adapters and lifecycle hooks
    sdkInstance = createSDK({
      debug: options.debug,
      logging: {
        level: options.debug ? "debug" : options.verbose ? "info" : "warn",
      },
      presets: config.presets,
      checkpointStorageAdapter: storageManager?.getCheckpointStorage() ?? undefined,
      workflowStorageAdapter: storageManager?.getWorkflowStorage() ?? undefined,
      taskStorageAdapter: storageManager?.getTaskStorage() ?? undefined,
      workflowExecutionStorageAdapter: storageManager?.getWorkflowExecutionStorage() ?? undefined,
      // Enable graceful shutdown (default is true, but explicit for clarity)
      gracefulShutdown: {
        enabled: true,
        timeoutMs: 15000,
      },
      hooks: {
        onBootstrapStart: () => {
          output.infoLog("Initializing SDK...");
        },
        onBootstrapComplete: () => {
          output.infoLog("SDK initialized successfully");
        },
        onBootstrapError: error => {
          output.errorLog(`SDK initialization failed: ${error.message}`);
        },
        onDestroy: async () => {
          output.infoLog("Shutting down SDK and storage...");
          await closeStorageManager();
        },
      },
    });

    // Wait for SDK bootstrap to complete
    try {
      await sdkInstance.waitForReady();
    } catch (error) {
      output.errorLog(
        `SDK initialization failed: ${error instanceof Error ? error.message : String(error)}`,
      );
      await ExitManager.exit(1);
    }

    // 8. Initialize User Interaction Handler for interactive tools
    const interactionHandler = new CLIUserInteractionManager();
    // The eventAPI has access to the event manager internally
    // We'll use a different approach: pass the SDK instance and let handler access events
    interactionHandler.initialize(sdkInstance);
    
    // Store handler reference for cleanup
    (global as any).__cliInteractionHandler = interactionHandler;

    // 9. Initialize dependency container
    initializeContainer(sdkInstance);
  });

// Add workflow command groups
program.addCommand(createWorkflowCommands());

// Add workflow execution command group
program.addCommand(createWorkflowExecutionCommands());

// Add a checkpoint command group
program.addCommand(createCheckpointCommands());

// Add template command group
program.addCommand(createTemplateCommands());

// Add the LLM Profile command group
program.addCommand(createLLMProfileCommands());

// Add a script command group
program.addCommand(createScriptCommands());

// Add tool command groups
program.addCommand(createToolCommands());

// Add trigger command group
program.addCommand(createTriggerCommands());

// Add message command group
program.addCommand(createMessageCommands());

// Add variable command group
program.addCommand(createVariableCommands());

// Add event command group
program.addCommand(createEventCommands());

// Add the Human Relay command group
program.addCommand(createHumanRelayCommands());

// Add the Agent Loop command group
program.addCommand(createAgentCommands());

// Add the Skill command group
program.addCommand(createSkillCommands());

// Add the Agent Profile command group
program.addCommand(createAgentProfileCommands());

// Add the Metrics command group
program.addCommand(createMetricsCommands());

// Global Error Handling
program.hook("postAction", async thisCommand => {
  const options = thisCommand.opts() as { verbose?: boolean; debug?: boolean };
  const output = getOutput();

  if (options.verbose || options.debug) {
    output.infoLog("Detailed mode is enabled.");
  }

  // In headless mode, safely exit after the command execution is complete.
  if (isHeadlessMode()) {
    await ExitManager.exit(0);
  }
});

// Parse command-line arguments
program.parse(process.argv);

// Check if TUI mode is requested
const hasTuiFlag = process.argv.includes("--tui") || process.argv.includes("-t");
const executionMode = detectExecutionMode();

// Display help information if no command is provided.
if (!process.argv.slice(2).length) {
  // If TUI mode is requested or in interactive mode, start TUI
  if (hasTuiFlag || executionMode === "interactive") {
    startTUI();
  } else {
    program.outputHelp();
    // In headless mode, exit immediately if no commands are executed.
    if (isHeadlessMode()) {
      setTimeout(() => {
        ExitManager.exit(0);
      }, 100);
    }
  }
}

/**
 * Start the TUI application
 */
async function startTUI() {
  try {
    const output = getOutput();
    output.infoLog("Starting TUI mode...");
    
    // Dynamically import TUI module
    const { CLIAppTUI } = await import("./tui/index.js");
    const app = new CLIAppTUI();
    
    // Register TUI Human Relay Handler with SDK
    const humanRelayHandler = app.getHumanRelayHandler();
    if (sdkInstance) {
      sdkInstance.humanRelay.registerHandler(humanRelayHandler);
    }
    
    // Setup cleanup handlers for TUI mode
    // Note: GracefulShutdownManager is already registered by SDK, but we need additional TUI-specific cleanup
    const cleanupAndExit = async () => {
      output.infoLog("Cleaning up resources...");
      
      try {
        // Close file IO services
        await app.getHumanRelayService().dispose();
        await app.getDisplayOutputService().dispose();
        
        // Destroy SDK (triggers onDestroy hook which closes storage manager)
        if (sdkInstance) {
          await sdkInstance.destroy();
          sdkInstance = null;
        }

        // Use container to cleanup services
        try {
          const container = getContainer();
          await container.cleanup();
        } catch (error) {
          output.warnLog(`Container cleanup warning: ${error instanceof Error ? error.message : String(error)}`);
        }

        output.infoLog("Resource cleanup is complete.");

        // Close the output stream.
        await output.close();
        process.exit(0);
      } catch (error) {
        output.errorLog(
          `Error cleaning up resources: ${error instanceof Error ? error.message : String(error)}`,
        );
        await output.close();
        process.exit(1);
      }
    };

    // Register cleanup handler for process exit events
    process.on('exit', () => {
      // Synchronous cleanup only (async operations won't complete)
      output.infoLog("Process exiting...");
    });
    
    process.on('SIGINT', async () => {
      output.infoLog("Received SIGINT, cleaning up...");
      await cleanupAndExit();
    });
    
    process.on('SIGTERM', async () => {
      output.infoLog("Received SIGTERM, cleaning up...");
      await cleanupAndExit();
    });
    
    // Start the TUI application
    app.start();
  } catch (error) {
    const output = getOutput();
    output.errorLog(
      `Failed to start TUI: ${error instanceof Error ? error.message : String(error)}`,
    );
    await ExitManager.exit(1);
  }
}

// Cleanup function for CLI mode (called by postAction hook or ExitManager)
const shutdown = async () => {
  const output = getOutput();
  output.infoLog("Cleaning up resources...");

  try {
    // Destroy SDK (triggers onDestroy hook which closes storage manager)
    if (sdkInstance) {
      await sdkInstance.destroy();
      sdkInstance = null;
    }

    // Use container to cleanup services
    try {
      const container = getContainer();
      await container.cleanup();
    } catch (error) {
      output.warnLog(`Container cleanup warning: ${error instanceof Error ? error.message : String(error)}`);
    }

    output.infoLog("Resource cleanup is complete.");

    // Close the output stream.
    await output.close();
  } catch (error) {
    output.errorLog(
      `Error cleaning up resources: ${error instanceof Error ? error.message : String(error)}`,
    );
    await output.close();
    throw error;
  }
};

// Export shutdown function for use in ExitManager or other modules
export { shutdown };

// Export function to get SDK instance (for adapters and other modules)
export function getSDKInstance(): import("@wf-agent/sdk").SDKInstance | null {
  return sdkInstance;
}
