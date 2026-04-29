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
import { getSDK } from "@wf-agent/sdk";
import { ExitManager, isHeadlessMode } from "./utils/exit-manager.js";
import {
  initializeStorageManager,
  closeStorageManager,
  getStorageManager,
} from "./storage/index.js";
import { createWorkflowCommands } from "./commands/workflow/index.js";
import { createThreadCommands } from "./commands/thread/index.js";
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
import { CLIHumanRelayHandler } from "./handlers/cli-human-relay-handler.js";
import { setAllLoggersLevel } from "@wf-agent/common-utils";

// Create an instance of the main program.
const program = new Command();

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
      const { TomlParserManager } = await import("@wf-agent/sdk");
      await TomlParserManager.initialize();
    } catch (error) {
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

    // 7. Initialize the SDK with storage callbacks
    const sdk = getSDK({
      debug: options.debug,
      logLevel: options.debug ? "debug" : options.verbose ? "info" : "warn",
      presets: config.presets,
      checkpointStorageCallback: storageManager?.getCheckpointStorage() ?? undefined,
      workflowStorageCallback: storageManager?.getWorkflowStorage() ?? undefined,
      taskStorageCallback: storageManager?.getTaskStorage() ?? undefined,
    });

    // Wait for SDK bootstrap to complete
    try {
      await sdk.healthCheck();
    } catch (error) {
      // If health check fails, log but continue - some modules may not be ready yet
      if (options.debug) {
        output.debugLog(
          `SDK health check warning: ${error instanceof Error ? error.message : String(error)}`,
        );
      }
    }

    // 8. Register Human Relay Handler
    const humanRelayHandler = new CLIHumanRelayHandler();
    sdk.humanRelay.registerHandler(humanRelayHandler);
  });

// Add workflow command groups
program.addCommand(createWorkflowCommands());

// Add thread command group
program.addCommand(createThreadCommands());

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

// Display help information if no command is provided.
if (!process.argv.slice(2).length) {
  program.outputHelp();
  // In headless mode, exit immediately if no commands are executed.
  if (isHeadlessMode()) {
    setTimeout(() => {
      ExitManager.exit(0);
    }, 100);
  }
}

// Graceful exit handling
const shutdown = async () => {
  const output = getOutput();
  output.infoLog("Cleaning up resources...");

  try {
    // Close storage manager
    await closeStorageManager();

    // Dynamically import terminal modules (to avoid circular dependencies)
    const { TerminalManager } = await import("./terminal/terminal-manager.js");
    const { CommunicationBridge } = await import("./terminal/communication-bridge.js");

    const terminalManager = new TerminalManager();
    const communicationBridge = new CommunicationBridge();

    // Clean up all terminal sessions.
    await terminalManager.cleanupAll();

    // Clean up all communication bridges.
    communicationBridge.cleanupAll();

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

// Listen for the exit signal.
process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
