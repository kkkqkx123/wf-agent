import type { SystemPromptFragment } from "@wf-agent/types";

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

  clear(): void {
    this.fragments.clear();
  }
}
