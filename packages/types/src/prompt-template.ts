export interface PromptVariableDefinition {
  name: string;
  type: "string" | "number" | "boolean" | "object" | "array";
  required: boolean;
  description?: string;
  defaultValue?: unknown;
}

export interface PromptTemplate {
  id: string;
  name: string;
  description: string;
  category: "system" | "rules" | "user-command" | "tools" | "composite" | "fragments" | "dynamic";
  content: string;
  variables?: PromptVariableDefinition[];
  fragments?: string[];
}

export interface TemplateFillRule {
  templateId: string;
  variableMapping: Record<string, string>;
  fragmentMapping?: Record<string, string>;
}
