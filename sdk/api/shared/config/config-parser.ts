/**
 * Configure the main class of the parser
 * Integrate TOML/JSON parsing and validation functions
 *
 * Design principles:
 * - Use a pure function processor architecture, where all processing logic is stateless and implemented as pure functions
 * - The api/config layer is only responsible for parsing configuration content and does not perform file I/O operations
 * - Validation logic is delegated to the pure functions within the processors, which in turn delegate it to the validators in the core layer
 * - ConfigParser is a generic module that does not contain any business logic specific to a particular configuration type
 * - Support multiple configuration types: WORKFLOW, NODE_TEMPLATE, TRIGGER_TEMPLATE, SCRIPT, LLM_PROFILE
 */

import type { ParsedConfig, IConfigParser } from "./types.js";
import { ConfigFormat, ConfigType } from "./types.js";
import type { WorkflowDefinition } from "@wf-agent/types";
import { parseToml } from "./toml-parser.js";
import { parseJson } from "./json-parser.js";
import { ConfigurationError } from "@wf-agent/types";
import { transformWorkflow } from "./processors/workflow.js";
import {
  validateWorkflow,
  validateNodeTemplate,
  validateScript,
  validateTriggerTemplate,
  validateLLMProfile,
  validatePromptTemplate,
} from "./processors/index.js";

/**
 * Configure the parser class
 */
export class ConfigParser implements IConfigParser {
  constructor() {
    // The handler is managed through the registry, eliminating the need for initialization in the constructor.
  }

  /**
   * Parse the content of the configuration file
   * @param content: The content of the configuration file
   * @param format: The format of the configuration
   * @returns: The parsed configuration object
   */
  parse<T extends ConfigType = "workflow">(
    content: string,
    format: ConfigFormat,
    configType?: T,
  ): ParsedConfig<T> {
    let config: unknown;

    // Select a parser based on the format.
    switch (format) {
      case "toml":
        config = parseToml(content);
        break;
      case "json":
        config = parseJson(content);
        break;
      default:
        throw new ConfigurationError(`Unsupported configuration format: ${format}`, format);
    }

    return {
      configType: (configType || "workflow") as T,
      format,
      config: config as ParsedConfig<T>["config"],
      rawContent: content,
    };
  }

  /**
   * Verify the validity of the configuration
   * Use the corresponding pure function to perform the validation
   * @param config The parsed configuration
   * @returns The validation result
   */
  validate<T extends ConfigType>(config: ParsedConfig<T>) {
    switch (config.configType) {
      case "workflow":
        return validateWorkflow(config as ParsedConfig<"workflow">);
      case "node_template":
        return validateNodeTemplate(config as ParsedConfig<"node_template">);
      case "script":
        return validateScript(config as ParsedConfig<"script">);
      case "trigger_template":
        return validateTriggerTemplate(config as ParsedConfig<"trigger_template">);
      case "llm_profile":
        return validateLLMProfile(config as ParsedConfig<"llm_profile">);
      case "prompt_template":
        return validatePromptTemplate(config as ParsedConfig<"prompt_template">);
      default:
        throw new ConfigurationError(
          `No handler found for config type ${config.configType}`,
          config.configType,
        );
    }
  }

  /**
   * Parse and validate the configuration (universal method)
   * @param content: Content of the configuration file
   * @param format: Configuration format
   * @param configType: Configuration type
   * @returns: The validated configuration object
   */
  parseAndValidate<T extends ConfigType>(
    content: string,
    format: ConfigFormat,
    configType: T,
  ): ParsedConfig<T> {
    // Parse the configuration.
    const parsedConfig = this.parse(content, format, configType);

    // Verify the configuration.
    const validationResult = this.validate(parsedConfig);
    if (validationResult.isErr()) {
      const errorMessages = validationResult.error.map(err => err.message).join("\n");
      throw new ConfigurationError(
        `Configuration validation failed:\n${errorMessages}`,
        undefined,
        {
          errors: validationResult.error,
        },
      );
    }

    return parsedConfig;
  }

  /**
   * Parse and convert the configuration into a WorkflowDefinition.
   * @param content: The content of the configuration file.
   * @param format: The format of the configuration.
   * @param parameters: Runtime parameters (optional).
   * @returns: A WorkflowDefinition.
   */
  async parseAndTransform(
    content: string,
    format: ConfigFormat,
    parameters?: Record<string, unknown>,
  ): Promise<WorkflowDefinition> {
    // Parse the configuration.
    const parsedConfig = this.parse(content, format, "workflow");

    // Transform the parsed configuration to WorkflowDefinition
    return transformWorkflow(parsedConfig, parameters);
  }
}
