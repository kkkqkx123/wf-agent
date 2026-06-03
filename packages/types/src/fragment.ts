import type { PromptTemplate, PromptVariableDefinition } from "./prompt-template.js";

export interface SystemPromptFragment {
  id: string;
  category: "role" | "capability" | "constraint" | "tool-usage" | "task-instruction";
  content: string;
  description?: string;
  /** Optional variable definitions for dynamic content rendering */
  variables?: PromptVariableDefinition[];
}

export interface FragmentCompositionConfig {
  fragmentIds: string[];
  separator?: string;
  prefix?: string;
  suffix?: string;
}

export interface TemplateComposition {
  baseTemplateId: string;
  overrides: Partial<PromptTemplate>;
  fragmentReplacements?: Record<string, string>;
}