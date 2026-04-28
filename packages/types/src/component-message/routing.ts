/**
 * Routing Types
 *
 * Defines routing rules and decision logic for the component message system.
 * Note: Default routing rules and matching logic are implemented at the application layer.
 */

import type { BaseComponentMessage, MessageLevel } from "./base.js";
import { MessageCategory } from "./base.js";
import type { EntityType } from "./entity.js";
import type { OutputDecision } from "./output.js";

/**
 * Routing Rule Match Condition
 */
export interface RoutingMatchCondition {
  /** Match by message categories */
  categories?: MessageCategory[];

  /** Match by message types */
  types?: string[];

  /** Match by message levels */
  levels?: MessageLevel[];

  /** Match by entity types */
  entityTypes?: EntityType[];

  /** Custom match function */
  custom?: (message: BaseComponentMessage) => boolean;
}

/**
 * Routing Rule
 * Defines how messages matching certain conditions should be routed.
 */
export interface RoutingRule {
  /** Rule name (for debugging and configuration) */
  name: string;

  /** Match condition */
  match: RoutingMatchCondition;

  /** Output decision for matching messages */
  decision: OutputDecision;

  /** Priority (lower number = higher priority) */
  priority: number;
}
