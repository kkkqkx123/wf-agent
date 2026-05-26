/**
 * Script Interaction Coordinator
 * Coordinates interactive script execution with user/LLM input handling
 *
 * Handles three interaction modes:
 * - blocking: Use provided inputProvider callback for user input
 * - llm-assisted: LLM provides automatic responses
 * - hybrid: LLM suggests, then inputProvider confirms or modifies
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { InteractionMode, InteractiveScriptNodeConfig, LLMMessage } from "@wf-agent/types";
import * as Identifiers from "../../../core/di/service-identifiers.js";
import type { ScriptRegistry } from "../../../core/registry/script-registry.js";
import type { GlobalContext } from "../../../core/global-context.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { getTerminalService } from "../../../services/terminal/index.js";

const logger = createContextualLogger({ component: "ScriptInteractionCoordinator" });

/**
 * Result of an interactive script execution round
 */
interface InteractionRound {
  /** Input sent to the script */
  input: string;
  /** Output received after sending input */
  output: string;
  /** Round number (1-based) */
  round: number;
}

/**
 * Result of interactive script execution
 */
export interface InteractiveExecutionResult {
  /** Overall success */
  success: boolean;
  /** Final accumulated output */
  output: string;
  /** Interaction rounds performed */
  rounds: InteractionRound[];
  /** Total execution time (ms) */
  executionTime: number;
  /** Error message if failed */
  error?: string;
}

/**
 * Input provider callback type
 * Applications can inject this to handle user input in blocking/hybrid modes
 */
export type InputProvider = (prompt: string, defaultValue?: string) => Promise<string>;

/**
 * Script Interaction Coordinator
 * Handles interactive script execution across blocking, llm-assisted, and hybrid modes
 */
export class ScriptInteractionCoordinator {
  private globalContext: GlobalContext;
  private terminalService = getTerminalService();
  private inputProvider?: InputProvider;

  constructor(
    globalContext: GlobalContext,
    _workflowExecutionEntity: WorkflowExecutionEntity,
    inputProvider?: InputProvider,
  ) {
    this.globalContext = globalContext;
    this.inputProvider = inputProvider;
  }

  /**
   * Execute a script with interaction support
   * @param scriptName Script name to execute
   * @param config Interactive script node configuration (optional)
   * @returns Interactive execution result
   */
  async executeWithInteraction(
    scriptName: string,
    config?: InteractiveScriptNodeConfig,
  ): Promise<InteractiveExecutionResult> {
    const scriptService = this.globalContext.container.get(
      Identifiers.ScriptRegistry,
    ) as ScriptRegistry;

    const interactionMode = config?.interactionMode ?? "blocking";
    const maxRounds = config?.maxRounds ?? 10;
    const roundTimeout = config?.roundTimeout ?? 60000;
    const promptPatterns = config?.promptPatterns ?? ["[?>:]\\s*$"];

    logger.debug("Starting interactive script execution", {
      scriptName,
      mode: interactionMode,
      maxRounds,
    });

    const startTime = Date.now();
    const rounds: InteractionRound[] = [];

    const executeResult = await scriptService.execute(scriptName);

    if (executeResult.isErr()) {
      return {
        success: false,
        output: "",
        rounds: [],
        executionTime: Date.now() - startTime,
        error: String(executeResult.error),
      };
    }

    const rawResult = executeResult.value;
    let accumulatedOutput = rawResult.stdout || rawResult.stderr || "";

    if (!this.needsInput(accumulatedOutput, promptPatterns)) {
      return {
        success: rawResult.success,
        output: accumulatedOutput,
        rounds,
        executionTime: Date.now() - startTime,
      };
    }

    for (let i = 0; i < maxRounds; i++) {
      let input: string;

      try {
        input = await this.getResponse(accumulatedOutput, interactionMode, roundTimeout);
      } catch (error) {
        return {
          success: false,
          output: accumulatedOutput,
          rounds,
          executionTime: Date.now() - startTime,
          error: error instanceof Error ? error.message : String(error),
        };
      }

      if (interactionMode === "blocking") {
        const inputResult = await this.terminalService.executeWithInput("", input, {
          timeout: roundTimeout,
        });
        accumulatedOutput += `\n${inputResult.stdout}${inputResult.stderr}`;
      } else {
        accumulatedOutput += `\n[input]: ${input}`;
      }

      rounds.push({ input, output: accumulatedOutput, round: i + 1 });

      if (!this.needsInput(accumulatedOutput, promptPatterns)) {
        break;
      }
    }

    return {
      success: true,
      output: accumulatedOutput,
      rounds,
      executionTime: Date.now() - startTime,
    };
  }

  /**
   * Check if output contains patterns indicating the script is waiting for input
   */
  private needsInput(output: string, patterns: string[]): boolean {
    if (!patterns || patterns.length === 0) {
      return false;
    }
    return patterns.some(pattern => {
      try {
        const regex =
          typeof pattern === "string" ? new RegExp(pattern, "m") : new RegExp(pattern, "m");
        return regex.test(output);
      } catch {
        return false;
      }
    });
  }

  /**
   * Get a response based on the interaction mode
   */
  private async getResponse(
    output: string,
    mode: InteractionMode,
    timeout: number,
  ): Promise<string> {
    switch (mode) {
      case "blocking": {
        if (this.inputProvider) {
          return this.inputProvider(`Script requires input:\n${output.slice(-500)}`);
        }
        logger.warn("No inputProvider registered for blocking mode, using empty response");
        return "";
      }
      case "llm-assisted": {
        const response = await this.getLLMResponse(output, timeout);
        return response;
      }
      case "hybrid": {
        const llmSuggestion = await this.getLLMResponse(output, Math.floor(timeout / 2));
        if (this.inputProvider) {
          const userInput = await this.inputProvider(
            `Script requires input. LLM suggestion: "${llmSuggestion}"\n\nScript output:\n${output.slice(-500)}`,
            llmSuggestion,
          );
          return userInput || llmSuggestion;
        }
        return llmSuggestion;
      }
      default:
        return "";
    }
  }

  /**
   * Get a response from the LLM assistant
   */
  private async getLLMResponse(output: string, timeout: number): Promise<string> {
    const llmExecutor = this.globalContext.llmExecutor;
    const messages: LLMMessage[] = [
      {
        role: "system",
        content:
          "You are assisting with an interactive script execution. The script output indicates it is waiting for input. Provide a single-line response that is appropriate to continue the script execution. Respond with ONLY the input value, no explanations.",
      },
      {
        role: "user",
        content: `Script output:\n${output.slice(-2000)}`,
      },
    ];

    const result = await llmExecutor.executeLLMCall(
      messages,
      {
        profileId: "default",
        prompt: "Generate script input response",
        parameters: { temperature: 0.3, maxTokens: 100 },
      },
      { abortSignal: AbortSignal.timeout(timeout) },
    );

    return result.content?.trim() ?? "";
  }
}
