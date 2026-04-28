/**
 * Clip Assembly Tool
 *
 * Provides the function of combining fragments of system prompt words
 */

/**
 * Segment Definition
 * Base unit for assemblies
 */
export interface SystemPromptFragment {
  /** unique identification of the segment */
  id: string;
  /** Category of clips */
  category: "role" | "capability" | "constraint" | "tool-usage";
  /** Content of the clip */
  content: string;
  /** clip description */
  description?: string;
}

/**
 * Configuration of segment combinations
 */
export interface FragmentCompositionConfig {
  /** List of segment IDs to combine (in order) */
  fragmentIds: string[];
  /** segment separator */
  separator?: string;
  /** Whether to add a prefix at the beginning */
  prefix?: string;
  /** Whether to add a suffix to the end */
  suffix?: string;
}

/**
 * Clip Registry
 * For managing and retrieving clips
 */
export class FragmentRegistry {
  private fragments = new Map<string, SystemPromptFragment>();

  /**
   * Registering individual segments
   */
  register(fragment: SystemPromptFragment): void {
    this.fragments.set(fragment.id, fragment);
  }

  /**
   * Batch Registration Clip
   */
  registerAll(fragments: SystemPromptFragment[]): void {
    for (const fragment of fragments) {
      this.register(fragment);
    }
  }

  /**
   * Get Clip
   */
  get(id: string): SystemPromptFragment | undefined {
    return this.fragments.get(id);
  }

  /**
   * Check if the fragment exists
   */
  has(id: string): boolean {
    return this.fragments.has(id);
  }

  /**
   * Get all clips
   */
  getAll(): SystemPromptFragment[] {
    return Array.from(this.fragments.values());
  }

  /**
   * Get clips by category
   */
  getByCategory(category: SystemPromptFragment["category"]): SystemPromptFragment[] {
    return this.getAll().filter(f => f.category === category);
  }

  /**
   * Empty the registry
   */
  clear(): void {
    this.fragments.clear();
  }
}

/**
 * Example of a default fragment registry
 */
export const fragmentRegistry = new FragmentRegistry();

/**
 * Combined segments for full system prompts
 *
 * @param config Combined configuration
 * @returns The combined system prompt.
 */
export function composeSystemPrompt(config: FragmentCompositionConfig): string {
  const separator = config.separator ?? "\n\n";
  const parts: string[] = [];

  if (config.prefix) {
    parts.push(config.prefix);
  }

  const fragmentContents: string[] = [];
  for (const fragmentId of config.fragmentIds) {
    const fragment = fragmentRegistry.get(fragmentId);
    if (fragment) {
      fragmentContents.push(fragment.content);
    }
    // Silently skip missing fragments
  }

  parts.push(...fragmentContents);

  if (config.suffix) {
    parts.push(config.suffix);
  }

  return parts.join(separator);
}

/**
 * Quick combination of segments
 *
 * @param fragmentIds List of fragment IDs
 * @returns Combined system prompts
 */
export function composeFragments(fragmentIds: string[]): string {
  return composeSystemPrompt({ fragmentIds });
}
