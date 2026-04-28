/**
 * Trigger Matcher Integration Tests
 *
 * Test Scenarios:
 * - Basic matching functionality
 * - Batch matching functionality
 * - Custom matchers
 */

import { describe, it, expect, beforeEach } from "vitest";
import {
  defaultTriggerMatcher,
  matchTriggerCondition,
  matchTriggers,
  createTriggerMatcher,
} from "../triggers/matcher.js";
import type { BaseTriggerCondition, BaseEventData } from "../triggers/types.js";

describe("Trigger Matcher - Trigger Matcher", () => {
  describe("Basic Matching Function", () => {
    it("Test event type matching: same event type should match successfully", () => {
      const condition: BaseTriggerCondition = {
        eventType: "THREAD_STARTED",
        eventName: "test-event",
      };

      const event: BaseEventData = {
        type: "THREAD_STARTED",
        eventName: "test-event",
        timestamp: Date.now(),
      };

      expect(defaultTriggerMatcher(condition, event)).toBe(true);
    });

    it("Test event type mismatch: different event types should fail to match", () => {
      const condition: BaseTriggerCondition = {
        eventType: "THREAD_STARTED",
        eventName: "test-event",
      };

      const event: BaseEventData = {
        type: "THREAD_COMPLETED",
        eventName: "test-event",
        timestamp: Date.now(),
      };

      expect(defaultTriggerMatcher(condition, event)).toBe(false);
    });

    it("Test event name match: when eventName is specified, the event name should match", () => {
      const condition: BaseTriggerCondition = {
        eventType: "THREAD_STARTED",
        eventName: "custom-event",
      };

      const event: BaseEventData = {
        type: "THREAD_STARTED",
        eventName: "custom-event",
        timestamp: Date.now(),
      };

      expect(defaultTriggerMatcher(condition, event)).toBe(true);
    });

    it("Test event name mismatch: different event names should fail to match when eventName is specified", () => {
      const condition: BaseTriggerCondition = {
        eventType: "THREAD_STARTED",
        eventName: "custom-event",
      };

      const event: BaseEventData = {
        type: "THREAD_STARTED",
        eventName: "different-event",
        timestamp: Date.now(),
      };

      expect(defaultTriggerMatcher(condition, event)).toBe(false);
    });

    it("Test for unspecified event name: when eventName is not specified, only the event type is matched", () => {
      const condition: BaseTriggerCondition = {
        eventType: "THREAD_STARTED",
      };

      const event1: BaseEventData = {
        type: "THREAD_STARTED",
        eventName: "any-event",
        timestamp: Date.now(),
      };

      const event2: BaseEventData = {
        type: "THREAD_STARTED",
        timestamp: Date.now(),
      };

      expect(defaultTriggerMatcher(condition, event1)).toBe(true);
      expect(defaultTriggerMatcher(condition, event2)).toBe(true);
    });

    it("Testing the matchTriggerCondition Function", () => {
      const condition: BaseTriggerCondition = {
        eventType: "NODE_COMPLETED",
        eventName: "node-1",
      };

      const event: BaseEventData = {
        type: "NODE_COMPLETED",
        eventName: "node-1",
        timestamp: Date.now(),
      };

      expect(matchTriggerCondition(condition, event)).toBe(true);
    });
  });

  describe("Batch Matching Function", () => {
    it("Test Batch Match: Filter out matching triggers from multiple triggers", () => {
      const triggers = [
        {
          id: "trigger-1",
          name: "Trigger 1",
          condition: { eventType: "THREAD_STARTED" },
          action: { type: "pause_thread", parameters: {} },
          enabled: true,
        },
        {
          id: "trigger-2",
          name: "Trigger 2",
          condition: { eventType: "THREAD_COMPLETED" },
          action: { type: "pause_thread", parameters: {} },
          enabled: true,
        },
        {
          id: "trigger-3",
          name: "Trigger 3",
          condition: { eventType: "THREAD_STARTED" },
          action: { type: "resume_thread", parameters: {} },
          enabled: true,
        },
      ];

      const event: BaseEventData = {
        type: "THREAD_STARTED",
        timestamp: Date.now(),
      };

      const matched = matchTriggers(triggers, event);

      expect(matched).toHaveLength(2);
      expect(matched[0]?.id).toBe("trigger-1");
      expect(matched[1]?.id).toBe("trigger-3");
    });

    it("Test for skipping disabled triggers: triggers with enabled as false should be skipped", () => {
      const triggers = [
        {
          id: "trigger-1",
          name: "Trigger 1",
          condition: { eventType: "THREAD_STARTED" },
          action: { type: "pause_thread", parameters: {} },
          enabled: true,
        },
        {
          id: "trigger-2",
          name: "Trigger 2",
          condition: { eventType: "THREAD_STARTED" },
          action: { type: "pause_thread", parameters: {} },
          enabled: false,
        },
        {
          id: "trigger-3",
          name: "Trigger 3",
          condition: { eventType: "THREAD_STARTED" },
          action: { type: "resume_thread", parameters: {} },
          enabled: true,
        },
      ];

      const event: BaseEventData = {
        type: "THREAD_STARTED",
        timestamp: Date.now(),
      };

      const matched = matchTriggers(triggers, event);

      expect(matched).toHaveLength(2);
      expect(matched.every(t => t.enabled !== false)).toBe(true);
    });

    it("Enabled by default when the test enabled is undefined", () => {
      const triggers = [
        {
          id: "trigger-1",
          name: "Trigger 1",
          condition: { eventType: "THREAD_STARTED" },
          action: { type: "pause_thread", parameters: {} },
          // enabled not set
        },
      ];

      const event: BaseEventData = {
        type: "THREAD_STARTED",
        timestamp: Date.now(),
      };

      const matched = matchTriggers(triggers, event);

      expect(matched).toHaveLength(1);
    });

    it("Testing Custom Matchers: Matching with Custom Matching Logic", () => {
      const triggers = [
        {
          id: "trigger-1",
          name: "Trigger 1",
          condition: { eventType: "NODE_CUSTOM_EVENT", eventName: "event-1" },
          action: { type: "pause_thread", parameters: {} },
          enabled: true,
        },
        {
          id: "trigger-2",
          name: "Trigger 2",
          condition: { eventType: "NODE_CUSTOM_EVENT", eventName: "event-2" },
          action: { type: "pause_thread", parameters: {} },
          enabled: true,
        },
      ];

      const event: BaseEventData = {
        type: "NODE_CUSTOM_EVENT",
        eventName: "event-1",
        timestamp: Date.now(),
      };

      const customMatcher = createTriggerMatcher((condition, event) => {
        // Custom logic: Only match eventNames that contain the word 'special'.
        return condition.eventName?.includes("special") || false;
      });

      const matched = matchTriggers(triggers, event, customMatcher);

      // Since the custom matcher requires the presence of 'special', none of them match.
      expect(matched).toHaveLength(0);
    });
  });

  describe("Customizable Matchers", () => {
    it("Test creation of custom matchers: custom matchers should perform custom logic on top of the default matches", () => {
      const condition: BaseTriggerCondition = {
        eventType: "THREAD_STARTED",
        eventName: "special-event",
      };

      const event1: BaseEventData = {
        type: "THREAD_STARTED",
        eventName: "special-event",
        timestamp: Date.now(),
      };

      const event2: BaseEventData = {
        type: "THREAD_STARTED",
        eventName: "normal-event",
        timestamp: Date.now(),
      };

      const customMatcher = createTriggerMatcher((condition, event) => {
        // Custom logic: Only match eventName that contain the word 'special'.
        return condition.eventName?.includes("special") || false;
      });

      expect(customMatcher(condition, event1)).toBe(true);
      expect(customMatcher(condition, event2)).toBe(false);
    });

    it("Testing custom matcher combinations: custom matchers can combine multiple conditions", () => {
      const condition: BaseTriggerCondition = {
        eventType: "NODE_CUSTOM_EVENT",
        eventName: "important-event",
        metadata: { priority: "high" },
      };

      const event1: BaseEventData = {
        type: "NODE_CUSTOM_EVENT",
        eventName: "important-event",
        data: { priority: "high" },
        timestamp: Date.now(),
      };

      const event2: BaseEventData = {
        type: "NODE_CUSTOM_EVENT",
        eventName: "important-event",
        data: { priority: "low" },
        timestamp: Date.now(),
      };

      const customMatcher = createTriggerMatcher((condition, event) => {
        const eventData = event.data as { priority?: string } | undefined;
        return eventData?.priority === "high";
      });

      expect(customMatcher(condition, event1)).toBe(true);
      expect(customMatcher(condition, event2)).toBe(false);
    });

    it("Test custom matchers to execute default matches first: custom logic is not executed when default matches fail", () => {
      const condition: BaseTriggerCondition = {
        eventType: "THREAD_STARTED",
        eventName: "test-event",
      };

      const event: BaseEventData = {
        type: "THREAD_COMPLETED", // Event type does not match.
        eventName: "test-event",
        timestamp: Date.now(),
      };

      const customMatcher = createTriggerMatcher(() => {
        // This logic should not be executed.
        return true;
      });

      expect(customMatcher(condition, event)).toBe(false);
    });

    it("Test custom matchers when eventName is not specified", () => {
      const condition: BaseTriggerCondition = {
        eventType: "THREAD_STARTED",
      };

      const event: BaseEventData = {
        type: "THREAD_STARTED",
        timestamp: Date.now(),
      };

      const customMatcher = createTriggerMatcher((condition, event) => {
        const eventData = event.data as { valid?: boolean } | undefined;
        return eventData?.valid === true;
      });

      const event1: BaseEventData = {
        type: "THREAD_STARTED",
        data: { valid: true },
        timestamp: Date.now(),
      };

      const event2: BaseEventData = {
        type: "THREAD_STARTED",
        data: { valid: false },
        timestamp: Date.now(),
      };

      expect(customMatcher(condition, event1)).toBe(true);
      expect(customMatcher(condition, event2)).toBe(false);
    });
  });

  describe("Boundary situation", () => {
    it("Test empty event type list", () => {
      const triggers: any[] = [];

      const event: BaseEventData = {
        type: "THREAD_STARTED",
        timestamp: Date.now(),
      };

      const matched = matchTriggers(triggers, event);

      expect(matched).toHaveLength(0);
    });

    it("The test event does not specify an eventName but the condition does.", () => {
      const condition: BaseTriggerCondition = {
        eventType: "THREAD_STARTED",
        eventName: "required-event",
      };

      const event: BaseEventData = {
        type: "THREAD_STARTED",
        timestamp: Date.now(),
      };

      expect(defaultTriggerMatcher(condition, event)).toBe(false);
    });

    it("The test condition does not specify eventName but the event does", () => {
      const condition: BaseTriggerCondition = {
        eventType: "THREAD_STARTED",
      };

      const event: BaseEventData = {
        type: "THREAD_STARTED",
        eventName: "any-event",
        timestamp: Date.now(),
      };

      expect(defaultTriggerMatcher(condition, event)).toBe(true);
    });

    it("Testing metadata does not affect matches", () => {
      const condition: BaseTriggerCondition = {
        eventType: "THREAD_STARTED",
        eventName: "test-event",
        metadata: { key: "value" },
      };

      const event: BaseEventData = {
        type: "THREAD_STARTED",
        eventName: "test-event",
        timestamp: Date.now(),
        data: { different: "metadata" },
      };

      expect(defaultTriggerMatcher(condition, event)).toBe(true);
    });
  });
});
