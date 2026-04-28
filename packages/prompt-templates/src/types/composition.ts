/**
 * Composite Type Definitions
 * Define template combination and override rules
 */

import type { PromptTemplate } from "./template.js";

/**
 * Template Combination
 * Define how to create new composite templates based on basic templates
 */
export interface TemplateComposition {
  /** Basic template ID */
  baseTemplateId: string;
  /** Covered Fields */
  overrides: Partial<PromptTemplate>;
  /** Segment Replacement Mapping */
  fragmentReplacements?: Record<string, string>;
}
