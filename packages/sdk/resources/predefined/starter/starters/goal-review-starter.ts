import type {
  WorkflowTemplate,
  PromptTemplate,
  VariableDefinition,
  Condition,
  StaticNode,
  Edge,
  LLMMessage,
} from "@wf-agent/types";
import type { StarterMetadata, WorkflowBundle } from "../types.js";
import { BaseStarter } from "../base-starter.js";
import { executorTemplate, reviewerTemplate } from "../../agent-templates/index.js";

export interface GoalReviewConfig extends Record<string, unknown> {
  rootRequirement: string;
  targetPath?: string;
  maxIterations: number;
  plannerProfileId: string;
  executorProfileId?: string;
  reviewerProfileId?: string;
  plannerSystemPrompt?: string;
  executorSystemPrompt?: string;
  reviewerSystemPrompt?: string;
  executorTools?: string[];
  reviewerTools?: string[];
  executorMaxIterations?: number;
  reviewerMaxIterations?: number;
  initialMessages?: LLMMessage[];
}

const defaultPlannerPrompt = `You are a task planner for a goal-driven review loop.
Read the root requirement, the conversation history, and the unresolved review defects.
Output a single clear task description for the executor to work on next.`;

const defaultInitialMessages: LLMMessage[] = [
  { role: "system", content: defaultPlannerPrompt },
];

export class GoalReviewStarter extends BaseStarter<GoalReviewConfig> {
  readonly metadata: StarterMetadata = {
    id: "@standard/goal-review-agent",
    name: "Goal Review Agent",
    version: "1.0.0",
    description: "Goal-driven review loop with planner, executor, and reviewer agents",
    tags: ["review", "goal-driven", "agent-loop"],
    category: "code-review",
    configurable: {
      maxIterations: {
        type: "number",
        default: 10,
        description: "Maximum review loop iterations",
      },
      plannerProfileId: {
        type: "string",
        default: "gpt-4o-mini",
        description: "LLM profile for task planning (lightweight model)",
      },
      executorProfileId: {
        type: "string",
        description: "LLM profile for executor (default from template)",
      },
      reviewerProfileId: {
        type: "string",
        description: "LLM profile for reviewer (default from template)",
      },
      plannerSystemPrompt: {
        type: "string",
        description: "Custom system prompt for the task planner",
      },
      executorSystemPrompt: {
        type: "string",
        description: "Override system prompt for the executor agent",
      },
      reviewerSystemPrompt: {
        type: "string",
        description: "Override system prompt for the reviewer agent",
      },
      executorTools: {
        type: "array",
        description: "Override tools for the executor agent",
      },
      reviewerTools: {
        type: "array",
        description: "Override tools for the reviewer agent (read-only recommended)",
      },
    },
  };

  assemble(config: GoalReviewConfig): WorkflowBundle {
    return {
      workflow: this.buildWorkflow(config),
      promptTemplates: [
        this.buildPlannerPrompt(config),
      ],
    };
  }

  private buildWorkflow(config: GoalReviewConfig): WorkflowTemplate {
    const variables: VariableDefinition[] = [
      {
        name: "rootRequirement",
        type: "string",
        value: config.rootRequirement,
        readonly: true,
        metadata: { description: "Original goal, injected into planner and reviewer each iteration" },
      },
      {
        name: "status",
        type: "string",
        value: "planning",
        readonly: false,
        metadata: { description: "Current loop status: planning | executing | reviewing | completed | stuck" },
      },
      {
        name: "complete",
        type: "boolean",
        value: false,
        readonly: false,
        metadata: { description: "Loop exit flag, set by reviewer agent" },
      },
      {
        name: "judges",
        type: "array",
        value: [],
        readonly: false,
        metadata: { description: "Review judgment records, appended each iteration" },
      },
      {
        name: "iterationCount",
        type: "number",
        value: 0,
        readonly: false,
        metadata: { description: "Current iteration counter" },
      },
    ];

    const executorInline = this.buildExecutorInlineConfig(config);
    const reviewerInline = this.buildReviewerInlineConfig(config);

    const nodes: StaticNode[] = [
      {
        id: "start",
        type: "START",
        name: "Start",
        config: {
          messageInputs: [
            {
              sourceContextId: "initial",
              internalName: "default",
              required: true,
              defaultMessages: config.initialMessages ?? defaultInitialMessages,
            },
          ],
          dataInputs: [
            {
              parentField: "rootRequirement",
              internalName: "rootRequirement",
              required: true,
            },
            {
              parentField: "targetPath",
              internalName: "targetPath",
              required: false,
            },
          ],
        },
      },
      {
        id: "loop_start",
        type: "LOOP_START",
        name: "Review Loop",
        config: {
          loopId: "review-loop",
          maxIterations: config.maxIterations,
          variableInputs: [
            { sourcePath: "status", internalName: "status", required: true },
            { sourcePath: "complete", internalName: "complete", required: true },
            { sourcePath: "judges", internalName: "judges", required: true },
            { sourcePath: "rootRequirement", internalName: "rootRequirement", required: true },
            { sourcePath: "iterationCount", internalName: "iterationCount", required: false, defaultValue: 0 },
          ],
        },
      },
      {
        id: "task_planner",
        type: "LLM",
        name: "Task Planner",
        config: {
          profileId: config.plannerProfileId,
          contextId: "default",
        },
      },
      {
        id: "executor_agent",
        type: "AGENT_LOOP",
        name: "Executor Agent",
        config: {
          agentLoopId: executorTemplate.id,
          inlineConfig: {
            ...executorInline,
            messageInputs: [
              { sourceContextId: "default", internalName: "system-context" },
            ],
            messageOutputs: [
              { internalName: "system-context", targetContextId: "default" },
            ],
          },
        },
      },
      {
        id: "reviewer_agent",
        type: "AGENT_LOOP",
        name: "Reviewer Agent",
        config: {
          agentLoopId: reviewerTemplate.id,
          inlineConfig: {
            ...reviewerInline,
            dataInputs: [
              { parentField: "judges", internalName: "previous_judges" },
            ],
            messageInputs: [
              { sourceContextId: "default", internalName: "review-context" },
            ],
            messageOutputs: [
              { internalName: "review-context", targetContextId: "default" },
            ],
          },
        },
      },
      {
        id: "loop_end",
        type: "LOOP_END",
        name: "Loop End Check",
        config: {
          loopId: "review-loop",
          breakCondition: {
            type: "expression",
            expression: "status === \"completed\" || status === \"stuck\"",
          } satisfies Condition,
          loopStartNodeId: "task_planner",
        },
      },
      {
        id: "end",
        type: "END",
        name: "End",
        config: {
          dataOutputs: [
            { internalName: "judges", outputKey: "judges" },
            { internalName: "status", outputKey: "status" },
            { internalName: "complete", outputKey: "complete" },
          ],
        },
      },
    ];

    const edges: Edge[] = [
      { id: "e0", sourceNodeId: "start", targetNodeId: "loop_start", type: "DEFAULT" },
      { id: "e2", sourceNodeId: "loop_start", targetNodeId: "task_planner", type: "DEFAULT" },
      { id: "e3", sourceNodeId: "task_planner", targetNodeId: "executor_agent", type: "DEFAULT" },
      { id: "e4", sourceNodeId: "executor_agent", targetNodeId: "reviewer_agent", type: "DEFAULT" },
      { id: "e5", sourceNodeId: "reviewer_agent", targetNodeId: "loop_end", type: "DEFAULT" },
      { id: "e6", sourceNodeId: "loop_end", targetNodeId: "end", type: "DEFAULT" },
      {
        id: "e7", sourceNodeId: "loop_end", targetNodeId: "task_planner", type: "CONDITIONAL",
        condition: { type: "expression", expression: "nextIteration === true" } satisfies Condition,
      },
    ];

    return {
      id: "@standard/goal-review-agent-workflow",
      name: "Goal Review Agent Workflow",
      type: "STANDALONE",
      version: "1.0.0",
      description: "Goal-driven review loop: planner -> executor -> reviewer -> loop check",
      nodes,
      edges,
      variables,
      config: {
        timeout: 600000,
        enableCheckpoints: true,
      },
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };
  }

  private buildExecutorInlineConfig(config: GoalReviewConfig) {
    return {
      profileId: config.executorProfileId ?? executorTemplate.profileId,
      systemPrompt: config.executorSystemPrompt ?? executorTemplate.systemPrompt,
      maxIterations: config.executorMaxIterations ?? executorTemplate.maxIterations,
      availableTools: {
        tools: config.executorTools ?? executorTemplate.availableTools?.tools ?? [],
      },
    };
  }

  private buildReviewerInlineConfig(config: GoalReviewConfig) {
    return {
      profileId: config.reviewerProfileId ?? reviewerTemplate.profileId,
      systemPrompt: config.reviewerSystemPrompt ?? reviewerTemplate.systemPrompt,
      maxIterations: config.reviewerMaxIterations ?? reviewerTemplate.maxIterations,
      availableTools: {
        tools: config.reviewerTools ?? reviewerTemplate.availableTools?.tools ?? [],
      },
    };
  }

  private buildPlannerPrompt(config: GoalReviewConfig): PromptTemplate {
    return {
      id: "@standard/goal-review-planner",
      name: "Goal Review Planner Prompt",
      description: "System prompt for the task planner LLM node",
      category: "system",
      content: config.plannerSystemPrompt ?? defaultPlannerPrompt,
    };
  }
}