import type { SystemPromptFragment } from "@wf-agent/types";
import { renderTemplate } from "../../../core/utils/template-renderer/index.js";

export class FragmentRegistry {
  private fragments = new Map<string, SystemPromptFragment>();

  register(fragment: SystemPromptFragment): void {
    this.fragments.set(fragment.id, fragment);
  }

  registerAll(fragments: SystemPromptFragment[]): void {
    for (const fragment of fragments) {
      this.register(fragment);
    }
  }

  get(id: string): SystemPromptFragment | undefined {
    return this.fragments.get(id);
  }

  has(id: string): boolean {
    return this.fragments.has(id);
  }

  getAll(): SystemPromptFragment[] {
    return Array.from(this.fragments.values());
  }

  getByCategory(category: SystemPromptFragment["category"]): SystemPromptFragment[] {
    return this.getAll().filter(f => f.category === category);
  }

  /**
   * Render fragment content with variable substitution.
   *
   * @param id Fragment ID
   * @param variables Variable values to substitute
   * @returns Rendered content string, or undefined if fragment not found
   */
  render(id: string, variables?: Record<string, unknown>): string | undefined {
    const fragment = this.get(id);
    if (!fragment) return undefined;
    if (!variables || !fragment.variables || fragment.variables.length === 0) {
      return fragment.content;
    }
    return renderTemplate(fragment.content, variables);
  }

  /**
   * Batch render multiple fragments with optional variable maps.
   *
   * @param ids Fragment IDs to render
   * @param variablesMap Optional map of fragment ID to variable values
   * @returns Array of rendered content strings (empty strings for missing fragments)
   */
  renderAll(ids: string[], variablesMap?: Map<string, Record<string, unknown>>): string[] {
    return ids.map(id => {
      const vars = variablesMap?.get(id);
      return this.render(id, vars) ?? "";
    });
  }

  clear(): void {
    this.fragments.clear();
  }
}
