/**
 * DispatchEventCommand - Dispatch Event Command
 *
 * Category: Management
 * Cross-module event dispatching for Graph, Agent, and other modules
 */

import {
  ManagementCommand,
  type CommandMetadataDefinition,
} from "../../types/command.js";
import { validateEventDispatchParams } from "../validators/shared-validators.js";
import type { CommandValidationResult } from "../../types/command.js";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import type { Event } from "@wf-agent/types";
import { emit } from "../../../../shared/events/emit-event.js";

/**
 * Dispatch event parameters
 */
export interface DispatchEventParams {
  /** Event object to dispatch */
  event: Event;
}

/**
 * DispatchEventCommand - Dispatch Event
 * Sends an event to the event manager for distribution
 */
export class DispatchEventCommand extends ManagementCommand<void> {
  constructor(
    private readonly params: DispatchEventParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "DispatchEventCommand",
      description: "Dispatch an event to the event manager",
      category: "management",
      requiresAuth: false,
      version: "1.0.0",
      idempotent: false,
    };
  }

  /**
   * Validate command parameters using shared validator
   */
  validate(): CommandValidationResult {
    return validateEventDispatchParams(this.params.event);
  }

  /**
   * Execute command - dispatch the event
   */
  protected async executeInternal(): Promise<void> {
    const eventManager = this.dependencies.getEventManager();
    await emit(eventManager, this.params.event);
  }
}
