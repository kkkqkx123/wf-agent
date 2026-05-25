/**
 * Script Flow Engine
 * Executes multi-branch, multi-step script flows
 */

import type { ScriptFlow, Script, ScriptExecutionResult } from "@wf-agent/types";
import { ScriptEngine } from "./script-engine.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ScriptFlowEngine" });

/**
 * Flow execution result
 */
export interface FlowExecutionResult {
  /** Overall success status */
  success: boolean;
  /** Per-branch results */
  branches: Record<string, BranchExecutionResult>;
  /** Total execution time */
  totalExecutionTime: number;
}

/**
 * Per-branch execution result
 */
export interface BranchExecutionResult {
  /** Whether the branch succeeded */
  success: boolean;
  /** Per-module results in the branch */
  modules: ScriptExecutionResult[];
  /** Branch execution time */
  executionTime: number;
}

/**
 * Script Flow Engine
 * Orchestrates multi-branch script execution with dependency ordering
 */
export class ScriptFlowEngine {
  private scriptEngine: ScriptEngine;
  private scripts: Map<string, Script>;

  constructor(scriptEngine: ScriptEngine, scripts: Map<string, Script>) {
    this.scriptEngine = scriptEngine;
    this.scripts = scripts;
  }

  /**
   * Execute a flow blueprint
   * @param flow Flow blueprint definition
   * @returns Flow execution result
   */
  async execute(flow: ScriptFlow): Promise<FlowExecutionResult> {
    const startTime = Date.now();
    const branches: Record<string, BranchExecutionResult> = {};
    const executed = new Set<string>();

    const order = this.topologicalSort(flow);

    for (const branchKey of order) {
      const branch = flow.branches.find((b) => b.key === branchKey);
      if (!branch) continue;

      logger.debug("Executing flow branch", { flow: flow.name, branch: branchKey });
      const branchStart = Date.now();
      const moduleResults: ScriptExecutionResult[] = [];

      for (const moduleRef of branch.modules) {
        const script = this.scripts.get(moduleRef.key);
        if (!script) {
          moduleResults.push({
            success: false,
            scriptName: moduleRef.key,
            executionTime: 0,
            error: `Script '${moduleRef.key}' not found in registry`,
          });
          continue;
        }

        const result = await this.scriptEngine.execute(script, undefined, {
          args: moduleRef.args,
        });
        moduleResults.push(result);
      }

      const allSuccess = moduleResults.every((r) => r.success);
      branches[branchKey] = {
        success: allSuccess,
        modules: moduleResults,
        executionTime: Date.now() - branchStart,
      };
      executed.add(branchKey);
    }

    const allBranchesSuccess = Object.values(branches).every((b) => b.success);

    return {
      success: allBranchesSuccess,
      branches,
      totalExecutionTime: Date.now() - startTime,
    };
  }

  /**
   * Topological sort of branches based on depends_on
   */
  private topologicalSort(flow: ScriptFlow): string[] {
    const visited = new Set<string>();
    const visiting = new Set<string>();
    const order: string[] = [];
    const branchMap = new Map(flow.branches.map((b) => [b.key, b]));

    function visit(key: string): void {
      if (visited.has(key)) return;
      if (visiting.has(key)) {
        throw new Error(`Circular dependency detected in flow '${flow.name}' involving branch '${key}'`);
      }
      visiting.add(key);

      const branch = branchMap.get(key);
      if (branch?.depends_on) {
        for (const dep of branch.depends_on) {
          if (!branchMap.has(dep)) {
            throw new Error(`Branch '${key}' depends on unknown branch '${dep}'`);
          }
          visit(dep);
        }
      }

      visiting.delete(key);
      visited.add(key);
      order.push(key);
    }

    for (const branch of flow.branches) {
      visit(branch.key);
    }

    return order;
  }
}