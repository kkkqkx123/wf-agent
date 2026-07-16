/**
 * Node Type Registry - Internal registry for plugin-contributed node types.
 *
 * @internal - Used internally by ContributionManager.
 */

import type { NodeHandlerFn } from "../../../workflow/execution/handlers/node-handlers/index.js";

interface NodeTypeEntry {
  pluginId: string;
  handler: NodeHandlerFn;
  template?: Record<string, unknown>;
}

export class NodeTypeRegistry {
  private entries = new Map<string, NodeTypeEntry>();

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

  getOwner(nodeType: string): string | undefined {
    return this.entries.get(nodeType)?.pluginId;
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [nodeType, entry] of this.entries) {
      if (entry.pluginId === pluginId) {
        this.entries.delete(nodeType);
      }
    }
  }
}