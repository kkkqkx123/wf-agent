import type {
  WorkflowTemplate,
  AgentLoopDefinition,
  NodeTemplate,
  TriggerTemplate,
  HookTemplate,
  PromptTemplate,
} from "@wf-agent/types";

type MaybePromise<T> = T | Promise<T>;

export interface StarterConfigField {
  type: "string" | "number" | "boolean" | "expression" | "array" | "object";
  default?: unknown;
  description: string;
  required?: boolean;
  allowedFunctions?: string[];
}

export interface StarterMetadata {
  id: string;
  name: string;
  version: string;
  description: string;
  author?: string;
  tags?: string[];
  category?: string;
  dependencies?: string[];
  configurable?: Record<string, StarterConfigField>;
}

export interface WorkflowBundle {
  workflow: WorkflowTemplate;
  agentLoops?: AgentLoopDefinition[];
  nodeTemplates?: NodeTemplate[];
  triggerTemplates?: TriggerTemplate[];
  hookTemplates?: HookTemplate[];
  promptTemplates?: PromptTemplate[];
}

export interface WorkflowStarter<C extends Record<string, unknown> = Record<string, unknown>> {
  readonly metadata: StarterMetadata;
  assemble(config: C): WorkflowBundle;
  onBeforeAssemble?(config: C): MaybePromise<void>;
  onAfterInstall?(bundle: WorkflowBundle): MaybePromise<void>;
  onBeforeUninstall?(): MaybePromise<void>;
  onAfterUninstall?(): MaybePromise<void>;
}

export interface StarterRegistries {
  workflowRegistry: {
    register(workflow: WorkflowTemplate): unknown;
    unregister(id: string): unknown;
  };
  nodeTemplateRegistry: {
    register(template: NodeTemplate): unknown;
    unregister(name: string): unknown;
  };
  triggerTemplateRegistry: {
    register(template: TriggerTemplate): unknown;
    unregister(name: string): unknown;
  };
  hookTemplateRegistry: {
    register(template: HookTemplate): unknown;
    unregister(name: string): unknown;
  };
  agentLoopRegistry: {
    register(loop: AgentLoopDefinition): unknown;
    unregister(id: string): unknown;
  };
  promptTemplateRegistry: {
    register(key: string, template: PromptTemplate): unknown;
    unregister(key: string): unknown;
  };
}
