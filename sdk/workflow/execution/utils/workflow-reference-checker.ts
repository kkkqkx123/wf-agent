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

  // Check runtime execution references.
  const executionRefs = checkExecutionReferences(workflowExecutionRegistry, workflowId);
  references.push(...executionRefs);

  const runtimeRefs = references.filter(ref => ref.isRuntimeReference).length;

  return {
    hasReferences: references.length > 0,
    references,
    canSafelyDelete: runtimeRefs === 0,
    stats: {
      subgraphReferences: subgraphRefs.length,
      triggerReferences: triggerRefs.length,
      executionReferences: executionRefs.length,
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
 * Check runtime execution references.
 */
function checkExecutionReferences(
  workflowExecutionRegistry: WorkflowExecutionRegistry,
  workflowId: string,
): WorkflowReference[] {
  const references: WorkflowReference[] = [];

  // Quick Check: Use the active workflow set for quick filtering.
  if (!workflowExecutionRegistry.isWorkflowActive(workflowId)) {
    return references;
  }

  // Detailed inspection: Only perform a thorough traversal on active workflows.
  const allExecutions = workflowExecutionRegistry.getAll();

  for (const executionEntity of allExecutions) {
    // Check whether the main execution is using this workflow.
    if (executionEntity.getWorkflowId() === workflowId) {
      references.push(createMainWorkflowReference(executionEntity));
    }

    // Check the context of the triggered sub-workflow.
    const triggeredSubworkflowId = executionEntity.getTriggeredSubworkflowId();
    if (triggeredSubworkflowId === workflowId) {
      references.push(createTriggeredSubworkflowReference(executionEntity));
    }

    // Check subgraph execution stack references (new feature).
    const subgraphStack = executionEntity.getSubgraphStack();
    for (const context of subgraphStack) {
      if (context.workflowId === workflowId) {
        references.push(createSubgraphStackReference(executionEntity, context));
      }
    }
  }

  return references;
}

/**
 * Create a main workflow reference
 */
function createMainWorkflowReference(executionEntity: WorkflowExecutionEntity): WorkflowReference {
  return {
    type: "workflowExecution",
    sourceId: executionEntity.id,
    sourceName: `Execution ${executionEntity.id}`,
    isRuntimeReference: true,
    details: {
      executionStatus: executionEntity.getStatus(),
      executionType: executionEntity.getExecutionType?.() ?? "main",
      referenceType: "main-workflow",
    },
  };
}

/**
 * Create a reference to the triggered sub-workflow.
 */
function createTriggeredSubworkflowReference(executionEntity: WorkflowExecutionEntity): WorkflowReference {
  return {
    type: "workflowExecution",
    sourceId: executionEntity.id,
    sourceName: `Execution ${executionEntity.id} (Triggered Subworkflow)`,
    isRuntimeReference: true,
    details: {
      executionStatus: executionEntity.getStatus(),
      executionType: executionEntity.getExecutionType?.() ?? "triggered",
      contextType: "triggered-subworkflow",
    },
  };
}

/**
 * Create a reference to the subgraph execution stack
 */
function createSubgraphStackReference(
  executionEntity: WorkflowExecutionEntity,
  context: { depth?: number; parentWorkflowId?: string },
): WorkflowReference {
  return {
    type: "workflowExecution",
    sourceId: executionEntity.id,
    sourceName: `Execution ${executionEntity.id} (Subgraph Stack)`,
    isRuntimeReference: true,
    details: {
      executionStatus: executionEntity.getStatus(),
      executionType: executionEntity.getExecutionType?.() ?? "subgraph",
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
