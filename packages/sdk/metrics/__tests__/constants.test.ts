import { describe, it, expect } from "vitest";
import {
  WORKFLOW_METRICS,
  NODE_METRICS,
  TOOL_METRICS,
  TOKEN_METRICS,
  ERROR_METRICS,
  RESOURCE_METRICS,
  AGENT_LOOP_METRICS,
  TEMPLATE_METRICS,
  CONFIG_METRICS,
  SUBGRAPH_METRICS,
} from "../constants.js";

describe("Metric Constants", () => {
  describe("WORKFLOW_METRICS", () => {
    it("should have all workflow metric names", () => {
      expect(WORKFLOW_METRICS.EXECUTION_COUNT).toBe("workflow.execution.count");
      expect(WORKFLOW_METRICS.EXECUTION_DURATION).toBe("workflow.execution.duration");
      expect(WORKFLOW_METRICS.NODE_COUNT).toBe("workflow.node.count");
      expect(WORKFLOW_METRICS.SUCCESS_COUNT).toBe("workflow.execution.success.count");
      expect(WORKFLOW_METRICS.FAILURE_COUNT).toBe("workflow.execution.failure.count");
      expect(WORKFLOW_METRICS.ACTIVE_COUNT).toBe("workflow.execution.active.count");
      expect(WORKFLOW_METRICS.ERROR_COUNT).toBe("workflow.error.count");
    });
  });

  describe("NODE_METRICS", () => {
    it("should have all node metric names", () => {
      expect(NODE_METRICS.EXECUTION_COUNT).toBe("node.execution.count");
      expect(NODE_METRICS.EXECUTION_DURATION).toBe("node.execution.duration");
      expect(NODE_METRICS.SUCCESS_COUNT).toBe("node.execution.success.count");
      expect(NODE_METRICS.FAILURE_COUNT).toBe("node.execution.failure.count");
      expect(NODE_METRICS.STARTED_COUNT).toBe("node.execution.started.count");
      expect(NODE_METRICS.RETRY_COUNT).toBe("node.retry.count");
      expect(NODE_METRICS.ERROR_COUNT).toBe("node.error.count");
      expect(NODE_METRICS.INPUT_SIZE).toBe("node.input.size");
      expect(NODE_METRICS.OUTPUT_SIZE).toBe("node.output.size");
      expect(NODE_METRICS.TOKEN_USAGE).toBe("node.execution.token_usage");
    });
  });

  describe("TOOL_METRICS", () => {
    it("should have all tool metric names", () => {
      expect(TOOL_METRICS.CALL_DURATION).toBe("tool.call.duration");
      expect(TOOL_METRICS.CALL_COUNT).toBe("tool.call.count");
      expect(TOOL_METRICS.ERROR_COUNT).toBe("tool.error.count");
      expect(TOOL_METRICS.PARAMETER_SIZE).toBe("tool.parameter.size");
      expect(TOOL_METRICS.RESULT_SIZE).toBe("tool.result.size");
    });
  });

  describe("TOKEN_METRICS", () => {
    it("should have all token metric names", () => {
      expect(TOKEN_METRICS.TOTAL_TOKENS).toBe("token.usage.total");
      expect(TOKEN_METRICS.PROMPT_TOKENS).toBe("token.usage.prompt");
      expect(TOKEN_METRICS.COMPLETION_TOKENS).toBe("token.usage.completion");
      expect(TOKEN_METRICS.COST).toBe("token.cost.total");
      expect(TOKEN_METRICS.REQUEST_COUNT).toBe("token.request.count");
    });
  });

  describe("ERROR_METRICS", () => {
    it("should have all error metric names", () => {
      expect(ERROR_METRICS.OCCURRENCE_COUNT).toBe("error.occurrence.count");
      expect(ERROR_METRICS.RECOVERY_RATE).toBe("error.recovery.rate");
      expect(ERROR_METRICS.AFFECTED_EXECUTIONS).toBe("error.affected.executions");
    });
  });

  describe("RESOURCE_METRICS", () => {
    it("should have all resource metric names", () => {
      expect(RESOURCE_METRICS.MEMORY_USAGE).toBe("resource.memory.usage");
      expect(RESOURCE_METRICS.ACTIVE_EXECUTIONS).toBe("resource.active.executions");
      expect(RESOURCE_METRICS.QUEUED_TASKS).toBe("resource.queued.tasks");
      expect(RESOURCE_METRICS.EVENT_QUEUE_LENGTH).toBe("resource.event.queue.length");
    });
  });

  describe("AGENT_LOOP_METRICS", () => {
    it("should have all agent loop metric names", () => {
      expect(AGENT_LOOP_METRICS.EXECUTION_DURATION).toBe("agent_loop.execution.duration");
      expect(AGENT_LOOP_METRICS.EXECUTION_COUNT).toBe("agent_loop.execution.count");
      expect(AGENT_LOOP_METRICS.ACTIVE_COUNT).toBe("agent_loop.active.count");
      expect(AGENT_LOOP_METRICS.ITERATION_COUNT).toBe("agent_loop.iteration.count");
      expect(AGENT_LOOP_METRICS.ITERATION_DURATION).toBe("agent_loop.iteration.duration");
      expect(AGENT_LOOP_METRICS.MAX_ITERATIONS_REACHED).toBe("agent_loop.iteration.limit_reached");
      expect(AGENT_LOOP_METRICS.TOOL_CALLS_TOTAL).toBe("agent_loop.tool_calls.total");
      expect(AGENT_LOOP_METRICS.TOOL_CALLS_PER_ITERATION).toBe(
        "agent_loop.tool_calls.per_iteration",
      );
      expect(AGENT_LOOP_METRICS.PAUSE_COUNT).toBe("agent_loop.pause.count");
      expect(AGENT_LOOP_METRICS.RESUME_COUNT).toBe("agent_loop.resume.count");
      expect(AGENT_LOOP_METRICS.PAUSE_DURATION).toBe("agent_loop.pause.duration");
      expect(AGENT_LOOP_METRICS.SUCCESS_RATE).toBe("agent_loop.success.rate");
      expect(AGENT_LOOP_METRICS.ERROR_COUNT).toBe("agent_loop.error.count");
    });
  });

  describe("TEMPLATE_METRICS", () => {
    it("should have all template metric names", () => {
      expect(TEMPLATE_METRICS.INSTANTIATION_COUNT).toBe("node.template.instantiation.count");
      expect(TEMPLATE_METRICS.RENDER_DURATION).toBe("template.render.duration");
      expect(TEMPLATE_METRICS.CACHE_HIT_COUNT).toBe("template.cache.hit_count");
      expect(TEMPLATE_METRICS.CACHE_MISS_COUNT).toBe("template.cache.miss_count");
      expect(TEMPLATE_METRICS.ERROR_COUNT).toBe("template.error.count");
    });
  });

  describe("CONFIG_METRICS", () => {
    it("should have all config metric names", () => {
      expect(CONFIG_METRICS.ACCESS_COUNT).toBe("config.access.count");
      expect(CONFIG_METRICS.LOAD_DURATION).toBe("config.load.duration");
      expect(CONFIG_METRICS.VALIDATION_ERROR_COUNT).toBe("config.validation_error.count");
      expect(CONFIG_METRICS.CACHE_HIT_COUNT).toBe("config.cache.hit_count");
      expect(CONFIG_METRICS.CACHE_MISS_COUNT).toBe("config.cache.miss_count");
    });
  });

  describe("SUBGRAPH_METRICS", () => {
    it("should have all subgraph metric names", () => {
      expect(SUBGRAPH_METRICS.EXECUTION_COUNT).toBe("subgraph.execution.count");
      expect(SUBGRAPH_METRICS.EXECUTION_DURATION).toBe("subgraph.execution.duration");
      expect(SUBGRAPH_METRICS.SUCCESS_COUNT).toBe("subgraph.execution.success.count");
      expect(SUBGRAPH_METRICS.FAILURE_COUNT).toBe("subgraph.execution.failure.count");
      expect(SUBGRAPH_METRICS.NESTED_DEPTH).toBe("subgraph.nested.depth");
      expect(SUBGRAPH_METRICS.VARIABLE_IMPORT_COUNT).toBe("subgraph.variable.import.count");
      expect(SUBGRAPH_METRICS.VARIABLE_EXPORT_COUNT).toBe("subgraph.variable.export.count");
      expect(SUBGRAPH_METRICS.VARIABLE_IMPORT_DURATION).toBe("subgraph.variable.import.duration");
      expect(SUBGRAPH_METRICS.VARIABLE_EXPORT_DURATION).toBe("subgraph.variable.export.duration");
    });
  });
});
