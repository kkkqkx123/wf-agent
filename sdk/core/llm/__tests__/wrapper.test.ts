/**
 * LLMWrapper Unit Tests
 * Test the core functionality of LLMWrapper using simple mocks
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { LLMWrapper } from "../wrapper.js";
import type { LLMProfile, LLMRequest, LLMResult, LLMClient } from "@wf-agent/types";
import { LLMError, ConfigurationError, AbortError } from "@wf-agent/types";
import { MessageStream } from "../index.js";

// Mock ProfileManager
class MockProfileManager {
  private profiles: Map<string, LLMProfile> = new Map();
  private defaultProfileId: string | null = null;

  register(profile: LLMProfile): void {
    this.profiles.set(profile.id, profile);
  }

  get(profileId?: string): LLMProfile | undefined {
    const id = profileId || this.defaultProfileId;
    return id ? this.profiles.get(id) : undefined;
  }

  remove(profileId: string): void {
    this.profiles.delete(profileId);
  }

  list(): LLMProfile[] {
    return Array.from(this.profiles.values());
  }

  clear(): void {
    this.profiles.clear();
    this.defaultProfileId = null;
  }

  setDefault(profileId: string): void {
    if (!this.profiles.has(profileId)) {
      throw new ConfigurationError("Profile not found", profileId);
    }
    this.defaultProfileId = profileId;
  }

  getDefault(): LLMProfile | undefined {
    return this.defaultProfileId ? this.profiles.get(this.defaultProfileId) : undefined;
  }
}

// Mock ClientFactory
class MockClientFactory {
  private clientCache: Map<string, LLMClient> = new Map();
  private mockClient: LLMClient | null = null;

  setMockClient(client: LLMClient): void {
    this.mockClient = client;
  }

  createClient(profile: LLMProfile): LLMClient {
    const cacheKey = profile.id;
    if (this.clientCache.has(cacheKey)) {
      return this.clientCache.get(cacheKey)!;
    }
    const client = this.mockClient || this.createDefaultMockClient();
    this.clientCache.set(cacheKey, client);
    return client;
  }

  clearCache(): void {
    this.clientCache.clear();
  }

  clearClientCache(profileId: string): void {
    for (const key of this.clientCache.keys()) {
      if (key.startsWith(profileId)) {
        this.clientCache.delete(key);
      }
    }
  }

  private createDefaultMockClient(): LLMClient {
    return {
      generate: vi.fn(),
      generateStream: vi.fn(),
    };
  }
}

// Mock EventRegistry
class MockEventManager {
  private events: any[] = [];

  emit(event: any): void {
    this.events.push(event);
  }

  getEvents(): any[] {
    return this.events;
  }

  clear(): void {
    this.events = [];
  }
}

describe("LLMWrapper", () => {
  let wrapper: LLMWrapper;
  let mockProfileManager: MockProfileManager;
  let mockClientFactory: MockClientFactory;
  let mockEventManager: MockEventManager;
  let mockClient: LLMClient;

  const testProfile: LLMProfile = {
    id: "test-profile",
    name: "Test Profile",
    provider: "OPENAI_CHAT",
    model: "gpt-4",
    apiKey: "test-api-key",
    parameters: {
      temperature: 0.7,
      maxTokens: 1000,
    },
  };

  const testRequest: LLMRequest = {
    profileId: "test-profile",
    messages: [{ role: "user", content: "Hello" }],
  };

  beforeEach(() => {
    mockProfileManager = new MockProfileManager();
    mockClientFactory = new MockClientFactory();
    mockEventManager = new MockEventManager();

    // Create a mock client
    mockClient = {
      generate: vi.fn(),
      generateStream: vi.fn(),
    };
    mockClientFactory.setMockClient(mockClient);

    // Register a test profile
    mockProfileManager.register(testProfile);

    // Create a wrapper instance and inject the mock.
    wrapper = new LLMWrapper(mockEventManager as any);
    (wrapper as any).profileManager = mockProfileManager;
    (wrapper as any).clientFactory = mockClientFactory;
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe("Profile Management", () => {
    it("Should be able to register Profile", () => {
      const newProfile: LLMProfile = {
        id: "new-profile",
        name: "New Profile",
        provider: "ANTHROPIC",
        model: "claude-3",
        apiKey: "new-api-key",
        parameters: {},
      };

      wrapper.registerProfile(newProfile);
      expect(wrapper.listProfiles()).toContainEqual(newProfile);
    });

    it("It should be possible to get the Profile", () => {
      const profile = wrapper.getProfile("test-profile");
      expect(profile).toEqual(testProfile);
    });

    it("It should be possible to delete the Profile", () => {
      wrapper.removeProfile("test-profile");
      expect(wrapper.listProfiles()).not.toContainEqual(testProfile);
    });

    it("Should be able to list all Profiles", () => {
      const profiles = wrapper.listProfiles();
      expect(profiles).toHaveLength(1);
      expect(profiles[0]).toEqual(testProfile);
    });

    it("Should be able to clear all Profiles", () => {
      wrapper.clearAll();
      expect(wrapper.listProfiles()).toHaveLength(0);
    });

    it("It should be possible to set a default Profile", () => {
      wrapper.setDefaultProfile("test-profile");
      expect(wrapper.getDefaultProfileId()).toBe("test-profile");
    });

    it("Getting a non-existent Profile should throw a ConfigurationError.", () => {
      expect(() => wrapper.getProfile("non-existent")).toThrow(ConfigurationError);
    });
  });

  describe("Non-streaming (generate)", () => {
    it("The response should be generated successfully", async () => {
      const mockResult: LLMResult = {
        id: "test-id",
        model: "gpt-4",
        content: "Hello!",
        message: { role: "assistant", content: "Hello!" },
        finishReason: "stop",
        duration: 0, // The `wrapper` will recalculate the `duration`.
      };

      vi.mocked(mockClient.generate).mockResolvedValue(mockResult);

      const result = await wrapper.generate(testRequest);

      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value.content).toBe("Hello!");
        // The `wrapper` will recalculate the `duration`, so it should be greater than 0.
        expect(result.value.duration).toBeGreaterThanOrEqual(0);
      }
    });

    it("The signal should be passed to the client correctly.", async () => {
      const mockResult: LLMResult = {
        id: "test-id",
        model: "gpt-4",
        content: "Hello!",
        message: { role: "assistant", content: "Hello!" },
        finishReason: "stop",
        duration: 100,
      };

      vi.mocked(mockClient.generate).mockResolvedValue(mockResult);

      const controller = new AbortController();
      const requestWithSignal: LLMRequest = {
        ...testRequest,
        signal: controller.signal,
      };

      await wrapper.generate(requestWithSignal);

      expect(mockClient.generate).toHaveBeenCalledWith(
        expect.objectContaining({
          signal: controller.signal,
        }),
      );
    });

    it("Profile non-existence errors should be handled", async () => {
      const requestWithInvalidProfile: LLMRequest = {
        ...testRequest,
        profileId: "non-existent",
      };

      // The `getProfile` method may throw a `ConfigurationError`, so it is necessary to catch the exception.
      await expect(wrapper.generate(requestWithInvalidProfile)).rejects.toThrow(ConfigurationError);
    });

    it("Client-side errors should be handled", async () => {
      const error = new Error("API Error");
      vi.mocked(mockClient.generate).mockRejectedValue(error);

      const result = await wrapper.generate(testRequest);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toBeInstanceOf(LLMError);
        expect(result.error.message).toContain("OPENAI_CHAT API error");
      }
    });

    it("AbortError should be handled", async () => {
      const abortError = new AbortError("Operation aborted");
      vi.mocked(mockClient.generate).mockRejectedValue(abortError);

      const controller = new AbortController();
      controller.abort();

      const requestWithSignal: LLMRequest = {
        ...testRequest,
        signal: controller.signal,
      };

      const result = await wrapper.generate(requestWithSignal);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toBeInstanceOf(LLMError);
      }
    });
  });

  describe("Stream generation (generateStream)", () => {
    it("The streaming response should be generated successfully", async () => {
      const mockChunks: LLMResult[] = [
        {
          id: "test-id",
          model: "gpt-4",
          content: "Hello",
          message: { role: "assistant", content: "Hello" },
          finishReason: "",
          duration: 50,
        },
        {
          id: "test-id",
          model: "gpt-4",
          content: "Hello!",
          message: { role: "assistant", content: "Hello!" },
          finishReason: "stop",
          duration: 100,
        },
      ];

      async function* generateMockStream(): AsyncIterable<LLMResult> {
        for (const chunk of mockChunks) {
          yield chunk;
        }
      }

      vi.mocked(mockClient.generateStream).mockReturnValue(generateMockStream());

      const result = await wrapper.generateStream(testRequest);

      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toBeInstanceOf(MessageStream);
      }
    });

    it("The signal should be passed to the client correctly.", async () => {
      async function* generateMockStream(): AsyncIterable<LLMResult> {
        yield {
          id: "test-id",
          model: "gpt-4",
          content: "Hello",
          message: { role: "assistant", content: "Hello" },
          finishReason: "stop",
          duration: 100,
        };
      }

      vi.mocked(mockClient.generateStream).mockReturnValue(generateMockStream());

      const controller = new AbortController();
      const requestWithSignal: LLMRequest = {
        ...testRequest,
        signal: controller.signal,
      };

      await wrapper.generateStream(requestWithSignal);

      expect(mockClient.generateStream).toHaveBeenCalledWith(
        expect.objectContaining({
          signal: controller.signal,
        }),
      );
    });

    it("Profile non-existence errors should be handled", async () => {
      const requestWithInvalidProfile: LLMRequest = {
        ...testRequest,
        profileId: "non-existent",
      };

      // The `getProfile` method may throw a `ConfigurationError`, so it is necessary to catch the exception.
      await expect(wrapper.generateStream(requestWithInvalidProfile)).rejects.toThrow(
        ConfigurationError,
      );
    });

    it("Client-side errors should be handled", async () => {
      const error = new Error("Stream Error");
      vi.mocked(mockClient.generateStream).mockImplementation(() => {
        throw error;
      });

      const result = await wrapper.generateStream(testRequest);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toBeInstanceOf(LLMError);
        expect(result.error.message).toContain("OPENAI_CHAT API error");
      }
    });

    it("AbortError should be handled correctly and the MessageStream should be aborted.", async () => {
      const abortError = new AbortError("Operation aborted");
      vi.mocked(mockClient.generateStream).mockImplementation(() => {
        throw abortError;
      });

      const controller = new AbortController();
      controller.abort();

      const requestWithSignal: LLMRequest = {
        ...testRequest,
        signal: controller.signal,
      };

      const result = await wrapper.generateStream(requestWithSignal);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toBeInstanceOf(LLMError);
      }

      // Verify that the event manager has received the abort event.
      const events = mockEventManager.getEvents();
      const abortEvent = events.find(e => e.type === "LLM_STREAM_ABORTED");
      expect(abortEvent).toBeDefined();
    });

    it("The LLM_STREAM_ERROR event should be triggered", async () => {
      const error = new Error("Stream Error");
      vi.mocked(mockClient.generateStream).mockImplementation(() => {
        throw error;
      });

      const result = await wrapper.generateStream(testRequest);

      expect(result.isErr()).toBe(true);

      // Verify that the event manager has received the error event.
      const events = mockEventManager.getEvents();
      const errorEvent = events.find(e => e.type === "LLM_STREAM_ERROR");
      expect(errorEvent).toBeDefined();
      expect(errorEvent.error).toBe("Stream Error");
    });

    it("The LLM_STREAM_ABORTED event should be triggered", async () => {
      const abortError = new AbortError("Operation aborted");
      vi.mocked(mockClient.generateStream).mockImplementation(() => {
        throw abortError;
      });

      const controller = new AbortController();
      controller.abort();

      const requestWithSignal: LLMRequest = {
        ...testRequest,
        signal: controller.signal,
      };

      await wrapper.generateStream(requestWithSignal);

      // Verify that the event manager has received the abort event.
      const events = mockEventManager.getEvents();
      const abortEvent = events.find(e => e.type === "LLM_STREAM_ABORTED");
      expect(abortEvent).toBeDefined();
      expect(abortEvent.reason).toBe("Stream aborted");
    });
  });

  describe("event manager", () => {
    it("It should be possible to set up an event manager", () => {
      const newEventManager = new MockEventManager();
      wrapper.setEventManager(newEventManager as any);
      expect((wrapper as any).eventManager).toBe(newEventManager);
    });

    it("No error should be thrown when there is no event manager", async () => {
      const wrapperWithoutEventManager = new LLMWrapper();
      (wrapperWithoutEventManager as any).profileManager = mockProfileManager;
      (wrapperWithoutEventManager as any).clientFactory = mockClientFactory;

      const error = new Error("Stream Error");
      vi.mocked(mockClient.generateStream).mockImplementation(() => {
        throw error;
      });

      const result = await wrapperWithoutEventManager.generateStream(testRequest);

      expect(result.isErr()).toBe(true);
      // No errors should be thrown.
    });
  });

  describe("misconversion", () => {
    it("Common errors should be converted correctly", async () => {
      const error = new Error("API Error");
      vi.mocked(mockClient.generate).mockRejectedValue(error);

      const result = await wrapper.generate(testRequest);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toBeInstanceOf(LLMError);
        expect(result.error.message).toContain("OPENAI_CHAT API error: API Error");
        expect(result.error.provider).toBe("OPENAI_CHAT");
        expect(result.error.model).toBe("gpt-4");
      }
    });

    it("Errors with code should be converted correctly", async () => {
      const error = new Error("Rate limit exceeded");
      (error as any).code = 429;
      vi.mocked(mockClient.generate).mockRejectedValue(error);

      const result = await wrapper.generate(testRequest);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toBeInstanceOf(LLMError);
        expect(result.error.statusCode).toBe(429);
      }
    });

    it("Errors with status should be converted correctly", async () => {
      const error = new Error("Internal server error");
      (error as any).status = 500;
      vi.mocked(mockClient.generate).mockRejectedValue(error);

      const result = await wrapper.generate(testRequest);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toBeInstanceOf(LLMError);
        expect(result.error.statusCode).toBe(500);
      }
    });

    it("Non-Error objects should be converted correctly", async () => {
      vi.mocked(mockClient.generate).mockRejectedValue("String error");

      const result = await wrapper.generate(testRequest);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toBeInstanceOf(LLMError);
        expect(result.error.message).toContain("OPENAI_CHAT API error: String error");
      }
    });
  });
});
