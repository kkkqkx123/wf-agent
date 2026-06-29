/**
 * Unified Registry Configuration Interfaces
 *
 * Provides a consistent configuration structure for all registries.
 * This ensures:
 * - All registries follow a unified interface
 * - Configurations can be centrally managed in SDKOptions
 * - Easy extensibility for new registries
 */

import type {
  WorkflowStorageAdapter,
  WorkflowExecutionStorageAdapter,
  TaskStorageAdapter,
  AgentLoopStorageAdapter,
  TriggerStorageAdapter,
  ToolStorageAdapter,
  ScriptStorageAdapter,
  NodeTemplateStorageAdapter,
  HookTemplateStorageAdapter,
  AgentProfileStorageAdapter,
} from "@wf-agent/storage";

/**
 * Global persistence strategy configuration
 * Applies as default to all registries unless overridden at registry level
 */
export interface GlobalPersistenceConfig {
  /** Enable persistence globally by default */
  enabled?: boolean;

  /** Auto-persist on modification (vs manual persist) */
  autoPersist?: boolean;

  /** Automatically initialize all persistable registries on SDK bootstrap */
  autoInitialize?: boolean;

  /** Default timeout for persistence operations (milliseconds) */
  persistenceTimeout?: number;

  /** Retry strategy for failed persistence operations */
  retryStrategy?: {
    /** Maximum retry attempts */
    maxAttempts?: number;
    /** Delay between retries (milliseconds) */
    delayMs?: number;
    /** Exponential backoff multiplier */
    backoffMultiplier?: number;
  };
}

/**
 * Base registry configuration interface
 * All registry-specific configs should extend this
 */
export interface RegistryConfig {
  /** Storage adapter instance for this registry */
  storageAdapter?: any; // Will be typed as specific adapter in extensions

  /** Enable auto-persistence for this registry */
  autoPersist?: boolean;

  /** Auto-initialize on SDK bootstrap */
  autoInitialize?: boolean;
}

/**
 * Tool Registry Configuration
 */
export interface ToolRegistryConfig extends RegistryConfig {
  storageAdapter?: ToolStorageAdapter;

  /** REST executor configuration */
  restExecutorConfig?: Record<string, any>;
}

/**
 * Skill Registry Configuration
 */
export interface SkillRegistryConfig extends RegistryConfig {
  /** Directory paths to scan for skill definitions */
  paths?: string[];

  /** Enable automatic skill directory scanning */
  autoScan?: boolean;

  /** Timeout for loading skill files (milliseconds) */
  loadTimeout?: number;
}

/**
 * Task Registry Configuration
 */
export interface TaskRegistryConfig extends RegistryConfig {
  storageAdapter?: TaskStorageAdapter;

  /** Enable auto-persistence of task changes */
  autoPersist?: boolean;

  /** Auto-initialize from storage */
  autoInitialize?: boolean;
}

/**
 * Workflow Registry Configuration
 */
export interface WorkflowRegistryConfig extends RegistryConfig {
  storageAdapter?: WorkflowStorageAdapter;

  /** Initialize related registries together */
  autoInitializeRelated?: boolean;
}

/**
 * Workflow Execution Registry Configuration
 */
export interface WorkflowExecutionRegistryConfig extends RegistryConfig {
  storageAdapter?: WorkflowExecutionStorageAdapter;

  /** Retention policy for old executions */
  retentionDays?: number;

  /** Maximum number of execution records to keep */
  maxRecords?: number;
}

/**
 * Node Template Registry Configuration
 */
export interface NodeTemplateRegistryConfig extends RegistryConfig {
  storageAdapter?: NodeTemplateStorageAdapter;
}

/**
 * Hook Template Registry Configuration
 */
export interface HookTemplateRegistryConfig extends RegistryConfig {
  storageAdapter?: HookTemplateStorageAdapter;
}

/**
 * Trigger Template Registry Configuration
 */
export interface TriggerTemplateRegistryConfig extends RegistryConfig {
  storageAdapter?: TriggerStorageAdapter;
}

/**
 * Agent Loop Registry Configuration
 */
export interface AgentLoopRegistryConfig extends RegistryConfig {
  storageAdapter?: AgentLoopStorageAdapter;

  /** Checkpoint persistence config specific to agent loops */
  persistenceConfig?: {
    checkpointInterval?: number;
    maxCheckpointSize?: number;
  };
}

/**
 * Script Registry Configuration
 */
export interface ScriptRegistryConfig extends RegistryConfig {
  storageAdapter?: ScriptStorageAdapter;

  /** Script execution timeout (milliseconds) */
  executionTimeout?: number;

  /** Enable script validation on registration */
  enableValidation?: boolean;
}

/**
 * Agent Profile Registry Configuration
 */
export interface AgentProfileRegistryConfig extends RegistryConfig {
  storageAdapter?: AgentProfileStorageAdapter;

  /** Profile caching strategy */
  cacheProfiles?: boolean;

  /** Cache expiration time (milliseconds) */
  cacheExpiration?: number;
}

/**
 * Prompt Template Registry Configuration
 */
export interface PromptTemplateRegistryConfig extends RegistryConfig {
  /** No storage adapter typically needed */
}

/**
 * Fragment Registry Configuration
 */
export interface FragmentRegistryConfig extends RegistryConfig {
  /** No storage adapter typically needed */
}

/**
 * Event Registry Configuration
 */
export interface EventRegistryConfig extends RegistryConfig {
  /** Maximum listener queue size */
  maxListenerQueueSize?: number;

  /** Default listener timeout (milliseconds) */
  defaultListenerTimeout?: number;

  /** Slow listener threshold (milliseconds) */
  slowListenerThreshold?: number;

  /** Enable backpressure control */
  enableBackpressure?: boolean;

  /** Maximum event history size */
  maxEventHistory?: number;
}

/**
 * Metrics Registry Configuration
 */
export interface MetricsRegistryConfig extends RegistryConfig {
  /** Enable metrics collection */
  enabled?: boolean;

  /** Enable periodic reporting */
  enablePeriodicReporting?: boolean;

  /** Reporting interval (milliseconds) */
  reportingInterval?: number;

  /** Custom metrics configuration */
  customMetrics?: Record<string, any>;
}

/**
 * Unified Registry Configurations Map
 * Contains configuration for all registries
 * This is the single source of truth for registry settings
 */
export interface RegistriesConfig {
  /** Global persistence strategy (applies as default to all registries) */
  persistence?: GlobalPersistenceConfig;

  /** Tool Registry configuration */
  tools?: ToolRegistryConfig;

  /** Skill Registry configuration */
  skills?: SkillRegistryConfig;

  /** Task Registry configuration */
  tasks?: TaskRegistryConfig;

  /** Workflow Registry configuration */
  workflows?: WorkflowRegistryConfig;

  /** Workflow Execution Registry configuration */
  workflowExecutions?: WorkflowExecutionRegistryConfig;

  /** Node Template Registry configuration */
  nodeTemplates?: NodeTemplateRegistryConfig;

  /** Hook Template Registry configuration */
  hookTemplates?: HookTemplateRegistryConfig;

  /** Trigger Template Registry configuration */
  triggers?: TriggerTemplateRegistryConfig;

  /** Agent Loop Registry configuration */
  agentLoops?: AgentLoopRegistryConfig;

  /** Script Registry configuration */
  scripts?: ScriptRegistryConfig;

  /** Agent Profile Registry configuration */
  agentProfiles?: AgentProfileRegistryConfig;

  /** Prompt Template Registry configuration */
  promptTemplates?: PromptTemplateRegistryConfig;

  /** Fragment Registry configuration */
  fragments?: FragmentRegistryConfig;

  /** Event Registry configuration */
  events?: EventRegistryConfig;

  /** Metrics Registry configuration */
  metrics?: MetricsRegistryConfig;
}

/**
 * Registry initialization order configuration
 * Specifies the order in which registries should be initialized
 */
export interface RegistryInitializationOrder {
  /** Priority level (higher number = higher priority, initialize first) */
  priority: number;

  /** Dependencies on other registries (by name) */
  dependencies?: string[];

  /** Whether this registry can be initialized asynchronously */
  async?: boolean;

  /** Whether this registry must be initialized before SDK is ready */
  required?: boolean;
}

/**
 * Registry metadata for initialization management
 */
export const REGISTRY_INITIALIZATION_METADATA: Record<string, RegistryInitializationOrder> = {
  // Layer 1: No dependencies
  workflowGraph: {
    priority: 100,
    dependencies: [],
    async: false,
    required: false,
  },
  workflowExecution: {
    priority: 100,
    dependencies: [],
    async: false,
    required: false,
  },
  event: {
    priority: 100,
    dependencies: [],
    async: false,
    required: false,
  },

  // Layer 2: Depends on event registry
  tool: {
    priority: 90,
    dependencies: ["event"],
    async: false,
    required: false,
  },
  script: {
    priority: 90,
    dependencies: [],
    async: false,
    required: false,
  },
  nodeTemplate: {
    priority: 90,
    dependencies: [],
    async: false,
    required: false,
  },
  hookTemplate: {
    priority: 90,
    dependencies: [],
    async: false,
    required: false,
  },
  trigger: {
    priority: 90,
    dependencies: [],
    async: false,
    required: false,
  },

  // Layer 3: Skills (depends on file loader)
  skill: {
    priority: 80,
    dependencies: ["event"],
    async: true,
    required: false,
  },
  agentProfile: {
    priority: 80,
    dependencies: [],
    async: false,
    required: false,
  },
  promptTemplate: {
    priority: 80,
    dependencies: [],
    async: false,
    required: false,
  },
  fragment: {
    priority: 80,
    dependencies: [],
    async: false,
    required: false,
  },

  // Layer 4: Depends on storage
  workflowRelationship: {
    priority: 70,
    dependencies: [],
    async: false,
    required: false,
  },
  workflow: {
    priority: 70,
    dependencies: ["workflowExecution", "workflowRelationship", "workflowGraph"],
    async: false,
    required: false,
  },

  // Layer 5: Executors
  llm: {
    priority: 60,
    dependencies: ["event"],
    async: false,
    required: false,
  },
  toolCall: {
    priority: 60,
    dependencies: ["tool", "event"],
    async: false,
    required: false,
  },

  // Layer 6: Agent loop
  agentLoop: {
    priority: 50,
    dependencies: [],
    async: false,
    required: false,
  },
  executionHierarchy: {
    priority: 50,
    dependencies: [],
    async: false,
    required: false,
  },

  // Layer 7+: Higher level coordinators and managers
  metrics: {
    priority: 40,
    dependencies: [],
    async: false,
    required: false,
  },
  task: {
    priority: 40,
    dependencies: [],
    async: false,
    required: false,
  },
};

/**
 * Helper function to resolve registry initialization order
 * Sorts registries by priority and validates dependencies
 *
 * @param registriesToInit Names of registries to initialize
 * @returns Sorted array of registry names in initialization order
 * @throws {Error} If there are circular dependencies or missing dependencies
 */
export function resolveInitializationOrder(registriesToInit: string[]): string[] {
  const sorted: string[] = [];
  const visited = new Set<string>();
  const visiting = new Set<string>();

  function visit(name: string) {
    if (visited.has(name)) {
      return;
    }

    if (visiting.has(name)) {
      throw new Error(`Circular dependency detected involving registry: ${name}`);
    }

    const metadata = REGISTRY_INITIALIZATION_METADATA[name];
    if (!metadata) {
      throw new Error(`Unknown registry: ${name}`);
    }

    visiting.add(name);

    // Visit dependencies first
    if (metadata.dependencies) {
      for (const dep of metadata.dependencies) {
        if (registriesToInit.includes(dep)) {
          visit(dep);
        }
      }
    }

    visiting.delete(name);
    visited.add(name);
    sorted.push(name);
  }

  // Sort by priority first, then resolve dependencies
  const sortedByPriority = registriesToInit.sort(
    (a, b) =>
      (REGISTRY_INITIALIZATION_METADATA[b]?.priority ?? 0) -
      (REGISTRY_INITIALIZATION_METADATA[a]?.priority ?? 0),
  );

  for (const name of sortedByPriority) {
    visit(name);
  }

  return sorted;
}
