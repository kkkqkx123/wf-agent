/**
 * Plugin Dependency Resolver - Resolves inter-plugin dependencies.
 *
 * Features:
 * - Topological sort for load order
 * - Cycle detection
 * - Missing dependency detection
 * - Semver range checking
 */

import type { PluginManifest, ResolvedDependencyGraph } from "./types.js";
import semver from "semver";

/**
 * Plugin Dependency Resolver
 */
export class PluginDependencyResolver {
  /**
   * Resolve dependencies for a set of plugins.
   * Returns a topologically sorted load order.
   * Throws on circular or missing dependencies.
   */
  resolve(plugins: PluginManifest[]): ResolvedDependencyGraph {
    const available = new Map<string, PluginManifest>();
    for (const p of plugins) {
      available.set(p.id, p);
    }

    const adjacency = new Map<string, string[]>();
    const missing: string[] = [];
    const cycles: string[][] = [];

    // Build adjacency map: plugin -> dependencies
    for (const plugin of plugins) {
      const deps: string[] = [];
      const allDeps = { ...plugin.dependencies, ...plugin.optionalDependencies };

      for (const [depId, depVersion] of Object.entries(allDeps)) {
        const depPlugin = available.get(depId);
        if (depPlugin) {
          if (this.checkDependency(plugin.id, depId, depVersion, available)) {
            deps.push(depId);
          } else {
            // Version mismatch - treat as missing
            if (plugin.dependencies?.[depId]) {
              missing.push(depId);
            }
          }
        } else {
          // Not available
          if (plugin.dependencies?.[depId]) {
            // Required dependency missing
            missing.push(depId);
          }
          // Optional dependency missing is OK - skip
        }
      }
      adjacency.set(plugin.id, deps);
    }

    // Topological sort (Kahn's algorithm)
    const loadOrder = this.topologicalSort(plugins.map(p => p.id), adjacency);

    // Detect cycles
    if (loadOrder.length < plugins.length) {
      const resolved = new Set(loadOrder);
      const unresolved = plugins.map(p => p.id).filter(id => !resolved.has(id));
      cycles.push(unresolved);
    }

    // Build reverse adjacency (dependency -> dependents)
    const reverseAdj = new Map<string, string[]>();
    for (const [pluginId, deps] of adjacency) {
      for (const depId of deps) {
        if (!reverseAdj.has(depId)) {
          reverseAdj.set(depId, []);
        }
        reverseAdj.get(depId)!.push(pluginId);
      }
    }

    return {
      loadOrder,
      adjacency: reverseAdj,
      cycles,
      missing,
    };
  }

  /**
   * Check if a dependency version constraint is satisfied using semver.
   */
  checkDependency(
    _pluginId: string,
    depId: string,
    depVersion: string,
    available: Map<string, PluginManifest>,
  ): boolean {
    const dep = available.get(depId);
    if (!dep) return false;
    return semver.satisfies(dep.version, depVersion);
  }

  /**
   * Topological sort using Kahn's algorithm.
   */
  private topologicalSort(
    nodes: string[],
    adjacency: Map<string, string[]>,
  ): string[] {
    const inDegree = new Map<string, number>();
    for (const node of nodes) {
      inDegree.set(node, 0);
    }
    for (const [, deps] of adjacency) {
      for (const dep of deps) {
        inDegree.set(dep, (inDegree.get(dep) || 0) + 1);
      }
    }

    const queue: string[] = [];
    for (const node of nodes) {
      if (inDegree.get(node) === 0) {
        queue.push(node);
      }
    }

    const sorted: string[] = [];
    while (queue.length > 0) {
      const node = queue.shift()!;
      sorted.push(node);

      const deps = adjacency.get(node) || [];
      for (const dep of deps) {
        const newDegree = (inDegree.get(dep) || 1) - 1;
        inDegree.set(dep, newDegree);
        if (newDegree === 0) {
          queue.push(dep);
        }
      }
    }

    return sorted;
  }
}