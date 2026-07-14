/**
 * LLM Wrapper
 *
 * Provides a unified interface for LLM calls, coordinates Profile management, and client creation
 * Handles request execution and response time statistics
 */

import type { LLMClient, LLMRequest, LLMResult, LLMProfile } from "@wf-agent/types";
import { ProfileManager } from "./profile-manager.js";
import { ClientFactory, MessageStream } from "./index.js";
import {
  tryCatchAsyncWithSignal,
  now,
  diffTimestamp,
  generateId,
  ok,
  err,
} from "@wf-agent/common-utils";
import { ConfigurationError, LLMError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import type { EventRegistry } from "@sdk/shared/registry/event-registry.js";
import { isAbortError } from "@sdk/shared/utils/error-utils.js";

import {
  buildLLMStreamAbortedEvent,
  buildLLMStreamErrorEvent,
} from "@sdk/shared/events/builders/index.js";

/**
 * LLM Wrapper Class
 *
 * A unified entry point for LLM calls, providing a simplified API interface
 * Responsible for coordinating Profile management and client factory, handling request execution
 */
export class LLMWrapper {
  private profileManager: ProfileManager;
  private clientFactory: ClientFactory;
  private eventManager?: EventRegistry;

  constructor(eventManager?: EventRegistry) {
    this.profileManager = new ProfileManager();
    this.clientFactory = new ClientFactory();
    this.eventManager = eventManager;
  }

  /**
   * Set up the event manager
   * @param eventManager  The event manager
   */
  setEventManager(eventManager: EventRegistry): void {
    this.eventManager = eventManager;
  }

  /**
   * Non-stream generation
   *
   * @param request LLM request
   * @returns Result<LLMResult, LLMError>
   */
  async generate(request: LLMRequest): Promise<Result<LLMResult, LLMError>> {
    const profile = this.getProfile(request.profileId);
    if (!profile) {
      return err(
        new LLMError("LLM Profile not found", "unknown", undefined, undefined, undefined, {
          profileId: request.profileId || "DEFAULT",
          availableProfiles: this.profileManager.list().map(p => p.id),
        }),
      );
    }

    const client = this.clientFactory.createClient(profile);
    const startTime = now();

    // Use `tryCatchAsyncWithSignal` to ensure that the signal is passed correctly.
    const result = await tryCatchAsyncWithSignal(
      signal => client.generate({ ...request, signal }),
      request.signal,
    );

    if (result.isErr()) {
      return err(this.convertToLLMError(result.error, profile));
    }

    result.value.duration = diffTimestamp(startTime, now());
    return ok(result.value);
  }

  /**
   * Stream Generation
   *
   * @param request LLM request
   * @returns Result<MessageStream, LLMError>
   */
  async generateStream(request: LLMRequest): Promise<Result<MessageStream, LLMError>> {
    const profile = this.getProfile(request.profileId);
    if (!profile) {
      return err(
        new LLMError("LLM Profile not found", "unknown", undefined, undefined, undefined, {
          profileId: request.profileId || "DEFAULT",
          availableProfiles: this.profileManager.list().map(p => p.id),
        }),
      );
    }

    const client = this.clientFactory.createClient(profile);
    const startTime = now();

    // Create a MessageStream with dead loop detection configuration
    const deadLoopConfig = request.deadLoopDetection;
    const streamOptions = {
      enableDeadLoopDetection: deadLoopConfig?.enabled !== false,
      deadLoopConfig: deadLoopConfig?.enabled !== false ? deadLoopConfig : undefined,
    };
    const stream = new MessageStream(streamOptions);

    // Reset dead loop detector for new request
    stream.resetDeadLoopDetector();

    // Flowing statistical information
    let chunkCount = 0;
    let firstChunkTime: number | undefined;
    let lastChunkTime: number | undefined;

    // Simplify exception handling using tryCatchAsyncWithSignal
    const result = await tryCatchAsyncWithSignal(async signal => {
      stream.setRequestId(generateId());

      try {
        // Perform streaming calls
        for await (const chunk of client.generateStream({ ...request, signal })) {
          const nowTime = now();

          // Update streaming statistics
          chunkCount++;
          if (firstChunkTime === undefined) {
            firstChunkTime = nowTime;
          }
          lastChunkTime = nowTime;

          chunk.duration = diffTimestamp(startTime, nowTime);

          // Push the text content to MessageStream.
          if (chunk.content) {
            stream.pushText(chunk.content);
          }

          // Push reasoning content to MessageStream
          if (chunk.reasoningContent) {
            stream.pushReasoning(chunk.reasoningContent);
          }

          if (chunk.finishReason) {
            stream.setFinalResult(chunk);
          }
        }

        // End the stream when it is completed normally.
        stream.end();
      } catch (error) {
        // If it is a termination error, the MessageStream needs to be terminated to correctly update its internal state.
        if (isAbortError(error)) {
          stream.abort();
        }
        throw error;
      }

      // Add additional streaming statistics to the final result.
      const finalResult = stream.getFinalResult
        ? await stream.getFinalResult().catch(() => null)
        : null;
      if (finalResult && firstChunkTime !== undefined && lastChunkTime !== undefined) {
        const endTime = now();
        (finalResult as { streamStats?: Record<string, number> }).streamStats = {
          chunkCount,
          timeToFirstChunk: diffTimestamp(startTime, firstChunkTime),
          streamDuration: diffTimestamp(firstChunkTime, lastChunkTime),
          totalDuration: diffTimestamp(startTime, endTime),
        };
      }

      return stream;
    }, request.signal);

    if (result.isErr()) {
      // If it is a termination error, the MessageStream needs to be terminated to correctly update its internal state.
      if (isAbortError(result.error)) {
        stream.abort();
      }

      // Trigger an LLM flow error event
      this.emitStreamErrorEvent(request, result.error);
      return err(this.convertToLLMError(result.error, profile));
    }

    return ok(result.value);
  }

  /**
   * Register LLM Profile
   *
   * @param profile LLM Profile configuration
   */
  registerProfile(profile: Parameters<ProfileManager["register"]>[0]): void {
    this.profileManager.register(profile);
  }

  /**
   * Retrieve LLM Profile
   *
   * @param profileId Profile ID
   * @returns LLM Profile or undefined
   */
  getProfile(profileId?: string): ReturnType<ProfileManager["get"]> {
    const profile = this.profileManager.get(profileId);
    if (!profile) {
      throw new ConfigurationError("LLM Profile not found", profileId || "DEFAULT", {
        availableProfiles: this.profileManager.list().map(p => p.id),
      });
    }
    return profile;
  }

  /**
   * Delete LLM Profile
   *
   * @param profileId Profile ID
   */
  removeProfile(profileId: string): void {
    this.profileManager.remove(profileId);
    this.clientFactory.clearClientCache(profileId);
  }

  /**
   * List all Profiles
   *
   * @returns List of Profiles
   */
  listProfiles(): LLMProfile[] {
    return this.profileManager.list();
  }

  /**
   * Clear all Profile and client caches.
   */
  clearAll(): void {
    this.profileManager.clear();
    this.clientFactory.clearCache();
  }

  /**
   * Set the default Profile
   *
   * @param profileId Profile ID
   */
  setDefaultProfile(profileId: string): void {
    this.profileManager.setDefault(profileId);
  }

  /**
   * Get the default Profile ID
   *
   * @returns The default Profile ID or null
   */
  getDefaultProfileId(): string | null {
    return this.profileManager.getDefault()?.id || null;
  }

  /**
   * Translate from auto to en:
   *
   * Convert errors from both HTTP clients and LLM clients into a unified format, wrap them in an LLMError, and include additional profile information such as the provider and model.
   *
   * @param error The original error
   * @param profile The LLM profile
   * @returns LLMError
   *
   */
  private convertToLLMError(error: unknown, profile: LLMProfile): LLMError {
    if (error instanceof LLMError) {
      return error;
    }

    const errorMessage = error instanceof Error ? error.message : String(error);
    const statusCode =
      error instanceof Error && ("code" in error || "status" in error)
        ? (error as { code?: string; status?: string }).code
          ? parseInt((error as { code?: string }).code!, 10)
          : (error as { status?: string }).status
            ? parseInt((error as { status?: string }).status!, 10)
            : undefined
        : undefined;

    return new LLMError(
      `${profile.provider} API error: ${errorMessage}`,
      profile.provider,
      profile.model,
      undefined,
      statusCode,
      {
        profileId: profile.id,
        originalError: error,
      },
      error instanceof Error ? error : undefined,
    );
  }

  /**
   * Trigger an LLM stream error event
   * @param request: LLM request
   * @param error: Error
   */
  private emitStreamErrorEvent(request: LLMRequest, error: unknown): void {
    if (!this.eventManager) {
      return;
    }

    const context = request as { nodeId?: string; executionId?: string; workflowId?: string };
    const nodeId = context.nodeId;
    const executionId = context.executionId;
    const workflowId = context.workflowId;

    // Check if it is an abort error.
    if (isAbortError(error)) {
      this.eventManager.emit(
        buildLLMStreamAbortedEvent({
          workflowId: workflowId || "",
          executionId: executionId || "",
          nodeId: nodeId || "",
          reason: "Stream aborted",
        }),
      );
      return;
    }

    // Handle other errors - use a local variable to avoid TypeScript type narrowing issues
    const err = error as Error | string;
    const errorMessage =
      typeof err === "string" ? err : (err.message ?? String(err ?? "Unknown error"));

    this.eventManager.emit(
      buildLLMStreamErrorEvent({
        workflowId: workflowId || "",
        executionId: executionId || "",
        nodeId: nodeId || "",
        error: errorMessage,
      }),
    );
  }

  /**
   * Register a mock LLM client for testing purposes.
   * Delegates to ClientFactory.registerMockClient.
   *
   * @param profileId The profile ID to associate with the mock client
   * @param client The mock LLM client instance
   */
  registerMockClient(profileId: string, client: LLMClient): void {
    this.clientFactory.registerMockClient(profileId, client);
  }
}
