/**
 * ToolVisibilityCoordinator - Tool Visibility Coordinator
 * Coordinates the process of tool visibility changes and generates visibility declaration messages.
 *
 * Key Responsibilities:
 * 1. Coordinates the process of tool visibility changes
 * 2. Generates structured visibility declarations
 * 3. Triggers declaration updates when scope is switched
 * 4. Supports the dynamic addition of tools
 * 5. Avoids duplicate declarations and optimizes token consumption
 *
 * Design Principles:
 * - Stateless design: Does not maintain any state; all state is accessed through managers
 * - Coordination logic: Encapsulates the logic for coordinating visibility changes
 * - Dependency injection: Receives managers and services required as dependencies through the constructor
 * - Incremental declarations: Adds new messages to declare the currently available set of tools
 * - Explicit overriding: New declarations overwrite old ones to create an "effective tool snapshot"
 * - Dual safeguards: Prompt messages and execution interception to ensure security
 * - Maintains KV cache: Does not modify historical messages to avoid cache invalidation
 * - Declaration deduplication: Prevents duplicate declarations of the same set of tools within a short period
 */

import type { ToolScope } from "../../stores/tool-context-store.js";
import type {
  ToolVisibilityContext,
  VisibilityDeclaration,
  VisibilityChangeType,
} from "../types/tool-visibility.types.js";
import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { ToolRegistry } from "../../../core/registry/tool-registry.js";
import type { LLMMessage } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import { ToolVisibilityStore } from "../../stores/tool-visibility-store.js";
import { ToolVisibilityMessageBuilder } from "../utils/tool-visibility-message-builder.js";

/**
 * ToolVisibilityCoordinator - Tool Visibility Coordinator
 */
export class ToolVisibilityCoordinator {
  /** Tool Visibility Store (Stateful) */
  private toolVisibilityStore: ToolVisibilityStore;

  /** Tool Services */
  private toolService: ToolRegistry;

  /** Message Builder */
  private messageBuilder: ToolVisibilityMessageBuilder;

  /**
   * Constructor
   * @param toolService: Tool service
   * @param toolVisibilityStore: Tool visibility store
   */
  constructor(toolService: ToolRegistry, toolVisibilityStore?: ToolVisibilityStore) {
    this.toolService = toolService;
    this.toolVisibilityStore = toolVisibilityStore || new ToolVisibilityStore();
    this.messageBuilder = new ToolVisibilityMessageBuilder(toolService);
  }

  /**
   * Initialize visibility context
   * @param executionId Execution ID
   * @param initialTools List of initial tool IDs
   * @param scope Initial scope
   * @param scopeId Scope ID
   */
  initializeContext(
    executionId: string,
    initialTools: string[],
    scope: ToolScope = "EXECUTION",
    scopeId: string = executionId,
  ): void {
    this.toolVisibilityStore.initializeContext(executionId, initialTools, scope, scopeId);
  }

  /**
   * Get visibility context
   * @param executionId Execution ID
   * @returns Tool visibility context; returns undefined if it does not exist
   */
  getContext(executionId: string): ToolVisibilityContext | undefined {
    return this.toolVisibilityStore.getContext(executionId);
  }

  /**
   * Update visibility when scope is switched
   * Generate and add new visibility declaration messages
   * @param executionEntity WorkflowExecution entity
   * @param newScope New scope
   * @param newScopeId New scope ID
   * @param availableTools List of available tool IDs
   * @param changeType Type of change
   */
  async updateVisibilityOnScopeChange(
    executionEntity: WorkflowExecutionEntity,
    newScope: ToolScope,
    newScopeId: string,
    availableTools: string[],
    changeType: VisibilityChangeType = "enter_scope",
  ): Promise<void> {
    const executionId = executionEntity.id;
    const context = this.getContext(executionId);

    if (!context) {
      // If the context does not exist, initialize it first.
      this.initializeContext(executionId, availableTools, newScope, newScopeId);
    } else {
      // Update the status in the store.
      this.toolVisibilityStore.updateVisibility(executionId, availableTools, newScope, newScopeId);
    }

    // Check if it is necessary to skip duplicate declarations.
    if (this.shouldSkipDeclaration(executionId, availableTools, changeType)) {
      return;
    }

    // Generate a declaration message
    const message = this.messageBuilder.buildVisibilityDeclarationMessage(
      newScope,
      newScopeId,
      availableTools,
      changeType,
    );

    // Add to the conversation history
    const llmMessage: LLMMessage = {
      role: "system",
      content: message,
      metadata: this.messageBuilder.buildVisibilityDeclarationMetadata(
        newScope,
        newScopeId,
        availableTools,
        changeType,
      ),
    };

    executionEntity.addMessage(llmMessage);

    // Update the statement history.
    const declaration: VisibilityDeclaration = {
      timestamp: now(),
      scope: newScope,
      scopeId: newScopeId,
      toolIds: [...availableTools],
      messageIndex: executionEntity.getMessages().length - 1,
      changeType,
    };

    const updatedContext = this.getContext(executionId)!;
    updatedContext.declarationHistory.push(declaration);
    updatedContext.lastDeclarationIndex = declaration.messageIndex;
  }

  /**
   * Check whether to skip duplicate declarations
   * @param executionId Execution ID
   * @param availableTools List of available tool IDs
   * @param changeType Type of change
   * @returns Whether to skip the process
   */
  private shouldSkipDeclaration(
    executionId: string,
    availableTools: string[],
    changeType: VisibilityChangeType,
  ): boolean {
    // For important types of changes (such as enter_scope, exit_scope), do not skip them.
    if (changeType === "enter_scope" || changeType === "exit_scope") {
      return false;
    }

    const context = this.getContext(executionId);
    if (!context) {
      return false;
    }

    // Check if the toolsets are the same.
    const currentToolSet = new Set(availableTools);
    if (this.areToolSetsEqual(context.visibleTools, currentToolSet)) {
      return true; // Same toolkit, skip the declaration.
    }

    return false;
  }

  /**
   * Compare whether two toolsets are the same
   * @param set1 Toolset 1
   * @param set2 Toolset 2
   * @returns Whether they are the same
   */
  private areToolSetsEqual(set1: Set<string>, set2: Set<string>): boolean {
    if (set1.size !== set2.size) {
      return false;
    }
    for (const tool of set1) {
      if (!set2.has(tool)) {
        return false;
      }
    }
    return true;
  }

  /**
   * Dynamically add tools
   * Generate incremental visibility declarations
   * @param executionEntity WorkflowExecution entity
   * @param toolIds List of tool IDs
   * @param scope Scope
   */
  async addToolsDynamically(
    executionEntity: WorkflowExecutionEntity,
    toolIds: string[],
    scope: ToolScope,
  ): Promise<void> {
    const executionId = executionEntity.id;
    const context = this.getContext(executionId);

    if (!context) {
      throw new Error(`Tool visibility context not found for workflow execution: ${executionId}`);
    }

    // Add the visible tool collection to the store.
    this.toolVisibilityStore.addTools(executionId, toolIds);

    // Generate the declaration immediately.
    await this.updateVisibilityOnScopeChange(
      executionEntity,
      scope,
      context.scopeId,
      Array.from(context.visibleTools),
      "add_tools",
    );
  }

  /**
   * Refresh Visibility Statement
   * Used to periodically refresh or restore after message operations
   *
   * Description:
   * - This method is called after message operations such as truncate, filter, or clear.
   * - It ensures that the tool's visibility statement is consistent with the current message state.
   * - The SDK does not provide default implementations for message operations; these are defined by the application layer.
   * - New statements are generated only when there are changes to the toolkit.
   *
   * @param executionEntity Workflow Execution Entity
   */
  async refreshDeclaration(executionEntity: WorkflowExecutionEntity): Promise<void> {
    const executionId = executionEntity.id;
    const context = this.getContext(executionId);

    if (!context) {
      throw new Error(`Tool visibility context not found for workflow execution: ${executionId}`);
    }

    // Check if a refresh is needed (to see if there have been any changes to the toolkit).
    const currentTools = Array.from(context.visibleTools);

    // Get the toolkit of the last declaration
    const lastDeclaration =
      context.declarationHistory.length > 0
        ? context.declarationHistory[context.declarationHistory.length - 1]
        : null;

    // If the last declaration still exists and the toolkit is the same, there is no need to refresh.
    if (lastDeclaration) {
      const lastTools = lastDeclaration.toolIds;
      if (this.areToolSetsEqual(new Set(lastTools), new Set(currentTools))) {
        return; // The toolkit has not changed; skip the refresh.
      }
    }

    // The toolkit has changed, so new declarations need to be generated.
    await this.updateVisibilityOnScopeChange(
      executionEntity,
      context.currentScope,
      context.scopeId,
      currentTools,
      "refresh",
    );
  }

  /**
   * Get the current valid set of tools (used for performing interceptions)
   * @param executionId Execution ID
   * @returns The currently visible set of tools
   */
  getEffectiveVisibleTools(executionId: string): Set<string> {
    return this.toolVisibilityStore.getVisibleTools(executionId);
  }

  /**
   * Check if the tool is within the current visibility context.
   * @param executionId: Execution ID
   * @param toolId: Tool ID
   * @returns: Whether it is visible or not
   */
  isToolVisible(executionId: string, toolId: string): boolean {
    return this.toolVisibilityStore.isToolVisible(executionId, toolId);
  }

  /**
   * Verify the integrity of the declaration history
   * @param executionId: Execution ID
   * @param executionEntity: WorkflowExecution entity
   * @returns: Verification result
   */
  validateDeclarationHistory(
    executionId: string,
    executionEntity: WorkflowExecutionEntity,
  ): { valid: boolean; errors: string[] } {
    const context = this.getContext(executionId);
    if (!context) {
      return { valid: false, errors: ["Context not found"] };
    }

    const errors: string[] = [];
    const conversationHistory = executionEntity.getMessages();

    // Check each declaration record.
    for (let i = 0; i < context.declarationHistory.length; i++) {
      const declaration = context.declarationHistory[i]!;

      // Check whether the message index is valid.
      if (declaration.messageIndex < 0 || declaration.messageIndex >= conversationHistory.length) {
        errors.push(`Declaration ${i}: messageIndex ${declaration.messageIndex} out of range`);
        continue;
      }

      // Check if the message is a tool visibility declaration.
      const message = conversationHistory[declaration.messageIndex];
      if (
        !message ||
        !message.metadata ||
        message.metadata["type"] !== "tool_visibility_declaration"
      ) {
        errors.push(
          `Declaration ${i}: message at index ${declaration.messageIndex} is not a visibility declaration`,
        );
        continue;
      }

      // Check whether the scope information matches.
      if (
        message.metadata["scope"] !== declaration.scope ||
        message.metadata["scopeId"] !== declaration.scopeId
      ) {
        errors.push(`Declaration ${i}: scope mismatch in metadata`);
      }
    }

    // Check for any isolated declaration messages that exist in the history but are not recorded.
    const declarationMessages = conversationHistory.filter(
      (msg: { metadata?: { type?: string } }) =>
        msg?.metadata?.["type"] === "tool_visibility_declaration",
    );

    if (declarationMessages.length !== context.declarationHistory.length) {
      errors.push(
        `Declaration count mismatch: ${declarationMessages.length} messages vs ${context.declarationHistory.length} records`,
      );
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  /**
   * Update the declaration history after message operations
   * This method is called when a message is truncated, filtered, or cleared.
   * @param executionId: Execution ID
   * @param executionEntity: WorkflowExecution entity
   * @param operation: Type of operation
   */
  async updateDeclarationHistoryAfterMessageOperation(
    executionId: string,
    executionEntity: WorkflowExecutionEntity,
    operation: "truncate" | "filter" | "clear",
  ): Promise<void> {
    const context = this.getContext(executionId);
    if (!context) {
      return;
    }

    const conversationHistory = executionEntity.getMessages();

    if (operation === "clear") {
      // Clear all declaration history
      context.declarationHistory = [];
      context.lastDeclarationIndex = -1;

      // Regenerate the initial declaration
      await this.updateVisibilityOnScopeChange(
        executionEntity,
        context.currentScope,
        context.scopeId,
        Array.from(context.visibleTools),
        "init",
      );
      return;
    }

    if (operation === "truncate" || operation === "filter") {
      // Remove declaration records that are out of scope.
      const validDeclarations = context.declarationHistory.filter(
        decl => decl.messageIndex < conversationHistory.length,
      );

      // If any declarations are removed, it is necessary to regenerate the latest declarations.
      if (validDeclarations.length !== context.declarationHistory.length) {
        context.declarationHistory = validDeclarations;

        // Regenerate the current visibility statement
        await this.updateVisibilityOnScopeChange(
          executionEntity,
          context.currentScope,
          context.scopeId,
          Array.from(context.visibleTools),
          "refresh",
        );
      }
    }
  }

  /**
   * Automatic repair declaration history
   * This method is called when verification fails.
   * @param executionId: Execution ID
   * @param executionEntity: WorkflowExecution entity
   */
  async repairDeclarationHistory(executionId: string, executionEntity: WorkflowExecutionEntity): Promise<void> {
    const context = this.getContext(executionId);
    if (!context) {
      return;
    }

    const conversationHistory = executionEntity.getMessages();

    // 1. Scan all tool visibility declaration messages in the conversation history.
    const declarationMessages: Array<{ index: number; message: LLMMessage }> = [];

    for (let i = 0; i < conversationHistory.length; i++) {
      const msg = conversationHistory[i];
      if (msg && msg.metadata && msg.metadata["type"] === "tool_visibility_declaration") {
        declarationMessages.push({ index: i, message: msg });
      }
    }

    // 2. Reconstruct the declaration history
    const rebuiltHistory: VisibilityDeclaration[] = [];

    for (const { index, message } of declarationMessages) {
      const metadata = message.metadata;
      if (metadata) {
        rebuiltHistory.push({
          timestamp: (metadata["timestamp"] as number) || now(),
          scope: (metadata["scope"] as ToolScope) || "EXECUTION",
          scopeId: (metadata["scopeId"] as string) || executionId,
          toolIds: (metadata["toolIds"] as string[]) || [],
          messageIndex: index,
          changeType: (metadata["changeType"] as VisibilityChangeType) || "refresh",
        });
      }
    }

    // 3. Update the context
    context.declarationHistory = rebuiltHistory;
    context.lastDeclarationIndex =
      rebuiltHistory.length > 0 ? rebuiltHistory[rebuiltHistory.length - 1]!.messageIndex : -1;

    // 4. If no declaration is made, generate an initial declaration.
    if (rebuiltHistory.length === 0) {
      await this.updateVisibilityOnScopeChange(
        executionEntity,
        context.currentScope,
        context.scopeId,
        Array.from(context.visibleTools),
        "init",
      );
    }
  }

  /**
   * Remove visibility context
   * @param executionId Execution ID
   */
  deleteContext(executionId: string): void {
    this.toolVisibilityStore.deleteContext(executionId);
  }

  /**
   * Clear all visibility contexts.
   */
  clearAll(): void {
    // Note: ToolVisibilityStore does not have a clearAll method, cleanup needs to be added or used
    // Temporarily clean up
    this.toolVisibilityStore.cleanup();
  }

  /**
   * Get a snapshot of the visibility context
   * @param executionId: Execution ID
   * @returns: Snapshot of the visibility context
   */
  getSnapshot(executionId: string): ToolVisibilityContext | undefined {
    return this.toolVisibilityStore.getSnapshot(executionId);
  }

  /**
   * Restore visibility context from a snapshot
   * @param executionId: Execution ID
   * @param snapshot: Visibility context snapshot
   */
  restoreSnapshot(executionId: string, snapshot: ToolVisibilityContext): void {
    this.toolVisibilityStore.restoreSnapshot(executionId, snapshot);
  }
}
