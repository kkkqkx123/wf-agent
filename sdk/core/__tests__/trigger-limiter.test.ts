/**
 * Trigger Limiter Integration Testing
 *
 * Test Scenarios:
 * - Trigger capability verification
 * - Status retrieval
 * - Count management
 */

import { describe, it, expect, beforeEach } from "vitest";
import {
  canTrigger,
  getTriggerStatus,
  incrementTriggerCount,
  resetTriggerCount,
  isTriggerExpired,
  getRemainingTriggers,
} from "../triggers/limiter.js";
import type { BaseTriggerDefinition } from "../triggers/types.js";

describe("Trigger Limiter - Trigger Limiter", () => {
  describe("Trigger Capability Check", () => {
    it("Test the trigger for the enabled status: It should be triggerable when 'enabled' is set to true or not set at all.", () => {
      const trigger1: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
      };

      const trigger2: BaseTriggerDefinition = {
        id: "trigger-2",
        name: "Trigger 2",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        // enabled is not set
      };

      expect(canTrigger(trigger1)).toBe(true);
      expect(canTrigger(trigger2)).toBe(true);
    });

    it("Testing the disabled status trigger: It should not be triggered when 'enabled' is set to 'false'.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: false,
      };

      expect(canTrigger(trigger)).toBe(false);
    });

    it("Test did not reach the maximum number of attempts: It should have triggered when maxTriggers was not reached.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 5,
        triggerCount: 2,
      };

      expect(canTrigger(trigger)).toBe(true);
    });

    it("Test reached the maximum number of triggers: It should not be triggered when maxTriggers is reached.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 5,
        triggerCount: 5,
      };

      expect(canTrigger(trigger)).toBe(false);
    });

    it("Test exceeded the maximum number of attempts: It should not be triggered when the maxTriggers limit is exceeded.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 5,
        triggerCount: 6,
      };

      expect(canTrigger(trigger)).toBe(false);
    });

    it("Test unlimited triggering: Triggering should be possible when maxTriggers is 0 or not set.", () => {
      const trigger1: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 0,
        triggerCount: 1000,
      };

      const trigger2: BaseTriggerDefinition = {
        id: "trigger-2",
        name: "Trigger 2",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        triggerCount: 1000,
        // `maxTriggers` is not set.
      };

      expect(canTrigger(trigger1)).toBe(true);
      expect(canTrigger(trigger2)).toBe(true);
    });

    it("When the number of test triggers is 0: Triggers that have not been triggered before should be able to be triggered.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 3,
        // `triggerCount` is not set; the default value is 0.
      };

      expect(canTrigger(trigger)).toBe(true);
    });
  });

  describe("Status retrieval", () => {
    it("Testing idle state: The status of a trigger that has not been triggered should be 'idle'.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        // `triggerCount` is not set; the default value is 0.
      };

      expect(getTriggerStatus(trigger)).toBe("idle");
    });

    it("Test trigger status: The status of a triggered trigger should be 'triggered'.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        triggerCount: 1,
      };

      expect(getTriggerStatus(trigger)).toBe("triggered");
    });

    it("Test disabled state: The status of a trigger with 'enabled' set to 'false' should be 'disabled'.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: false,
        triggerCount: 0,
      };

      expect(getTriggerStatus(trigger)).toBe("disabled");
    });

    it("Test priority for disabled state: When 'enabled' is set to false, it should always be considered as 'disabled', even if it has been triggered before.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: false,
        triggerCount: 5,
      };

      expect(getTriggerStatus(trigger)).toBe("disabled");
    });

    it("Test expiration status: The trigger status that reaches maxTriggers should be considered expired.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 5,
        triggerCount: 5,
      };

      expect(getTriggerStatus(trigger)).toBe("expired");
    });

    it("Test Expiration Status Priority: The expiration status takes precedence over the triggered status.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 5,
        triggerCount: 5,
      };

      expect(getTriggerStatus(trigger)).toBe("expired");
    });

    it("When the number of tests exceeds the maximum limit: The status of triggers that have exceeded maxTriggers should be set to expired.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 5,
        triggerCount: 6,
      };

      expect(getTriggerStatus(trigger)).toBe("expired");
    });
  });

  describe("Count Management", () => {
    it("Test the increment of the trigger count: The trigger count should increase correctly.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        triggerCount: 2,
      };

      const newCount = incrementTriggerCount(trigger);

      expect(newCount).toBe(3);
      expect(trigger.triggerCount).toBe(3);
    });

    it("The test starts with a count of 0 and increments the trigger count.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        // `triggerCount` is not set; the default value is `undefined`.
      };

      const newCount = incrementTriggerCount(trigger);

      expect(newCount).toBe(1);
      expect(trigger.triggerCount).toBe(1);
    });

    it("Test reset trigger count: The count should be reset to zero after a reset.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        triggerCount: 10,
      };

      resetTriggerCount(trigger);

      expect(trigger.triggerCount).toBe(0);
    });

    it("Test multiple increases to trigger the count.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
      };

      incrementTriggerCount(trigger);
      incrementTriggerCount(trigger);
      incrementTriggerCount(trigger);

      expect(trigger.triggerCount).toBe(3);
    });

    it("Test remaining attempts calculation: Correctly calculate the remaining number of triggers.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 5,
        triggerCount: 2,
      };

      const remaining = getRemainingTriggers(trigger);

      expect(remaining).toBe(3);
    });

    it("Test remaining attempts: 0 - The remaining attempts are 0 when the maximum number of attempts is reached.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 5,
        triggerCount: 5,
      };

      const remaining = getRemainingTriggers(trigger);

      expect(remaining).toBe(0);
    });

    it("Test unlimited remaining attempts: When maxTriggers is 0, -1 should be returned.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 0,
        triggerCount: 100,
      };

      const remaining = getRemainingTriggers(trigger);

      expect(remaining).toBe(-1);
    });

    it("Test result when maxTriggers is not set: The remaining number should return -1.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        triggerCount: 100,
        // `maxTriggers` is not set.
      };

      const remaining = getRemainingTriggers(trigger);

      expect(remaining).toBe(-1);
    });

    it("When the number of attempts exceeds the maximum limit, the remaining attempts should be 0, not a negative number.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 5,
        triggerCount: 7,
      };

      const remaining = getRemainingTriggers(trigger);

      expect(remaining).toBe(0);
    });

    it("Test the association between trigger counts and remaining counts.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 3,
      };

      // Initial state
      expect(trigger.triggerCount).toBeUndefined();
      expect(getRemainingTriggers(trigger)).toBe(3);

      // First trigger
      incrementTriggerCount(trigger);
      expect(trigger.triggerCount).toBe(1);
      expect(getRemainingTriggers(trigger)).toBe(2);

      // Second trigger
      incrementTriggerCount(trigger);
      expect(trigger.triggerCount).toBe(2);
      expect(getRemainingTriggers(trigger)).toBe(1);

      // Third trigger
      incrementTriggerCount(trigger);
      expect(trigger.triggerCount).toBe(3);
      expect(getRemainingTriggers(trigger)).toBe(0);

      // Reset
      resetTriggerCount(trigger);
      expect(trigger.triggerCount).toBe(0);
      expect(getRemainingTriggers(trigger)).toBe(3);
    });
  });

  describe("Expiration check", () => {
    it("Test whether the trigger has expired: It should not expire if the maximum number of attempts has not been reached.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 5,
        triggerCount: 2,
      };

      expect(isTriggerExpired(trigger)).toBe(false);
    });

    it("Test to check if the trigger has expired: It should expire when the maximum number of occurrences is reached.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 5,
        triggerCount: 5,
      };

      expect(isTriggerExpired(trigger)).toBe(true);
    });

    it("Test to check if the trigger has expired: It should expire after exceeding the maximum number of attempts.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 5,
        triggerCount: 6,
      };

      expect(isTriggerExpired(trigger)).toBe(true);
    });

    it("The test for unlimited triggers should not expire: It should not expire when `maxTriggers` is set to 0 or not defined at all.", () => {
      const trigger1: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 0,
        triggerCount: 1000,
      };

      const trigger2: BaseTriggerDefinition = {
        id: "trigger-2",
        name: "Trigger 2",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        triggerCount: 1000,
        // `maxTriggers` is not set.
      };

      expect(isTriggerExpired(trigger1)).toBe(false);
      expect(isTriggerExpired(trigger2)).toBe(false);
    });

    it("Testing the one-time trigger: The trigger should expire after being activated once when maxTriggers is set to 1.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 1,
      };

      expect(isTriggerExpired(trigger)).toBe(false);

      incrementTriggerCount(trigger);

      expect(isTriggerExpired(trigger)).toBe(true);
    });
  });

  describe("Boundary cases", () => {
    it("Test the negative value for maxTriggers: It should be considered unlimited.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: -1,
        triggerCount: 100,
      };

      expect(canTrigger(trigger)).toBe(true);
      expect(isTriggerExpired(trigger)).toBe(false);
      expect(getRemainingTriggers(trigger)).toBe(-1);
    });

    it("Test with maxTriggers set to 1 and triggerCount set to 0: It should be possible to trigger.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: true,
        maxTriggers: 1,
      };

      expect(canTrigger(trigger)).toBe(true);
      expect(getTriggerStatus(trigger)).toBe("idle");
    });

    it("Test both reaching the maximum number of attempts and the disabled state: The disabled state takes precedence.", () => {
      const trigger: BaseTriggerDefinition = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" },
        action: { type: "pause_thread", parameters: {} },
        enabled: false,
        maxTriggers: 5,
        triggerCount: 5,
      };

      expect(canTrigger(trigger)).toBe(false);
      expect(getTriggerStatus(trigger)).toBe("disabled");
    });
  });
});
