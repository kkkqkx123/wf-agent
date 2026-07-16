/**
 * Middleware Registry - Internal registry for plugin-contributed execution middleware.
 *
 * @internal - Used internally by ContributionManager.
 */

import type { ExecutionMiddleware, MiddlewarePhase } from "../middleware.types.js";

interface MiddlewareItem {
  pluginId: string;
  mw: ExecutionMiddleware;
}

export class MiddlewareRegistry {
  private entries = new Map<MiddlewarePhase, MiddlewareItem[]>();

  register(phase: MiddlewarePhase, mw: ExecutionMiddleware): void {
    if (!this.entries.has(phase)) {
      this.entries.set(phase, []);
    }
    this.entries.get(phase)!.push({ pluginId: '', mw });
    // Sort by priority (lower number = higher priority)
    this.entries.get(phase)!.sort((a, b) => (a.mw.priority ?? 0) - (b.mw.priority ?? 0));
  }

  getMiddleware(phase: MiddlewarePhase): ExecutionMiddleware[] {
    return this.entries.get(phase)?.map(item => item.mw) ?? [];
  }

  hasMiddleware(phase: MiddlewarePhase): boolean {
    const items = this.entries.get(phase);
    return items !== undefined && items.length > 0;
  }

  async runMiddleware(phase: MiddlewarePhase, context: Record<string, unknown>): Promise<void> {
    const items = this.entries.get(phase);
    if (!items || items.length === 0) return;

    let index = 0;
    const next = async (): Promise<void> => {
      if (index < items.length) {
        const item = items[index++]!;
        await item.mw.handler(context, next);
      }
    };
    await next();
  }

  unregisterByPluginId(pluginId: string): void {
    const phasesToDelete: MiddlewarePhase[] = [];
    for (const [phase, items] of this.entries) {
      const remaining = items.filter(item => item.pluginId !== pluginId);
      if (remaining.length === 0) {
        phasesToDelete.push(phase);
      } else {
        items.length = 0;
        items.push(...remaining);
      }
    }
    for (const phase of phasesToDelete) {
      this.entries.delete(phase);
    }
  }
}