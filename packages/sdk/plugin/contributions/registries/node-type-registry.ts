/**
 * Node Type Registry - Internal registry for plugin-contributed node types.
 *
 * @internal - Used internally by ContributionManager.
 */

import type { NodeHandlerFn } from "../../../workflow/execution/handlers/node-handlers/index.js";
import { BaseContributionRegistry, type ContributionEntry } from "./base-contribution-registry.js";

interface NodeTypeEntry extends ContributionEntry {
  handler: NodeHandlerFn;
  template?: Record<string, unknown>;
}

export class NodeTypeRegistry extends BaseContributionRegistry<NodeTypeEntry> {
  register(pluginId: string, nodeType: string, handler: NodeHandlerFn, template?: Record<string, unknown>): void {
    this.entries.set(nodeType, { pluginId, handler, template });
  }

  getHandler(nodeType: string): NodeHandlerFn | undefined {
    return this.entries.get(nodeType)?.handler;
  }

  hasHandler(nodeType: string): boolean {
    return this.entries.has(nodeType);
  }

  getAllNodeTypes(): string[] {
    return Array.from(this.entries.keys());
  }
}