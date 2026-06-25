// Messaging exports
export { MessageHistory, type MessageHistoryState } from "./message-history.js";
export { MessageBuilder } from "./message-builder.js";
export {
  ConversationSession,
  type ConversationSessionConfig,
  type ConversationState,
} from "./conversation-session.js";
export { MessageArrayManager } from "./message-array-manager.js";
export { HistoryConverter, type HistoryConversionOptions } from "./history-converter.js";

// Base State Coordinator (shared by Agent/Workflow state coordinators)
export {
  BaseStateCoordinator,
  type StateCoordinatorSnapshot,
  type BaseStateCoordinatorConfig,
} from "./base-state-coordinator.js";

// Named Message Context exports
export { InMemoryMessageContextRegistry } from "./message-context-registry.js";
export { initializeExecutionContext, getOrCreateContext } from "./message-context-utils.js";
export type { NamedMessageContext, MessageContextRegistry } from "@wf-agent/types";

// Prompt submodule (template-based prompt assembly)
export * from "./prompt/index.js";

// Dynamic prompt injection (message-level context injection)
export { injectDynamicPrompts, type DynamicPromptInjectionResult } from "./dynamic-injection.js";
