/**
 * Whiteout Cache — Trie-based whiteout tracking
 *
 * Architecture reference: docs/infra/sandbox/strategies/vfs-overlay.md
 *
 * Performance: O(depth) lookup with ancestor whiteout propagation.
 * A whiteout on a directory hides all descendants.
 */

interface WhiteoutNode {
  children: Map<string, WhiteoutNode>;
  isWhiteout: boolean;
}

export class WhiteoutCache {
  private root: WhiteoutNode = { children: new Map(), isWhiteout: false };

  /**
   * Check if a path is whiteouted (or has a whiteouted ancestor).
   */
  hasWhiteout(path: string): boolean {
    let node = this.root;
    const components = path.replace(/\\/g, "/").split("/").filter(Boolean);

    for (const component of components) {
      if (node.isWhiteout) return true;
      const child = node.children.get(component);
      if (!child) return false;
      node = child;
    }

    return node.isWhiteout;
  }

  /**
   * Mark a path as whiteouted.
   */
  markWhiteout(path: string): void {
    let node = this.root;
    const components = path.replace(/\\/g, "/").split("/").filter(Boolean);

    for (const component of components) {
      let child = node.children.get(component);
      if (!child) {
        child = { children: new Map(), isWhiteout: false };
        node.children.set(component, child);
      }
      node = child;
    }

    node.isWhiteout = true;
  }

  /**
   * Remove a whiteout mark from a path.
   */
  clearWhiteout(path: string): void {
    let node = this.root;
    const components = path.replace(/\\/g, "/").split("/").filter(Boolean);

    for (const component of components) {
      const child = node.children.get(component);
      if (!child) return;
      node = child;
    }

    node.isWhiteout = false;
  }

  /**
   * Check if the cache has any whiteout entries.
   */
  get hasEntries(): boolean {
    return this.root.children.size > 0;
  }

  /**
   * Clear all whiteouts.
   */
  clear(): void {
    this.root = { children: new Map(), isWhiteout: false };
  }

  /**
   * Get the total number of whiteout entries.
   */
  get size(): number {
    return this.countWhiteouts(this.root);
  }

  private countWhiteouts(node: WhiteoutNode): number {
    let count = node.isWhiteout ? 1 : 0;
    for (const child of node.children.values()) {
      count += this.countWhiteouts(child);
    }
    return count;
  }
}