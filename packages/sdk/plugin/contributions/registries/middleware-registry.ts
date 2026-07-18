/**
 * Middleware Registry - Internal registry for plugin-contributed execution middleware.
 *
 * @internal - Used internally by ContributionManager.
 */

import type { ExecutionMiddleware, MiddlewarePhase } from "../middleware.types.js";
import { BaseContributionRegistry, type ContributionEntry } from "./base-contribution-registry.js";

interface MiddlewareItem extends ContributionEntry {
  mw: ExecutionMiddleware;
}

export class MiddlewareRegistry extends BaseContributionRegistry<MiddlewareItem> {
  /**
   * Middleware entries grouped by phase (maintaining insertion order within each phase).
   * Override the base class to use phase-based grouping.
   */
  private phaseEntries = new Map<MiddlewarePhase, MiddlewareItem[]>();

  register(phase: MiddlewarePhase, mw: ExecutionMiddleware): void {
    if (!this.phaseEntries.has(phase)) {
      this.phaseEntries.set(phase, []);
    }
    this.phaseEntries.get(phase)!.push({ pluginId: '', mw });
    // Sort by priority (lower number = higher priority)
    this.phaseEntries.get(phase)!.sort((a, b) => (a.mw.priority ?? 0) - (b.mw.priority ?? 0));
  }

  getMiddleware(phase: MiddlewarePhase): ExecutionMiddleware[] {
    return this.phaseEntries.get(phase)?.map(item => item.mw) ?? [];
  }

  hasMiddleware(phase: MiddlewarePhase): boolean {
    const items = this.phaseEntries.get(phase);
    return items !== undefined && items.length > 0;
  }

  async runMiddleware(phase: MiddlewarePhase, context: Record<string, unknown>): Promise<void> {
    const items = this.phaseEntries.get(phase);
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

  override unregisterByPluginId(pluginId: string): void {
    const phasesToDelete: MiddlewarePhase[] = [];
    for (const [phase, items] of this.phaseEntries) {
      const remaining = items.filter(item => item.pluginId !== pluginId);
      if (remaining.length === 0) {
        phasesToDelete.push(phase);
      } else {
        items.length = 0;
        items.push(...remaining);
      }
    }
    for (const phase of phasesToDelete) {
      this.phaseEntries.delete(phase);
    }
  }

  override clear(): void {
    this.phaseEntries.clear();
  }
}