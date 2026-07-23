/**
 * Agent Loop Command Group
 */

import { Command } from "commander";
import { AgentLoopAdapter } from "../../adapters/agent-loop-adapter.js";
import { AgentLoopCheckpointAdapter } from "../../adapters/agent-loop-checkpoint-adapter.js";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { getFormatter } from "../../utils/formatter.js";
import { formatAgentLoop, formatAgentLoopList } from "../../utils/formatters/index.js";
import type { CommandOptions } from "../../types/cli-types.js";
import type { AgentLoopRuntimeConfig, AgentLoopCheckpoint, Message, LLMMessage } from "@wf-agent/types";
import { handleError } from "../../utils/error-handler.js";
import { CLIValidationError } from "../../types/cli-types.js";
import { loadAgentLoopConfig } from "@wf-agent/config-processor";
import { transformToAgentLoopConfig } from "@wf-agent/sdk/api";
import { existsSync } from "fs";
import { createIterationCommands } from "../agent-loop-iteration/index.js";
import { createAgentErrorCommands } from "./error.js";
import { createAgentPerfCommands } from "./perf.js";
import { createAgentTemplateCommands } from "./template.js";
import { createAgentTriggerTemplateCommands } from "./trigger-template.js";
import { createAgentHookTemplateCommands } from "./hook-template.js";

const output = getOutput();
const router = getRouter();

/**
 * Create Agent Loop Command Group
 */
export function createAgentCommands(): Command {
  const agentCmd = new Command("agent").description("Managing the Agent Loop");

  // Execute the Agent Loop command
  agentCmd
    .command("run")
    .description("Execute Agent Loop")
    .option("-c, --config <file>", "Configuration file path (TOML or JSON)")
    .option("-p, --profile <profileId>", "LLM Profile ID")
    .option("-s, --system-prompt <prompt>", "System prompt")
    .option("-m, --max-iterations <number>", "Maximum number of iterations", "10")
    .option("-t, --tools <tools>", "List of tools (comma separated)")
    .option("-i, --input <json>", "Initial input data (JSON format)")
    .option("--stream", "Streaming implementation")
    .option("-v, --verbose", "Detailed output")
    .action(
      async (
        options: CommandOptions & {
          config?: string;
          profile?: string;
          systemPrompt?: string;
          maxIterations?: string;
          tools?: string;
          input?: string;
          stream?: boolean;
        },
      ) => {
        try {
          output.infoLog("Executing Agent Loop...");

          let config: AgentLoopRuntimeConfig;

          // Load from configuration file or use command line arguments
          if (options.config) {
            if (!existsSync(options.config)) {
              handleError(new CLIValidationError(`Config file not found: ${options.config}`), {
                operation: "runAgentLoop",
                additionalInfo: { config: options.config },
              });
              return;
            }

            output.infoLog(`Loading config from: ${options.config}`);
            const parsedConfig = await loadAgentLoopConfig(options.config);
            config = transformToAgentLoopConfig(parsedConfig.config);
          } else {
            // Building a Configuration with Command Line Parameters
            config = {
              profileId: options.profile || "DEFAULT",
              systemPrompt: options.systemPrompt,
              maxIterations: parseInt(options.maxIterations || "10", 10),
              availableTools: options.tools ? { tools: options.tools.split(",").map(t => t.trim()) } : undefined,
            };
          }

          // Command line arguments override configuration files
          if (options.profile) config.profileId = options.profile;
          if (options.systemPrompt) config.systemPrompt = options.systemPrompt;
          if (options.maxIterations) config.maxIterations = parseInt(options.maxIterations, 10);
          if (options.tools) config.availableTools = { tools: options.tools.split(",").map(t => t.trim()) };
          if (options.stream !== undefined) config.stream = options.stream;

          // Parse initial input
          let initialMessages: LLMMessage[] = [];
          if (options.input) {
            try {
              const inputData = JSON.parse(options.input);
              if (Array.isArray(inputData)) {
                initialMessages = inputData;
              } else if (inputData.message) {
                initialMessages = [{ role: "user", content: inputData.message }];
              }
            } catch (_error) {
              handleError(new CLIValidationError("Input data must be in a valid JSON format"), {
                operation: "runAgentLoop",
                additionalInfo: { input: options.input },
              });
              return;
            }
          }

          const adapter = new AgentLoopAdapter();

          // Apply dynamic context injection
          // All dynamic context is disabled by default; only agent config or explicit CLI
          // options can enable specific features
          await adapter.applyDynamicContextToConfig(config);

          if (config.stream || options.stream) {
            // Stream execution
            const result = await adapter.executeAgentLoopStream(
              config,
              { initialMessages },
              // eslint-disable-next-line @typescript-eslint/no-explicit-any
              (event: any) => {
                // Print stream events
                if (event.type === "message_update" || event.type === "text") {
                  if (event.delta) {
                    output.stream(event.delta);
                  }
                } else if (event.type === "tool_call_start" || event.type === "tool_execution_start") {
                  output.newLine();
                  output.output(
                    `Calling tool: ${event.data?.toolCall?.function?.name || "unknown"}`,
                  );
                } else if (event.type === "tool_call_end" || event.type === "tool_execution_end") {
                  if (event.data?.success) {
                    output.output(`Tool call completed: ${event.data?.toolCallId}`);
                  } else {
                    output.output(
                      `Tool call failed: ${event.data?.toolCallId}, ${event.data?.error}`,
                    );
                  }
                } else if (event.type === "iteration_complete") {
                  output.newLine();
                  output.output(`Iteration complete: ${event.data?.iteration}`);
                }
              },
            );

            output.newLine();
            router.render(result, {
              type: "detail",
              entity: "agent-loop",
              format: () => formatAgentLoop(result, { verbose: options.verbose }),
            });
          } else {
            // Sync execution
            const result = await adapter.executeAgentLoop(config, { initialMessages });
            router.render(result, {
              type: "detail",
              entity: "agent-loop",
              format: () => formatAgentLoop(result, { verbose: options.verbose }),
            });
          }
        } catch (error) {
          handleError(error, {
            operation: "runAgentLoop",
            additionalInfo: { profileId: options.profile, maxIterations: options.maxIterations },
          });
        }
      },
    );

  // Asynchronous Start Agent Loop Command
  agentCmd
    .command("start")
    .description("Start Agent Loop asynchronously")
    .option("-p, --profile <profileId>", "LLM Profile ID")
    .option("-s, --system-prompt <prompt>", "System prompt")
    .option("-m, --max-iterations <number>", "Maximum number of iterations", "10")
    .option("-t, --tools <tools>", "List of tools (comma separated)")
    .option("-i, --input <json>", "Initial input data (JSON format)")
    .action(
      async (
        options: CommandOptions & {
          profile?: string;
          systemPrompt?: string;
          maxIterations?: string;
          tools?: string;
          input?: string;
        },
      ) => {
        try {
          output.infoLog("Starting Agent Loop...");

          const config: AgentLoopRuntimeConfig = {
            profileId: options.profile || "DEFAULT",
            systemPrompt: options.systemPrompt,
            maxIterations: parseInt(options.maxIterations || "10", 10),
            availableTools: options.tools ? { tools: options.tools.split(",").map(t => t.trim()) } : undefined,
          };

          let initialMessages: LLMMessage[] = [];
          if (options.input) {
            try {
              const inputData = JSON.parse(options.input);
              if (Array.isArray(inputData)) {
                initialMessages = inputData;
              } else if (inputData.message) {
                initialMessages = [{ role: "user", content: inputData.message }];
              }
            } catch (_error) {
              handleError(new CLIValidationError("Input data must be in a valid JSON format"), {
                operation: "startAgentLoop",
                additionalInfo: { input: options.input },
              });
              return;
            }
          }

          const adapter = new AgentLoopAdapter();
          const id = await adapter.startAgentLoop(config, { initialMessages });

          output.newLine();
          output.info("Agent Loop started.");
          output.output(getFormatter().keyValue("  ID", id));
          output.newLine();
          output.info(`Agent Loop ID: ${id}`);
        } catch (error) {
          handleError(error, {
            operation: "startAgentLoop",
            additionalInfo: { profileId: options.profile, maxIterations: options.maxIterations },
          });
        }
      },
    );

  // The Pause Agent Loop command
  agentCmd
    .command("pause <id>")
    .description("Pause Agent Loop")
    .action(async id => {
      try {
        output.infoLog(`Pausing Agent Loop: ${id}`);

        const adapter = new AgentLoopAdapter();
        await adapter.pauseAgentLoop(id);
      } catch (error) {
        handleError(error, {
          operation: "pauseAgentLoop",
          additionalInfo: { id },
        });
      }
    });

  // Restoring the Agent Loop Command
  agentCmd
    .command("resume <id>")
    .description("Resume Agent Loop")
    .action(async id => {
      try {
        output.infoLog(`Resuming Agent Loop: ${id}`);

        const adapter = new AgentLoopAdapter();
        const result = await adapter.resumeAgentLoop(id);

        router.render(result, {
          type: "detail",
          entity: "agent-loop",
          format: () => formatAgentLoop(result),
        });
      } catch (error) {
        handleError(error, {
          operation: "resumeAgentLoop",
          additionalInfo: { id },
        });
      }
    });

  // Stop Agent Loop command
  agentCmd
    .command("stop <id>")
    .description("Stop Agent Loop")
    .action(async id => {
      try {
        output.infoLog(`Stopping Agent Loop: ${id}`);

        const adapter = new AgentLoopAdapter();
        await adapter.stopAgentLoop(id);
      } catch (error) {
        handleError(error, {
          operation: "stopAgentLoop",
          additionalInfo: { id },
        });
      }
    });

  // View Status Command
  agentCmd
    .command("status <id>")
    .description("Check Agent Loop status")
    .action(async id => {
      try {
        const adapter = new AgentLoopAdapter();
        const status = await adapter.getAgentLoopStatus(id);

        if (!status) {
          handleError(new CLIValidationError(`Agent Loop not found: ${id}`), {
            operation: "getAgentLoopStatus",
            additionalInfo: { id },
          });
          return;
        }

        output.newLine();
        output.output(getFormatter().subsection("Agent Loop Status."));
        output.output(getFormatter().keyValue("  ID", id));
        output.output(getFormatter().keyValue("  Status", status));
      } catch (error) {
        handleError(error, {
          operation: "getAgentLoopStatus",
          additionalInfo: { id },
        });
      }
    });

  // View Details command
  agentCmd
    .command("show <id>")
    .description("View Agent Loop details")
    .option("-v, --verbose", "Detailed output")
    .action(async (id, options: CommandOptions) => {
      try {
        const adapter = new AgentLoopAdapter();
        const agentLoopInfo = await adapter.getAgentLoop(id);

        if (!agentLoopInfo) {
          handleError(new CLIValidationError(`Agent Loop not found: ${id}`), {
            operation: "getAgentLoop",
            additionalInfo: { id },
          });
          return;
        }

        // Convert to AgentLoopWithMetadata format
        const agentLoopWithMetadata = {
          ...agentLoopInfo,
          success: true,
          iterations: agentLoopInfo.currentIteration,
        };
        router.render(agentLoopWithMetadata, {
          type: "detail",
          entity: "agent-loop",
          format: () => formatAgentLoop(agentLoopWithMetadata, { verbose: options.verbose }),
        });
      } catch (error) {
        handleError(error, {
          operation: "getAgentLoop",
          additionalInfo: { id },
        });
      }
    });

  // List Agent Loop Commands
  agentCmd
    .command("list")
    .description("List all Agent Loops")
    .option("--running", "Show only the running")
    .option("--paused", "Show only paused")
    .option("-t, --table", "Output in tabular format")
    .action(async (options: CommandOptions & { running?: boolean; paused?: boolean }) => {
      try {
        const adapter = new AgentLoopAdapter();

        let agentLoops: unknown[];
        if (options.running) {
          agentLoops = adapter.listRunningAgentLoops();
        } else if (options.paused) {
          agentLoops = adapter.listPausedAgentLoops();
        } else {
          agentLoops = adapter.listAgentLoops();
        }

        router.render(agentLoops as never, {
          type: "list",
          entity: "agent-loop",
          format: () => formatAgentLoopList(agentLoops as never, { table: options.table }),
          metadata: { total: (agentLoops as never[]).length },
        });
      } catch (error) {
        handleError(error, {
          operation: "listAgentLoops",
          additionalInfo: { running: options.running, paused: options.paused },
        });
      }
    });

  // Create Checkpoint Command
  agentCmd
    .command("checkpoint <id>")
    .description("Create an Agent Loop checkpoint")
    .option("-n, --name <name>", "Name of checkpoint")
    .action(async (id, options: CommandOptions & { name?: string }) => {
      try {
        output.infoLog(`Creating checkpoint for Agent Loop: ${id}`);

        const adapter = new AgentLoopAdapter();
        const checkpointAdapter = new AgentLoopCheckpointAdapter();
        
        // Get the agent loop entity from registry
        const entity = adapter.getAgentLoopEntity(id);
        if (!entity) {
          handleError(new CLIValidationError(`Agent Loop not found: ${id}`), {
            operation: "createCheckpoint",
            additionalInfo: { id },
          });
          return;
        }

        // Create checkpoint dependencies with real storage
        const dependencies = {
          saveCheckpoint: async (checkpoint: unknown) => {
            return await checkpointAdapter.saveCheckpointToStorage(checkpoint as AgentLoopCheckpoint);
          },
          getCheckpoint: async (checkpointId: string) => {
            return await checkpointAdapter.getCheckpointFromStorage(checkpointId);
          },
          listCheckpoints: async (agentLoopId: string) => {
            return await checkpointAdapter.listCheckpointIdsFromStorage(agentLoopId);
          },
        };

        const checkpointId = await adapter.createCheckpoint(id, dependencies, {
          name: options.name,
        });
        output.newLine();
        output.info(`Checkpoint created: ${checkpointId}`);
      } catch (error) {
        handleError(error, {
          operation: "createCheckpoint",
          additionalInfo: { id, name: options.name },
        });
      }
    });

  // Recover from Checkpoint command
  agentCmd
    .command("restore <checkpoint-id>")
    .description("Restore the Agent Loop from checkpoint")
    .option("-c, --config <file>", "Configuration file path (TOML or JSON) for restore")
    .action(async (checkpointId, options: { config?: string }) => {
      try {
        output.infoLog(`Restoring from checkpoint: ${checkpointId}`);

        const checkpointAdapter = new AgentLoopCheckpointAdapter();

        // Get checkpoint details first
        const checkpoint = await checkpointAdapter.getCheckpoint(checkpointId);
        if (!checkpoint) {
          handleError(new CLIValidationError(`Checkpoint not found: ${checkpointId}`), {
            operation: "restoreFromCheckpoint",
            additionalInfo: { checkpointId },
          });
          return;
        }

        // Create checkpoint dependencies with real storage
        const dependencies = {
          saveCheckpoint: async (cp: AgentLoopCheckpoint) => {
            return await checkpointAdapter.saveCheckpointToStorage(cp);
          },
          getCheckpoint: async (cpId: string) => {
            return await checkpointAdapter.getCheckpointFromStorage(cpId);
          },
          listCheckpoints: async (agentLoopId: string) => {
            return await checkpointAdapter.listCheckpointIdsFromStorage(agentLoopId);
          },
        };

        // Load restore config if provided
        let restoreConfig: AgentLoopRuntimeConfig | undefined;
        if (options.config) {
          if (!existsSync(options.config)) {
            handleError(new CLIValidationError(`Config file not found: ${options.config}`), {
              operation: "restoreFromCheckpoint",
              additionalInfo: { checkpointId, config: options.config },
            });
            return;
          }
          output.infoLog(`Loading restore config from: ${options.config}`);
          const parsedConfig = await loadAgentLoopConfig(options.config);
          restoreConfig = transformToAgentLoopConfig(parsedConfig.config);
        }

        const result = await checkpointAdapter.restoreCheckpoint(checkpointId, dependencies, restoreConfig);
        output.newLine();
        output.info(`Restored from checkpoint: ${result.id}`);
      } catch (error) {
        handleError(error, {
          operation: "restoreFromCheckpoint",
          additionalInfo: { checkpointId },
        });
      }
    });

  // Clone Agent Loop command
  agentCmd
    .command("clone <id>")
    .description("Clone Agent Loop")
    .action(async id => {
      try {
        output.infoLog(`Cloning Agent Loop: ${id}`);

        const adapter = new AgentLoopAdapter();
        const result = await adapter.cloneAgentLoop(id);

        output.newLine();
        output.info("Agent Loop has been cloned.");
        output.output(getFormatter().keyValue("New ID", result.id));
      } catch (error) {
        handleError(error, {
          operation: "cloneAgentLoop",
          additionalInfo: { id },
        });
      }
    });

  // Clean up completed instances command
  agentCmd
    .command("cleanup")
    .description("Clean up completed Agent Loops")
    .action(async () => {
      try {
        const adapter = new AgentLoopAdapter();
        const count = adapter.cleanupAgentLoops();

        output.newLine();
        output.info(`Cleaned up ${count} Agent Loops`);
      } catch (error) {
        handleError(error, {
          operation: "cleanupAgentLoops",
        });
      }
    });

  // View message history command
  agentCmd
    .command("messages <id>")
    .description("View the Agent Loop message history")
    .option("-v, --verbose", "Detailed output")
    .action(async (id, options: CommandOptions) => {
      try {
        const adapter = new AgentLoopAdapter();
        const messages = await adapter.getAgentLoopMessages(id);

        if (options.verbose) {
          output.output(getFormatter().json(messages));
        } else {
          messages.forEach((msg: Message, index: number) => {
            const role = msg.role || "unknown";
            const content = typeof msg.content === 'string' ? msg.content : JSON.stringify(msg.content);
            const preview = content.length > 100 ? content.substring(0, 100) + "..." : content;
            output.output(`${index + 1}. [${role}] ${preview}`);
          });
        }
      } catch (error) {
        handleError(error, {
          operation: "getAgentLoopMessages",
          additionalInfo: { id },
        });
      }
    });

  // Delete the Agent Loop command
  agentCmd
    .command("delete <id>")
    .description("Delete Agent Loop")
    .option("-f, --force", "Force deletion without prompting for confirmation")
    .action(async (id, options: { force?: boolean }) => {
      try {
        if (!options.force) {
          output.warnLog(`About to delete Agent Loop: ${id}`);
          output.infoLog("Use the --force option to skip the confirmation.");
          return;
        }

        const adapter = new AgentLoopAdapter();
        adapter.cleanupAgentLoop(id);
      } catch (error) {
        handleError(error, {
          operation: "deleteAgentLoop",
          additionalInfo: { id },
        });
      }
    });

  // Add iteration analysis subcommand
  agentCmd.addCommand(createIterationCommands());

  // ========================================================================
  // Agent Trigger Subcommand Group
  // ========================================================================
  const agentTriggerCmd = new Command("trigger").description("Manage agent loop triggers");

  agentTriggerCmd
    .command("list <agent-loop-id>")
    .description("List triggers for an agent loop")
    .option("-t, --table", "Output in table format")
    .option("-v, --verbose", "Detailed output")
    .action(async (agentLoopId, options: CommandOptions) => {
      try {
        const { getSDKInstance } = await import("../../services/sdk-globals.js");
        const sdk = getSDKInstance();
        if (!sdk) throw new Error("SDK not available");
        const triggers = await sdk.triggers.getAll({ agentLoopId } as any);

        router.render(triggers, {
          type: "list",
          entity: "agent-trigger",
          format: () => {
            if (triggers.length === 0) return "No triggers found for this agent loop";
            if (options.table) {
              const formatter = getFormatter();
              const headers = ["ID", "Type", "Status"];
              const rows = triggers.map((t: any) => [
                t.id || "N/A",
                t.type || "N/A",
                t.status || "N/A",
              ]);
              return formatter.table(headers, rows);
            }
            return triggers.map((t: any) => `  ${t.id} - ${t.type || "N/A"} - ${t.status || "N/A"}`).join("\n");
          },
          metadata: { total: triggers.length },
        });
      } catch (error) {
        handleError(error, { operation: "agent-trigger-list", additionalInfo: { agentLoopId } });
      }
    });

  agentTriggerCmd
    .command("enable <agent-loop-id> <trigger-id>")
    .description("Enable an agent loop trigger")
    .action(async (agentLoopId, triggerId) => {
      try {
        output.infoLog(`Enabling trigger ${triggerId} for agent loop ${agentLoopId}`);
        const { EnableAgentTriggerCommand } = await import("@wf-agent/sdk/api");
        const { getSDKInstance } = await import("../../services/sdk-globals.js");
        const sdk = getSDKInstance();
        if (!sdk) throw new Error("SDK not available");
        const deps = sdk.getFactory().getDependencies();
        const cmd = new EnableAgentTriggerCommand({ agentLoopId, triggerId }, deps);
        const result = await sdk.executeCommand(cmd);
        if (result.result.isOk()) {
          output.success(`Trigger enabled: ${triggerId}`);
        } else {
          throw new Error(result.result.error?.message || "Failed to enable trigger");
        }
      } catch (error) {
        handleError(error, { operation: "agent-trigger-enable", additionalInfo: { agentLoopId, triggerId } });
      }
    });

  agentTriggerCmd
    .command("disable <agent-loop-id> <trigger-id>")
    .description("Disable an agent loop trigger")
    .action(async (agentLoopId, triggerId) => {
      try {
        output.infoLog(`Disabling trigger ${triggerId} for agent loop ${agentLoopId}`);
        const { DisableAgentTriggerCommand } = await import("@wf-agent/sdk/api");
        const { getSDKInstance } = await import("../../services/sdk-globals.js");
        const sdk = getSDKInstance();
        if (!sdk) throw new Error("SDK not available");
        const deps = sdk.getFactory().getDependencies();
        const cmd = new DisableAgentTriggerCommand({ agentLoopId, triggerId }, deps);
        const result = await sdk.executeCommand(cmd);
        if (result.result.isOk()) {
          output.success(`Trigger disabled: ${triggerId}`);
        } else {
          throw new Error(result.result.error?.message || "Failed to disable trigger");
        }
      } catch (error) {
        handleError(error, { operation: "agent-trigger-disable", additionalInfo: { agentLoopId, triggerId } });
      }
    });

  agentCmd.addCommand(agentTriggerCmd);

  // ========================================================================
  // Agent Variable Subcommand Group
  // ========================================================================
  const agentVariableCmd = new Command("variable").description("Manage agent loop variables");

  agentVariableCmd
    .command("list <agent-loop-id>")
    .description("List variables for an agent loop")
    .option("-t, --table", "Output in table format")
    .option("-v, --verbose", "Detailed output")
    .action(async (agentLoopId, options: CommandOptions) => {
      try {
        const { getSDKInstance } = await import("../../services/sdk-globals.js");
        const sdk = getSDKInstance();
        if (!sdk) throw new Error("SDK not available");
        const api = sdk.agentVariables;
        const variables = await api.getExecutionVariables(agentLoopId);

        const entries = Object.entries(variables);
        router.render(variables, {
          type: "list",
          entity: "agent-variable",
          format: () => {
            if (entries.length === 0) return "No variables found for this agent loop";
            if (options.table) {
              const formatter = getFormatter();
              const headers = ["Name", "Value"];
              const rows = entries.map(([name, value]) => [
                name,
                value !== undefined ? String(value).substring(0, 40) : "N/A",
              ]);
              return formatter.table(headers, rows);
            }
            return entries
              .map(([name, value]) => `  ${name} = ${value !== undefined ? JSON.stringify(value) : "N/A"}`)
              .join("\n");
          },
          metadata: { total: entries.length },
        });
      } catch (error) {
        handleError(error, { operation: "agent-variable-list", additionalInfo: { agentLoopId } });
      }
    });

  agentVariableCmd
    .command("show <agent-loop-id> <variable-name>")
    .description("Show an agent loop variable value")
    .action(async (agentLoopId, variableName) => {
      try {
        const { getSDKInstance } = await import("../../services/sdk-globals.js");
        const sdk = getSDKInstance();
        if (!sdk) throw new Error("SDK not available");
        const api = sdk.agentVariables;
        const value = await api.getExecutionVariable(agentLoopId, variableName);

        if (value === undefined || value === null) {
          output.info(`Variable '${variableName}' not found`);
          return;
        }

        output.newLine();
        output.output(getFormatter().subsection(`Variable: ${variableName}`));
        output.output(getFormatter().keyValue("Value", JSON.stringify(value)));
      } catch (error) {
        handleError(error, { operation: "agent-variable-show", additionalInfo: { agentLoopId, variableName } });
      }
    });

  agentVariableCmd
    .command("set <agent-loop-id> <variable-name> <value>")
    .description("Set an agent loop variable value")
    .option("-j, --json", "The value is in JSON format")
    .action(async (agentLoopId, variableName, value, options: { json?: boolean }) => {
      try {
        const { getSDKInstance } = await import("../../services/sdk-globals.js");
        const sdk = getSDKInstance();
        if (!sdk) throw new Error("SDK not available");

        const parsedValue = options.json ? JSON.parse(value) : value;
        const result = await sdk.agentVariables.setExecutionVariable(agentLoopId, variableName, parsedValue);

        if (result.result.isOk()) {
          output.success(`Variable '${variableName}' set on agent loop ${agentLoopId}`);
        } else {
          throw new Error(result.result.error?.message || "Failed to set variable");
        }
      } catch (error) {
        handleError(error, { operation: "agent-variable-set", additionalInfo: { agentLoopId, variableName } });
      }
    });

  agentVariableCmd
    .command("delete <agent-loop-id> <variable-name>")
    .description("Delete an agent loop variable")
    .option("-f, --force", "Skip confirmation")
    .action(async (agentLoopId, variableName, options: { force?: boolean }) => {
      try {
        if (!options.force) {
          output.warnLog(`About to delete variable: ${variableName}`);
          output.infoLog("Use the --force option to skip confirmation.");
          return;
        }

        const { getSDKInstance } = await import("../../services/sdk-globals.js");
        const sdk = getSDKInstance();
        if (!sdk) throw new Error("SDK not available");

        const result = await sdk.agentVariables.deleteExecutionVariable(agentLoopId, variableName);

        if (result.result.isOk()) {
          output.success(`Variable '${variableName}' deleted from agent loop ${agentLoopId}`);
        } else {
          throw new Error(result.result.error?.message || "Failed to delete variable");
        }
      } catch (error) {
        handleError(error, { operation: "agent-variable-delete", additionalInfo: { agentLoopId, variableName } });
      }
    });

  agentCmd.addCommand(agentVariableCmd);

  // ========================================================================
  // Agent Error Analysis Subcommand Group
  // ========================================================================
  agentCmd.addCommand(createAgentErrorCommands());

  // ========================================================================
  // Agent Performance Analysis Subcommand Group
  // ========================================================================
  agentCmd.addCommand(createAgentPerfCommands());

  // ========================================================================
  // Agent Template Registry Subcommand Groups
  // ========================================================================
  agentCmd.addCommand(createAgentTemplateCommands());
  agentCmd.addCommand(createAgentTriggerTemplateCommands());
  agentCmd.addCommand(createAgentHookTemplateCommands());

  return agentCmd;
}
