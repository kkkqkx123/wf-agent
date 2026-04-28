/**
 * Workflow Reference Checker
 * Provides a workflow reference checking feature for secure deletion and update operations.
 */

import type { WorkflowRegistry } from "../../stores/workflow-registry.js";
import type { WorkflowExecutionRegistry } from "../../stores/workflow-execution-registry.js";
import type { WorkflowExecutionEntity } from "../../entities/index.js";
import type { WorkflowTrigger } from "@wf-agent/types";
import type { TriggerReference } from "@wf-agent/types";
import type { WorkflowReference, WorkflowReferenceInfo } from "@wf-agent/types";

/**
 * Check if the workflow is being referenced.
 * @param workflowRegistry: Workflow registry
 * @param workflowExecutionRegistry: Thread registry
 * @param workflowId: Workflow ID
 * @returns: Reference information
 */
export function checkWorkflowReferences(
  workflowRegistry: WorkflowRegistry,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
  workflowId: string,
): WorkflowReferenceInfo {
  const references: WorkflowReference[] = [];

  // Check sub-workflow references.
  const subgraphRefs = checkSubgraphReferences(workflowRegistry, workflowId);
  references.push(...subgraphRefs);

  // Check trigger references.
  const triggerRefs = checkTriggerReferences(workflowRegistry, workflowId);
  references.push(...triggerRefs);

  // Check runtime thread references.
  const threadRefs = checkThreadReferences(threadRegistry, workflowId);
  references.push(...threadRefs);

  const runtimeRefs = references.filter(ref => ref.isRuntimeReference).length;

  return {
    hasReferences: references.length > 0,
    references,
    canSafelyDelete: runtimeRefs === 0,
    stats: {
      subgraphReferences: subgraphRefs.length,
      triggerReferences: triggerRefs.length,
      threadReferences: threadRefs.length,
      runtimeReferences: runtimeRefs,
    },
  };
}

/**
 * Check sub-workflow references.
 */
function checkSubgraphReferences(
  workflowRegistry: WorkflowRegistry,
  workflowId: string,
): WorkflowReference[] {
  const references: WorkflowReference[] = [];

  // Check the parent workflow reference (the current workflow is being referenced as a sub-workflow).
  const parentWorkflowId = workflowRegistry.getParentWorkflow(workflowId);
  if (parentWorkflowId) {
    const parentWorkflow = workflowRegistry.get(parentWorkflowId);
    if (parentWorkflow) {
      references.push({
        type: "subgraph",
        sourceId: parentWorkflowId,
        sourceName: parentWorkflow.name,
        isRuntimeReference: false,
        details: {
          relationshipType: "parent-child",
          depth: workflowRegistry.getWorkflowHierarchy(workflowId).depth,
        },
      });
    }
  }

  return references;
}

/**
 * Check the trigger reference.
 */
function checkTriggerReferences(
  workflowRegistry: WorkflowRegistry,
  workflowId: string,
): WorkflowReference[] {
  const references: WorkflowReference[] = [];
  const allWorkflows = workflowRegistry.list();

  for (const summary of allWorkflows) {
    const workflow = workflowRegistry.get(summary.id);
    if (!workflow?.triggers) continue;

    for (const trigger of workflow.triggers) {
      if (isTriggerReferencingWorkflow(trigger, workflowId)) {
        // Securely obtain the id and name properties of the trigger.
        let triggerId: string | undefined;
        let triggerName: string | undefined;

        if (isWorkflowTrigger(trigger)) {
          triggerId = trigger.id;
          triggerName = trigger.name;
        } else if (isTriggerReference(trigger)) {
          triggerId = trigger.triggerId;
          triggerName = trigger.triggerName;
        }

        references.push({
          type: "trigger",
          sourceId: `${workflow.id}:${triggerId || "unnamed-trigger"}`,
          sourceName: `${workflow.name} - ${triggerName || "Unnamed Trigger"}`,
          isRuntimeReference: false,
          details: {
            workflowId: workflow.id,
            triggerId: triggerId,
            triggerType: "START_WORKFLOW",
          },
        });
      }
    }
  }

  return references;
}

/**
 * Check runtime thread references.
 */
function checkThreadReferences(
  workflowExecutionRegistry: WorkflowExecutionRegistry,
  workflowId: string,
): WorkflowReference[] {
  const references: WorkflowReference[] = [];

  // Quick Check: Use the active workflow set for quick filtering.
  if (!threadRegistry.isWorkflowActive(workflowId)) {
    return references;
  }

  // Detailed inspection: Only perform a thorough traversal on active workflows.
  const allThreads = threadRegistry.getAll();

  for (const threadEntity of allThreads) {
    // Check whether the main thread is using this workflow.
    if (threadEntity.getWorkflowId() === workflowId) {
      references.push(createMainWorkflowReference(threadEntity));
    }

    // Check the context of the triggered sub-workflow.
    const triggeredSubworkflowId = threadEntity.getTriggeredSubworkflowId();
    if (triggeredSubworkflowId === workflowId) {
      references.push(createTriggeredSubworkflowReference(threadEntity));
    }

    // Check subgraph execution stack references (new feature).
    const subgraphStack = threadEntity.getSubgraphStack();
    for (const context of subgraphStack) {
      if (context.workflowId === workflowId) {
        references.push(createSubgraphStackReference(threadEntity, context));
      }
    }
  }

  return references;
}

/**
 * Create a main workflow reference
 */
function createMainWorkflowReference(threadEntity: WorkflowExecutionEntity): WorkflowReference {
  return {
    type: "thread",
    sourceId: threadEntity.id,
    sourceName: `Thread ${threadEntity.id}`,
    isRuntimeReference: true,
    details: {
      threadStatus: threadEntity.getStatus(),
      threadType: threadEntity.getThreadType?.() ?? "main",
      referenceType: "main-workflow",
    },
  };
}

/**
 * Create a reference to the triggered sub-workflow.
 */
function createTriggeredSubworkflowReference(threadEntity: WorkflowExecutionEntity): WorkflowReference {
  return {
    type: "thread",
    sourceId: threadEntity.id,
    sourceName: `Thread ${threadEntity.id} (Triggered Subworkflow)`,
    isRuntimeReference: true,
    details: {
      threadStatus: threadEntity.getStatus(),
      threadType: threadEntity.getThreadType?.() ?? "triggered",
      contextType: "triggered-subworkflow",
    },
  };
}

/**
 * Create a reference to the subgraph execution stack
 */
function createSubgraphStackReference(
  threadEntity: WorkflowExecutionEntity,
  context: { depth?: number; parentWorkflowId?: string },
): WorkflowReference {
  return {
    type: "thread",
    sourceId: threadEntity.id,
    sourceName: `Thread ${threadEntity.id} (Subgraph Stack)`,
    isRuntimeReference: true,
    details: {
      threadStatus: threadEntity.getStatus(),
      threadType: threadEntity.getThreadType?.() ?? "subgraph",
      contextType: "subgraph-stack",
      depth: context.depth,
      parentWorkflowId: context.parentWorkflowId,
    },
  };
}

/**
 * Determine whether the trigger references a specified workflow.
 */
function isTriggerReferencingWorkflow(
  trigger: WorkflowTrigger | TriggerReference,
  targetWorkflowId: string,
): boolean {
  // Handling the WorkflowTrigger type
  if (isWorkflowTrigger(trigger)) {
    // Process ExecuteTriggeredSubgraphActionConfig
    if (trigger.action?.type === "execute_triggered_subgraph") {
      const triggeredWorkflowId = trigger.action.parameters?.["triggeredWorkflowId"];
      return triggeredWorkflowId === targetWorkflowId;
    }
  }

  // Handling the TriggerReference type
  if (isTriggerReference(trigger)) {
    if (trigger.configOverride?.action?.type === "execute_triggered_subgraph") {
      const triggeredWorkflowId = trigger.configOverride.action.parameters?.["triggeredWorkflowId"];
      return triggeredWorkflowId === targetWorkflowId;
    }
  }

  return false;
}

/**
 * Type Guard: Checking if it is a WorkflowTrigger
 */
function isWorkflowTrigger(
  trigger: WorkflowTrigger | TriggerReference,
): trigger is WorkflowTrigger {
  return "action" in trigger;
}

/**
 * Type Guard: Check if it is a TriggerReference
 */
function isTriggerReference(
  trigger: WorkflowTrigger | TriggerReference,
): trigger is TriggerReference {
  return "templateName" in trigger;
}
