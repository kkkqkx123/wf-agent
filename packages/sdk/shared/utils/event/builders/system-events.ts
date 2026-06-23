/**
 * System Event Builders
 * Provides builders for system-related events
 */

import { createBuilder } from "./common.js";
import type { VariableChangedEvent } from "@wf-agent/types";

// =============================================================================
// Variable Events
// =============================================================================

/**
 * Build variable changed event
 */
export const buildVariableChangedEvent = createBuilder<VariableChangedEvent>("VARIABLE_CHANGED");
