/**
 * State Managers Module Export
 * Provides state management classes for agent loop execution.
 * 
 * Note: Agent's message history is now managed exclusively through
 * ConversationSession (sdk/core/messaging/conversation-session.ts).
 * The legacy agent-specific MessageHistory has been removed.
 */

export { AgentLoopState } from "./agent-loop-state.js";
