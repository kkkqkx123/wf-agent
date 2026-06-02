import type { PromptTemplate } from "./prompt-template.js";

export interface SystemPromptFragment {
  id: string;
  category: "role" | "capability" | "constraint" | "tool-usage";
  content: string;
  description?: string;
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