# Agent Loop Architecture: Iteration & State Management

## Overview

The Agent Loop in this project is designed as a **pure, message-driven execution cycle**. Unlike Workflows, which rely on complex variable scopes and node-level state management, the Agent Loop maintains its state primarily through **Message History** and **Iteration Records**.

## Key Design Principles

1.  **No Internal Variables**: The Agent Loop does not maintain a `variables` or `state` object for business logic. All context is derived from the conversation history (`messages`).
2.  **Execution Wrapper Responsibility**: Metadata such as `status`, `timestamps`, and `parentContext` are managed by the `AgentLoopEntity` wrapper, not by the internal state manager.
3.  **Checkpoint Simplicity**: Checkpoints serialize only the essential execution trajectory (messages + iteration history), ensuring fast restoration without complex state reconciliation.
4.  **Event-Driven Extension**: Any custom logic or external state tracking should be handled via `AgentHookTriggeredEvent` or external monitoring systems, not by injecting data into the Agent's internal model.

## Why No `state` Field?

- **Avoid Logic Splitting**: If an Agent reads from both `messages` and `state`, it creates ambiguity about which source is authoritative.
- **Workflow vs. Agent**: Complex control flow and persistent state belong in a Workflow. The Agent is a "tool-using loop" that reacts to its environment via prompts.
- **Human Interaction**: Handled via `steer()`/`followUp()` methods which inject new messages, keeping the interaction model consistent.

## Migration Guide

If you were previously using `variables` or `state` in your Agent implementation:

| Old Approach                            | New Recommended Approach                                                             |
| --------------------------------------- | ------------------------------------------------------------------------------------ |
| `entity.setVariable('key', value)`      | Include the information in the next `LLMMessage` (User or Tool Result).              |
| `const val = entity.getVariable('key')` | Extract from `entity.getMessages()` history.                                         |
| Storing task progress in `state`        | Use a dedicated Workflow node to track progress, or include it in the System Prompt. |

## Documentation Status

The previous proposal for iteration-scoped variables has been **deprecated and discarded**. The current architecture favors simplicity and alignment with LLM-native patterns.
