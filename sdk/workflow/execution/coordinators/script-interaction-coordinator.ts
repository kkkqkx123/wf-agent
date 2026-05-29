/**
 * Script Interaction Coordinator
 * Coordinates interactive script execution with user/LLM input handling
 *
 * Uses a persistent PTY session for true interactive I/O:
 * 1. Creates a terminal session and starts the script in background
 * 2. Polls for output, detecting prompt patterns
 * 3. Sends input via session stdin when the script waits for input
 * 4. Repeats until the script finishes or maxRounds is reached
 *
 * Handles three interaction modes:
 * - blocking: Use provided inputProvider callback for user input
 * - llm-assisted: LLM provides automatic responses
 * - hybrid: LLM suggests, then inputProvider confirms or modifies
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { InteractionMode, InteractiveScriptNodeConfig, LLMMessage, Script } from "@wf-agent/types";
import * as Identifiers from "../../../core/di/service-identifiers.js";
import type { ScriptRegistry } from "../../../core/registry/script-registry.js";
import type { GlobalContext } from "../../../core/global-context.js";
import { ScriptTemplateEngine } from "../../../core/script/engine/script-template.js";
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
  /** Node execution status (mirrors NodeExecutionResult.status) */
  status: "COMPLETED" | "FAILED";
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
 * Uses a persistent PTY session for true interactive I/O instead of one-off command execution.
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
   * Execute a script with interaction support using a persistent PTY session
   * @param scriptName Script name to execute
   * @param config Interactive script node configuration (optional)
   * @returns Interactive execution result
   */
  async executeWithInteraction(
    scriptName: string,
    config?: InteractiveScriptNodeConfig,
    abortSignal?: AbortSignal,
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

    if (abortSignal?.aborted) {
      return {
        success: false,
        status: "FAILED",
        output: "",
        rounds: [],
        executionTime: 0,
        error: "Execution cancelled",
      };
    }
    const rounds: InteractionRound[] = [];

    // Step 1: Get script definition and render command
    let script: Script;
    try {
      script = scriptService.getScript(scriptName);
    } catch {
      return {
        success: false,
        status: "FAILED",
        output: "",
        rounds: [],
        executionTime: Date.now() - startTime,
        error: `Script not found: ${scriptName}`,
      };
    }

    let command: string;
    try {
      command = this.renderCommand(script);
    } catch (error) {
      return {
        success: false,
        status: "FAILED",
        output: "",
        rounds: [],
        executionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
      };
    }

    if (!command) {
      return {
        success: false,
        status: "FAILED",
        output: "",
        rounds: [],
        executionTime: Date.now() - startTime,
        error: "Empty script command",
      };
    }

    // Step 2: Create PTY session and start command in background
    let session;
    try {
      session = await this.terminalService.createSession({
        cwd: script.executor?.cwd,
        env: script.executor?.environment,
      });
    } catch (error) {
      return {
        success: false,
        status: "FAILED",
        output: "",
        rounds: [],
        executionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
      };
    }

    const bgResult = await this.terminalService.startBackgroundCommand(
      session.sessionId,
      command,
      { timeout: roundTimeout * maxRounds },
    );

    if (!bgResult.success) {
      await this.terminalService.terminateSession(session.sessionId);
      return {
        success: false,
        status: "FAILED",
        output: "",
        rounds: [],
        executionTime: Date.now() - startTime,
        error: bgResult.error || "Failed to start background command",
      };
    }

    // Step 3: Poll for initial output
    let accumulatedOutput = await this.pollForOutput(
      session.sessionId,
      roundTimeout,
      promptPatterns,
      "",
      abortSignal,
    );

    // Step 4: Interaction loop
    for (let i = 0; i < maxRounds; i++) {
      if (abortSignal?.aborted) {
        break;
      }

      if (!this.needsInput(accumulatedOutput, promptPatterns)) {
        break;
      }

      let input: string;
      try {
        input = await this.getResponse(accumulatedOutput, interactionMode, roundTimeout);
      } catch (error) {
        await this.cleanupSession(session.sessionId);
        return {
          success: false,
          status: "FAILED",
          output: accumulatedOutput,
          rounds,
          executionTime: Date.now() - startTime,
          error: error instanceof Error ? error.message : String(error),
        };
      }

      const sent = await this.terminalService.sendInput(session.sessionId, input);
      if (!sent) {
        logger.warn("Failed to send input, process may have ended", { scriptName, round: i + 1 });
        break;
      }

      const newOutput = await this.pollForOutput(
        session.sessionId,
        roundTimeout,
        promptPatterns,
        accumulatedOutput,
        abortSignal,
      );
      accumulatedOutput += newOutput;

      rounds.push({ input, output: accumulatedOutput, round: i + 1 });
    }

    // Step 5: Cleanup
    await this.cleanupSession(session.sessionId);

    return {
      success: true,
      status: "COMPLETED",
      output: accumulatedOutput,
      rounds,
      executionTime: Date.now() - startTime,
    };
  }

  /**
   * Render the script command from the script definition
   */
  private renderCommand(script: Script): string {
    if (script.template) {
      const templateEngine = new ScriptTemplateEngine();
      const args = script.arguments || [];
      const resolvedArgs = templateEngine.resolveArguments(args);
      const renderResult = templateEngine.render(script.template, resolvedArgs as Record<string, unknown>);
      if (!renderResult.resolved) {
        logger.warn("Template has unresolved placeholders", {
          scriptName: script.name,
          placeholders: renderResult.unresolvedPlaceholders,
        });
      }
      return renderResult.command;
    }
    return script.content || "";
  }

  /**
   * Poll for output from the session until prompt pattern detected, process idle, or timeout.
   * Returns only the NEW output accumulated during this poll.
   */
  private async pollForOutput(
    sessionId: string,
    timeout: number,
    promptPatterns: string[],
    accumulatedSoFar: string,
    abortSignal?: AbortSignal,
  ): Promise<string> {
    const pollStart = Date.now();
    let output = "";
    let idlePolls = 0;

    while (Date.now() - pollStart < timeout) {
      if (abortSignal?.aborted) break;
      await this.delay(200);
      const newOutput = await this.terminalService.getOutput(sessionId);
      if (newOutput) {
        if (output) {
          output += `\n${newOutput}`;
        } else {
          output = newOutput;
        }
        idlePolls = 0;
        if (this.needsInput(accumulatedSoFar + output, promptPatterns)) {
          break;
        }
      } else {
        idlePolls++;
        if (idlePolls >= 5) {
          break;
        }
      }
    }

    const finalOutput = await this.terminalService.getOutput(sessionId);
    if (finalOutput) {
      if (output) {
        output += `\n${finalOutput}`;
      } else {
        output = finalOutput;
      }
    }

    return output;
  }

  /**
   * Clean up the PTY session
   */
  private async cleanupSession(sessionId: string): Promise<void> {
    try {
      await this.terminalService.killBackgroundCommand(sessionId);
    } catch {
      // Ignore cleanup errors
    }
    try {
      await this.terminalService.terminateSession(sessionId);
    } catch {
      // Ignore cleanup errors
    }
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
        const regex = new RegExp(pattern, "m");
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

  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}
