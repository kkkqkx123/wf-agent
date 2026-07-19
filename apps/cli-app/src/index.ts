#!/usr/bin/env node

/**
 * Modular Agent CLI Application
 * Modular Agent Framework command-line application
 */

import { Command } from "commander";
import { getOutput } from "./utils/output.js";
import { initializeFormatter } from "./utils/formatter.js";
import { initLogger, initSDKLogger } from "./utils/logger.js";
import { loadConfigWithEnvOverride } from "./config/index.js";
import { createAppSDK } from "@wf-agent/runtime/bootstrap";
import { ExitManager } from "./utils/exit-manager.js";
import { isHeadless, getMode, getOutputFormat } from "./utils/mode-detector.js";
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
import { createAgentCommands } from "./commands/agent/index.js";
import { createSkillCommands } from "./commands/skill/index.js";
import { createAgentProfileCommands } from "./commands/agent-profile/index.js";
import { createMetricsCommands } from "./commands/metrics/index.js";
import { createStorageCommands } from "./commands/storage/index.js";
import { createSearchCommand } from "./commands/search/index.js";
import { createWorkflowGraphCommands } from "./commands/workflow-graph/index.js";
import { createExecutionComparisonCommand } from "./commands/execution-comparison/index.js";
import { createProgressCommand } from "./commands/progress/index.js";
import { createWorkflowVersionCommand } from "./commands/workflow-version/index.js";
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
  .option("-t, --tui", "Start the TUI (interactive terminal UI) mode.")
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

    // 2. Initialize/reconfigure the output system in-place
    //    (command files captured getOutput() at module load; reconfigure
    //     ensures their references stay valid with proper settings)
    const output = getOutput();
    output.reconfigure({
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

    // 5. Initialize SDK and storage via createAppSDK
    const { sdk: sdkInstance } = await createAppSDK({
      appName: "cli-app",
      debug: options.debug,
      verbose: options.verbose,
      logging: {
        level: config.logLevel ||
               (options.debug ? "debug" : options.verbose ? "info" : "warn"),
      },
      storage: {
        storage: config.storage,
        appName: "cli-app",
      },
      presets: config.presets,
      strictStorage: true,
      defaultTimeout: config.defaultTimeout,
      maxConcurrentExecutions: config.maxConcurrentExecutions,
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
      },
    });

    // 6. Initialize User Interaction Handler for interactive tools
    const interactionHandler = new CLIUserInteractionManager();
    interactionHandler.initialize(sdkInstance);

    // 7. Initialize container with SDK and config (includes StorageManager)
    const container = initializeContainer(sdkInstance, config);
    container.registerInteractionHandler(interactionHandler);
  });

// Ensure headless-mode exit after command completion
// MUST be before any command registration so .action() is wrapped for all commands
const originalAction = Command.prototype.action;
Command.prototype.action = function (fn: (...args: unknown[]) => unknown) {
  const wrappedFn = async (...args: unknown[]) => {
    try {
      await fn(...args);
    } finally {
      // Run cleanup (storage close, SDK destroy) before exit.
      // This ensures SQLite connections are properly closed and WAL
      // is checkpointed, preventing data loss or corruption.
      try {
        await shutdown();
      } catch {
        // Ignore shutdown errors during process exit
      }
      process.exit(0);
    }
  };
  return originalAction.call(this, wrappedFn);
};

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

// Add the Agent Loop command group
program.addCommand(createAgentCommands());

// Add the Skill command group
program.addCommand(createSkillCommands());

// Add the Agent Profile command group
program.addCommand(createAgentProfileCommands());

// Add the Metrics command group
program.addCommand(createMetricsCommands());

// Add storage command group
program.addCommand(createStorageCommands());

// Add search command
program.addCommand(createSearchCommand());

// Add workflow graph command as subcommand under workflow
const workflowCmd = program.commands.find(c => c.name() === "workflow");
if (workflowCmd) {
  workflowCmd.addCommand(createWorkflowGraphCommands());
  workflowCmd.addCommand(createWorkflowVersionCommand());
}

// Add execution comparison command as subcommand under workflow-execution
const executionCmd = program.commands.find(c => c.name() === "workflow-execution");
if (executionCmd) {
  executionCmd.addCommand(createExecutionComparisonCommand());
  executionCmd.addCommand(createProgressCommand());
}

// Parse command-line arguments
program.parse(process.argv);

// Check if TUI mode is requested (using commander's parsed options)
const cliOpts = program.opts();
const hasTuiFlag = cliOpts["tui"] ?? false;
const executionMode = getMode().mode;
const outputFormat = getOutputFormat();

// Determine if TUI mode should be started
const shouldStartTUI =
  (hasTuiFlag || executionMode === "interactive") && !process.argv.slice(2).length;

// Display help information if no command is provided.
if (!process.argv.slice(2).length) {
  if (shouldStartTUI) {
    // TUI mode requires a TTY and text output format
    if (outputFormat !== "text") {
      getOutput().warn(
        `TUI mode requires text output format (current: ${outputFormat}). Falling back to help.`,
      );
      program.outputHelp();
    } else if (!process.stdout.isTTY) {
      getOutput().warn("TUI mode requires a TTY terminal. Falling back to help.");
      program.outputHelp();
    } else {
      startTUI();
    }
  } else {
    program.outputHelp();
    // In headless mode, exit immediately if no commands are executed.
    if (isHeadless()) {
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

    // Setup cleanup handlers for TUI mode
    // Note: GracefulShutdownManager is already registered by SDK, but we need additional TUI-specific cleanup
    const cleanupAndExit = async () => {
      output.infoLog("Cleaning up resources...");

      try {
        // Destroy SDK (triggers onDestroy hook which closes storage manager)
        if (sdkInstance) {
          await sdkInstance.destroy();
          sdkInstance = null;
        }

        // Use container to cleanup services (including StorageManager)
        try {
          const container = getContainer();
          await container.cleanup();
        } catch (error) {
          output.warnLog(
            `Container cleanup warning: ${error instanceof Error ? error.message : String(error)}`,
          );
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
    process.on("exit", () => {
      // Synchronous cleanup only (async operations won't complete)
      output.infoLog("Process exiting...");
    });

    process.on("SIGINT", async () => {
      output.infoLog("Received SIGINT, cleaning up...");
      await cleanupAndExit();
    });

    process.on("SIGTERM", async () => {
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

    // Use container to cleanup services (including StorageManager)
    try {
      const container = getContainer();
      await container.cleanup();
    } catch (error) {
      output.warnLog(
        `Container cleanup warning: ${error instanceof Error ? error.message : String(error)}`,
      );
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
