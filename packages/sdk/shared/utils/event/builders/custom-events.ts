/**
 * Custom Event Builders
 * Provides builders for custom node events
 */

import { createBuilder } from "./common.js";
import type { NodeCustomEvent } from "@wf-agent/types";

// =============================================================================
// Node Custom Event Builder
// =============================================================================

/**
 * Build node custom event
 */
export const buildNodeCustomEvent = createBuilder<NodeCustomEvent>("NODE_CUSTOM_EVENT");
