/**
 * Thread Operation Coordinator
 * Responsible for coordinating thread structure operations (Fork/Join/Copy)
 *
 * Key Responsibilities:
 * 1. Coordinate Fork operations - Create child threads
 * 2. Coordinate Join operations - Merge results of child threads
 * 3. Coordinate Copy operations - Create thread copies
 * 4. Trigger relevant events
 *
 * Design Principles:
 * - Stateless design: Does not hold any instance variables
 * - Dependency injection: Receives dependencies through the constructor
 * - Specifically handles thread structure modification operations
 */

import type { ForkConfig, JoinResult } from "../utils/thread-operations.js";
import { type ThreadRegistry } from "../../stores/thread-registry.js";
import { ThreadBuilder } from "../factories/thread-builder.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import { ThreadContextNotFoundError } from "@wf-agent/types";
import { fork, join, copy } from "../utils/thread-operations.js";

/**
 * Thread Operation Coordinator Class
 *
 * Responsibilities:
 * - Coordinate the structural operations of threads (Fork/Join/Copy)
 * - Manage the relationships between threads
 * - Trigger relevant events
 *
 * Design Principles:
 * - Stateless design: Does not hold any instance variables
 * - Dependency injection: Receives dependencies through the constructor
 * - Specifically handles thread structure modification operations
 */
export class ThreadOperationCoordinator {
  private threadRegistry: ThreadRegistry;
  private eventManager: EventRegistry;

  constructor(threadRegistry: ThreadRegistry, eventManager: EventRegistry) {
    this.threadRegistry = threadRegistry;
    this.eventManager = eventManager;
  }

  /**
   * Fork Operation - Creating Child Threads
   *
   * @param parentThreadId: Parent thread ID
   * @param forkConfig: Fork configuration
   * @returns: Array of child thread IDs
   */
  async fork(parentThreadId: string, forkConfig: ForkConfig): Promise<string[]> {
    // Step 1: Obtain the parent thread entity
    const parentThreadEntity = this.threadRegistry.get(parentThreadId);
    if (!parentThreadEntity) {
      throw new ThreadContextNotFoundError(
        `Parent thread not found: ${parentThreadId}`,
        parentThreadId,
      );
    }

    // Step 2: Create a child thread using ThreadOperations (event triggering is handled internally).
    const threadBuilder = new ThreadBuilder();
    const childThreadEntity = await fork(
      parentThreadEntity,
      forkConfig,
      threadBuilder,
      this.eventManager,
    );

    // Step 3: Registering a sub-thread
    this.threadRegistry.register(childThreadEntity);

    // Step 4: Return the array of sub-thread IDs
    return [childThreadEntity.id];
  }

  /**
   * Join Operation - Merging the results of child threads
   *
   * @param parentThreadId: Parent thread ID
   * @param childThreadIds: Array of child thread IDs
   * @param joinStrategy: Join strategy
   * @param timeout: Timeout period (in seconds)
   * @param mainPathId: Main thread path ID (optional)
   * @returns: Results of the join operation
   */
  async join(
    parentThreadId: string,
    childThreadIds: string[],
    joinStrategy:
      | "ALL_COMPLETED"
      | "ANY_COMPLETED"
      | "ALL_FAILED"
      | "ANY_FAILED"
      | "SUCCESS_COUNT_THRESHOLD" = "ALL_COMPLETED",
    timeout: number = 60,
    mainPathId: string,
  ): Promise<JoinResult> {
    // Step 1: Obtain the parent thread entity
    const parentThreadEntity = this.threadRegistry.get(parentThreadId);
    if (!parentThreadEntity) {
      throw new ThreadContextNotFoundError(
        `Parent thread not found: ${parentThreadId}`,
        parentThreadId,
      );
    }

    // Step 2: Use ThreadOperations to perform the Join operation (the event triggering is handled internally).
    const joinResult = await join(
      childThreadIds,
      joinStrategy,
      this.threadRegistry,
      mainPathId,
      timeout, // Note: The `timeout` in `thread-operations.ts` is already in seconds.
      parentThreadId,
      this.eventManager,
    );

    // Step 3: Return the Join result
    return joinResult;
  }

  /**
   * Copy Operation - Creates a copy of a Thread
   *
   * @param sourceThreadId: Source thread ID
   * @returns: Copy thread ID
   */
  async copy(sourceThreadId: string): Promise<string> {
    // Step 1: Obtain the source thread entity
    const sourceThreadEntity = this.threadRegistry.get(sourceThreadId);
    if (!sourceThreadEntity) {
      throw new ThreadContextNotFoundError(
        `Source thread not found: ${sourceThreadId}`,
        sourceThreadId,
      );
    }

    // Step 2: Use ThreadOperations to create a copy (the event is processed internally).
    const threadBuilder = new ThreadBuilder();
    const copiedThreadEntity = await copy(sourceThreadEntity, threadBuilder, this.eventManager);

    // Step 3: Register the copy thread
    this.threadRegistry.register(copiedThreadEntity);

    // Step 4: Return the copy thread ID
    return copiedThreadEntity.id;
  }
}
