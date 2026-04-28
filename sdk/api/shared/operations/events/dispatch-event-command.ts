/**
 * DispatchEventCommand - Dispatch Event
 *
 * Note: This is now a shared command as event dispatching is a cross-module concern
 * used by Graph, Agent, and other modules.
 */

import { BaseCommand, CommandValidationResult } from "../../types/command.js";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import type { Event } from "@wf-agent/types";
import { emit } from "../../../../core/utils/event/event-emitter.js";

/**
 * Dispatch event parameters
 */
export interface DispatchEventParams {
  /** Event object */
  event: Event;
}

/**
 * DispatchEventCommand - Dispatch Event
 */
export class DispatchEventCommand extends BaseCommand<void> {
  constructor(
    private readonly params: DispatchEventParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  /**
   * Validate command parameters
   */
  validate(): CommandValidationResult {
    const errors: string[] = [];

    if (!this.params.event) {
      errors.push("Event object cannot be empty");
    } else if (!this.params.event.type) {
      errors.push("Event type cannot be empty");
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  /**
   * Execute command
   */
  protected async executeInternal(): Promise<void> {
    const eventManager = this.dependencies.getEventManager();
    await emit(eventManager, this.params.event);
  }
}
