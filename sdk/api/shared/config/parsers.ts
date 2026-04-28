/**
 * Configuration Parser Functions
 * Provides a purely functional configuration parsing interface
 *
 * Design Principles:
 * - All functions are pure functions
 * - hold no state
 * - No file I/O
 * - No registry manipulation
 */

import type { WorkflowDefinition } from "@wf-agent/types";
import type { NodeTemplate } from "@wf-agent/types";
import type { TriggerTemplate } from "@wf-agent/types";
import type { Script } from "@wf-agent/types";
import type { LLMProfile } from "@wf-agent/types";
import { ConfigFormat } from "./types.js";
import type { ParsedConfig } from "./types.js";
import { ConfigParser } from "./config-parser.js";

// Shared ConfigParser instances
const workflowParser = new ConfigParser();
const nodeTemplateParser = new ConfigParser();
const triggerTemplateParser = new ConfigParser();
const scriptParser = new ConfigParser();
const llmProfileParser = new ConfigParser();

/**
 * Parsing Workflow Configurations
 * @param content Configuration file content
 * @param format Configuration format
 * @param parameters runtime parameters (for template replacement)
 * @returns WorkflowDefinition
 */
export async function parseWorkflow(
  content: string,
  format: ConfigFormat,
  parameters?: Record<string, unknown>,
): Promise<WorkflowDefinition> {
  return workflowParser.parseAndTransform(content, format, parameters);
}

/**
 * Validate workflow configuration (via content)
 * @param content Configuration file content
 * @param format Configuration format
 * @returns Validation results
 */
export function validateWorkflowByContent(content: string, format: ConfigFormat) {
  const parsed = workflowParser.parse(content, format, "workflow");
  return workflowParser.validate(parsed);
}

/**
 * Parsing a workflow configuration (without conversion)
 * @param content Configuration file content
 * @param format Configuration format
 * @returns The parsed configuration object
 */
export function parseWorkflowConfig(
  content: string,
  format: ConfigFormat,
): ParsedConfig<"workflow"> {
  return workflowParser.parse(content, format, "workflow");
}

/**
 * Batch Parsing Workflow Configuration
 * @param contents Configuration contents array
 * @param formats Configuration formats array
 * @param parameters Array of runtime parameters
 * @returns WorkflowDefinition array
 */
export async function parseBatchWorkflows(
  contents: string[],
  formats: ConfigFormat[],
  parameters?: Record<string, unknown>[],
): Promise<WorkflowDefinition[]> {
  if (contents.length !== formats.length) {
    throw new Error("The lengths of the `contents` and `formats` arrays must be the same.");
  }
  return Promise.all(
    contents.map((content, index) => parseWorkflow(content, formats[index]!, parameters?.[index])),
  );
}

/**
 * Parsing Node Template Configuration
 * @param content Configuration file content
 * @param format Configuration format
 * @returns NodeTemplate
 */
export function parseNodeTemplate(content: string, format: ConfigFormat): NodeTemplate {
  const config = nodeTemplateParser.parse(content, format, "node_template");
  return config.config as NodeTemplate;
}

/**
 * Batch Parsing Node Template Configuration
 * @param contents Configuration content array
 * @param formats Configuration formats array
 * @returns NodeTemplate array
 */
export function parseBatchNodeTemplates(
  contents: string[],
  formats: ConfigFormat[],
): NodeTemplate[] {
  if (contents.length !== formats.length) {
    throw new Error("The lengths of the `contents` and `formats` arrays must be the same.");
  }
  return contents.map((content, index) => parseNodeTemplate(content, formats[index]!));
}

/**
 * Parsing Trigger Template Configuration
 * @param content Configuration file content
 * @param format Configuration format
 * @returns TriggerTemplate
 */
export function parseTriggerTemplate(content: string, format: ConfigFormat): TriggerTemplate {
  const config = triggerTemplateParser.parse(content, format, "trigger_template");
  return config.config as TriggerTemplate;
}

/**
 * Batch Parsing Trigger Template Configuration
 * @param contents Configuration contents array
 * @param formats Configuration formats array
 * @returns TriggerTemplate array
 */
export function parseBatchTriggerTemplates(
  contents: string[],
  formats: ConfigFormat[],
): TriggerTemplate[] {
  if (contents.length !== formats.length) {
    throw new Error("The lengths of the `contents` and `formats` arrays must be the same.");
  }
  return contents.map((content, index) => parseTriggerTemplate(content, formats[index]!));
}

/**
 * Parsing Script Configuration
 * @param content Configuration file content
 * @param format Configuration format
 * @returns Script
 */
export function parseScript(content: string, format: ConfigFormat): Script {
  const config = scriptParser.parse(content, format, "script");
  return config.config as Script;
}

/**
 * Batch Parsing Script Configuration
 * @param contents Configuration contents array
 * @param formats Configuration formats array
 * @returns Script array
 */
export function parseBatchScripts(contents: string[], formats: ConfigFormat[]): Script[] {
  if (contents.length !== formats.length) {
    throw new Error("The lengths of the `contents` and `formats` arrays must be the same.");
  }
  return contents.map((content, index) => parseScript(content, formats[index]!));
}

/**
 * Parsing the LLM Profile Configuration
 * @param content Configuration file content
 * @param format Configuration format
 * @returns LLMProfile
 */
export function parseLLMProfile(content: string, format: ConfigFormat): LLMProfile {
  const config = llmProfileParser.parse(content, format, "llm_profile");
  return config.config as LLMProfile;
}

/**
 * Batch Parsing LLM Profile Configuration
 * @param contents Configuration contents array
 * @param formats Configuration formats array
 * @returns LLMProfile array
 */
export function parseBatchLLMProfiles(contents: string[], formats: ConfigFormat[]): LLMProfile[] {
  if (contents.length !== formats.length) {
    throw new Error("The lengths of the `contents` and `formats` arrays must be the same.");
  }
  return contents.map((content, index) => parseLLMProfile(content, formats[index]!));
}
