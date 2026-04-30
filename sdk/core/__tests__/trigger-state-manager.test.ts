/**
 * Trigger State Manager Integration Testing
 *
 * Test Scenarios:
 * - State registration
 * - State update
 * - Snapshot functionality
 * - Query and deletion
 */

import { describe, it, expect, beforeEach } from "vitest";
import { TriggerState } from "../../workflow/state-managers/trigger-state.js";
import type { TriggerRuntimeState } from "@wf-agent/types";
import { NotFoundError, RuntimeValidationError } from "@wf-agent/types";

describe("Trigger State Manager", () => {
  let stateManager: TriggerState;

  beforeEach(() => {
    stateManager = new TriggerState("test-thread");
    stateManager.setWorkflowId("workflow-123");
  });

  describe("Status Registration", () => {
    it("Test registration status: register method stores the status correctly", () => {
      const state: TriggerRuntimeState = {
        triggerId: "trigger-1",
        executionId: "test-thread",
        workflowId: "workflow-123",
        status: "enabled",
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      stateManager.register(state);

      expect(stateManager.hasState("trigger-1")).toBe(true);
      const retrievedState = stateManager.getState("trigger-1");
      expect(retrievedState).toEqual(state);
    });

    it("Test execution ID validation: the executionId in the state must match the manager's executionId", () => {
      const state: TriggerRuntimeState = {
        triggerId: "trigger-1",
        executionId: "different-thread", // Does not match.
        workflowId: "workflow-123",
        status: "enabled",
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      expect(() => {
        stateManager.register(state);
      }).toThrow(RuntimeValidationError);
    });

    it("Test workflow ID validation: the workflowId in the state must match the manager's workflowId", () => {
      const state: TriggerRuntimeState = {
        triggerId: "trigger-1",
        executionId: "test-thread",
        workflowId: "different-workflow", // Does not match
        status: "enabled",
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      expect(() => {
        stateManager.register(state);
      }).toThrow(RuntimeValidationError);
    });

    it("Test Trigger ID is null: an error should be thrown", () => {
      const state: any = {
        triggerId: "", // Empty ID
        executionId: "test-thread",
        workflowId: "workflow-123",
        status: "enabled",
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      expect(() => {
        stateManager.register(state);
      }).toThrow(RuntimeValidationError);
    });

    it("Test execution ID is null: an error should be thrown", () => {
      const state: any = {
        triggerId: "trigger-1",
        executionId: "", // Empty ID
        workflowId: "workflow-123",
        status: "enabled",
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      expect(() => {
        stateManager.register(state);
      }).toThrow(RuntimeValidationError);
    });

    it("Test workflow ID is empty: an error should be thrown", () => {
      const state: any = {
        triggerId: "trigger-1",
        executionId: "test-thread",
        workflowId: "", // Empty ID
        status: "enabled",
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      expect(() => {
        stateManager.register(state);
      }).toThrow(RuntimeValidationError);
    });

    it("Testing for duplicate registrations: duplicate registrations should throw an error", () => {
      const state: TriggerRuntimeState = {
        triggerId: "trigger-1",
        executionId: "test-thread",
        workflowId: "workflow-123",
        status: "enabled",
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      stateManager.register(state);

      expect(() => {
        stateManager.register(state);
      }).toThrow();
    });

    it("Test that the workflow ID is not set: an error should be thrown if the workflow ID is null", () => {
      const managerWithoutWorkflowId = new TriggerState("test-thread");
      // Do not set the workflowId.

      const state: TriggerRuntimeState = {
        triggerId: "trigger-1",
        executionId: "test-thread",
        workflowId: null as any,
        status: "enabled",
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      expect(() => {
        managerWithoutWorkflowId.register(state);
      }).toThrow(RuntimeValidationError);
    });
  });

  describe("Status Update", () => {
    beforeEach(() => {
      const state: TriggerRuntimeState = {
        triggerId: "trigger-1",
        executionId: "test-thread",
        workflowId: "workflow-123",
        status: "enabled",
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      stateManager.register(state);
    });

    it("Test update status: updateStatus method updates the status correctly", () => {
      const originalState = stateManager.getState("trigger-1");
      const originalUpdatedAt = originalState!.updatedAt;

      // Wait for 1 millisecond to ensure that the timestamps are different.
      return new Promise<void>(resolve => {
        setTimeout(() => {
          stateManager.updateStatus("trigger-1", "disabled");

          const updatedState = stateManager.getState("trigger-1");
          expect(updatedState?.status).toBe("disabled");
          expect(updatedState?.updatedAt).toBeGreaterThan(originalUpdatedAt);
          resolve();
        }, 1);
      });
    });

    it("Test to increase trigger count: increaseTriggerCount correct incremental count", () => {
      const originalState = stateManager.getState("trigger-1");
      const originalUpdatedAt = originalState!.updatedAt;

      // Wait for 1 millisecond to ensure the timestamps are different.
      return new Promise<void>(resolve => {
        setTimeout(() => {
          stateManager.incrementTriggerCount("trigger-1");

          const updatedState = stateManager.getState("trigger-1");
          expect(updatedState?.triggerCount).toBe(1);
          expect(updatedState?.updatedAt).toBeGreaterThan(originalUpdatedAt);
          resolve();
        }, 1);
      });
    });

    it("Test multiple increases in trigger count: correctly accrued", () => {
      stateManager.incrementTriggerCount("trigger-1");
      stateManager.incrementTriggerCount("trigger-1");
      stateManager.incrementTriggerCount("trigger-1");

      const state = stateManager.getState("trigger-1");
      expect(state?.triggerCount).toBe(3);
    });

    it("Test for updating a non-existent state: an error should be thrown", () => {
      expect(() => {
        stateManager.updateStatus("non-existent", "disabled");
      }).toThrow(NotFoundError);
    });

    it("Test to add non-existent state: an error should be thrown", () => {
      expect(() => {
        stateManager.incrementTriggerCount("non-existent");
      }).toThrow(NotFoundError);
    });
  });

  describe("Snapshot function", () => {
    beforeEach(() => {
      const states: TriggerRuntimeState[] = [
        {
          triggerId: "trigger-1",
          executionId: "test-thread",
          workflowId: "workflow-123",
          status: "enabled",
          triggerCount: 5,
          updatedAt: Date.now(),
        },
        {
          triggerId: "trigger-2",
          executionId: "test-thread",
          workflowId: "workflow-123",
          status: "disabled",
          triggerCount: 10,
          updatedAt: Date.now(),
        },
      ];

      states.forEach(state => stateManager.register(state));
    });

    it("Test creating a snapshot: createSnapshot creates a deep copy of the state", () => {
      const snapshot = stateManager.createSnapshot();

      expect(snapshot).toBeDefined();
      expect(snapshot.size).toBe(2);
      expect(snapshot.get("trigger-1")).toEqual(stateManager.getState("trigger-1"));
      expect(snapshot.get("trigger-2")).toEqual(stateManager.getState("trigger-2"));
    });

    it("Test recovery snapshot: restoreFromSnapshot correct recovery state", () => {
      const snapshot = stateManager.createSnapshot();

      // Modify the current state.
      stateManager.updateStatus("trigger-1", "disabled");
      stateManager.incrementTriggerCount("trigger-2");

      // Recover from a snapshot
      stateManager.restoreFromSnapshot(snapshot);

      // Verify the restored state.
      const state1 = stateManager.getState("trigger-1");
      const state2 = stateManager.getState("trigger-2");
      expect(state1?.status).toBe("enabled");
      expect(state1?.triggerCount).toBe(5);
      expect(state2?.triggerCount).toBe(10);
    });

    it("Testing snapshot independence: snapshot modifications do not affect the original state", () => {
      const snapshot = stateManager.createSnapshot();

      // Modify the snapshot
      const snapshotState = snapshot.get("trigger-1");
      if (snapshotState) {
        snapshotState.status = "disabled";
        snapshotState.triggerCount = 100;
      }

      // The original state should not be modified.
      const originalState = stateManager.getState("trigger-1");
      expect(originalState?.status).toBe("enabled");
      expect(originalState?.triggerCount).toBe(5);
    });

    it("Test restoring an empty snapshot: all states should be cleared", () => {
      const emptySnapshot = new Map<string, TriggerRuntimeState>();

      stateManager.restoreFromSnapshot(emptySnapshot);

      expect(stateManager.size()).toBe(0);
      expect(stateManager.getAllStates().size).toBe(0);
    });

    it("Test snapshot contains all states: the snapshot should contain all registered states", () => {
      // Add more states
      const additionalState: TriggerRuntimeState = {
        triggerId: "trigger-3",
        executionId: "test-thread",
        workflowId: "workflow-123",
        status: "triggered",
        triggerCount: 2,
        updatedAt: Date.now(),
      };

      stateManager.register(additionalState);

      const snapshot = stateManager.createSnapshot();

      expect(snapshot.size).toBe(3);
      expect(snapshot.has("trigger-1")).toBe(true);
      expect(snapshot.has("trigger-2")).toBe(true);
      expect(snapshot.has("trigger-3")).toBe(true);
    });

    it("Test restoring snapshots with mismatched execution IDs: an error should be thrown", () => {
      const invalidSnapshot = new Map<string, TriggerRuntimeState>();
      invalidSnapshot.set("trigger-1", {
        triggerId: "trigger-1",
        executionId: "different-thread", // Does not match
        workflowId: "workflow-123",
        status: "enabled",
        triggerCount: 0,
        updatedAt: Date.now(),
      });

      expect(() => {
        stateManager.restoreFromSnapshot(invalidSnapshot);
      }).toThrow(RuntimeValidationError);
    });
  });

  describe("Queries and deletions", () => {
    beforeEach(() => {
      const states: TriggerRuntimeState[] = [
        {
          triggerId: "trigger-1",
          executionId: "test-thread",
          workflowId: "workflow-123",
          status: "enabled",
          triggerCount: 0,
          updatedAt: Date.now(),
        },
        {
          triggerId: "trigger-2",
          executionId: "test-thread",
          workflowId: "workflow-123",
          status: "disabled",
          triggerCount: 5,
          updatedAt: Date.now(),
        },
      ];

      states.forEach(state => stateManager.register(state));
    });

    it("Test getting state: getState returns the correct state", () => {
      const state1 = stateManager.getState("trigger-1");
      const state2 = stateManager.getState("trigger-2");

      expect(state1).toBeDefined();
      expect(state1?.triggerId).toBe("trigger-1");
      expect(state1?.status).toBe("enabled");
      expect(state1?.triggerCount).toBe(0);

      expect(state2).toBeDefined();
      expect(state2?.triggerId).toBe("trigger-2");
      expect(state2?.status).toBe("disabled");
      expect(state2?.triggerCount).toBe(5);
    });

    it("Test to get non-existent state: return undefined", () => {
      const state = stateManager.getState("non-existent");

      expect(state).toBeUndefined();
    });

    it("Test checks for existence: hasState correctly returns whether the state exists or not", () => {
      expect(stateManager.hasState("trigger-1")).toBe(true);
      expect(stateManager.hasState("trigger-2")).toBe(true);
      expect(stateManager.hasState("non-existent")).toBe(false);
    });

    it("Test deletion state: deleteState correct deletion state", () => {
      expect(stateManager.hasState("trigger-1")).toBe(true);

      stateManager.deleteState("trigger-1");

      expect(stateManager.hasState("trigger-1")).toBe(false);
      expect(stateManager.getState("trigger-1")).toBeUndefined();
    });

    it("Test for deletion of non-existent state: an error should be thrown", () => {
      expect(() => {
        stateManager.deleteState("non-existent");
      }).toThrow(NotFoundError);
    });

    it("Test to get all states: getAllStates returns a read-only copy of all states", () => {
      const allStates = stateManager.getAllStates();

      expect(allStates.size).toBe(2);
      expect(allStates.get("trigger-1")).toEqual(stateManager.getState("trigger-1"));
      expect(allStates.get("trigger-2")).toEqual(stateManager.getState("trigger-2"));

      // Verify that what is returned is a copy (modifying the copy should not affect the original state).
      const stateCopy = allStates.get("trigger-1");
      if (stateCopy) {
        stateCopy.status = "disabled";
      }

      expect(stateManager.getState("trigger-1")?.status).toBe("enabled");
    });

    it("Tests the number of acquired states: size returns the number of states", () => {
      expect(stateManager.size()).toBe(2);

      stateManager.register({
        triggerId: "trigger-3",
        executionId: "test-thread",
        workflowId: "workflow-123",
        status: "enabled",
        triggerCount: 0,
        updatedAt: Date.now(),
      });

      expect(stateManager.size()).toBe(3);

      stateManager.deleteState("trigger-1");

      expect(stateManager.size()).toBe(2);
    });
  });

  describe("Clear function", () => {
    it("Test cleanup resources: cleanup method clears all states", () => {
      const states: TriggerRuntimeState[] = [
        {
          triggerId: "trigger-1",
          executionId: "test-thread",
          workflowId: "workflow-123",
          status: "enabled",
          triggerCount: 0,
          updatedAt: Date.now(),
        },
        {
          triggerId: "trigger-2",
          executionId: "test-thread",
          workflowId: "workflow-123",
          status: "disabled",
          triggerCount: 5,
          updatedAt: Date.now(),
        },
      ];

      states.forEach(state => stateManager.register(state));

      expect(stateManager.size()).toBe(2);

      stateManager.cleanup();

      expect(stateManager.size()).toBe(0);
      expect(stateManager.getAllStates().size).toBe(0);
      expect(stateManager.hasState("trigger-1")).toBe(false);
      expect(stateManager.hasState("trigger-2")).toBe(false);
    });

    it("Test clearing empty status manager: should process normally", () => {
      expect(stateManager.size()).toBe(0);

      stateManager.cleanup();

      expect(stateManager.size()).toBe(0);
    });
  });

  describe("Getter Methods", () => {
    it("Test to get execution ID: getExecutionId returns the correct execution ID", () => {
      expect(stateManager.getExecutionId()).toBe("test-thread");
    });

    it("Test to get workflow ID: getWorkflowId returns the correct workflow ID", () => {
      expect(stateManager.getWorkflowId()).toBe("workflow-123");
    });

    it("Test to get unset workflow ID: return null", () => {
      const managerWithoutWorkflowId = new TriggerState("test-thread");
      // Do not set workflowId.

      expect(managerWithoutWorkflowId.getWorkflowId()).toBeNull();
    });

    it("Test setting the workflow ID: setWorkflowId correctly sets the workflow ID", () => {
      stateManager.setWorkflowId("new-workflow-456");

      expect(stateManager.getWorkflowId()).toBe("new-workflow-456");
    });
  });

  describe("Boundary situation", () => {
    it("Test registration mass status: should be able to handle correctly", () => {
      const stateCount = 100;

      for (let i = 0; i < stateCount; i++) {
        const state: TriggerRuntimeState = {
          triggerId: `trigger-${i}`,
          executionId: "test-thread",
          workflowId: "workflow-123",
          status: "enabled",
          triggerCount: i,
          updatedAt: Date.now(),
        };

        stateManager.register(state);
      }

      expect(stateManager.size()).toBe(stateCount);
      expect(stateManager.getAllStates().size).toBe(stateCount);
    });

    it("Test concurrent status updates: status should be updated correctly", () => {
      const state: TriggerRuntimeState = {
        triggerId: "trigger-1",
        executionId: "test-thread",
        workflowId: "workflow-123",
        status: "enabled",
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      stateManager.register(state);

      // Increase the trigger count multiple times
      for (let i = 0; i < 10; i++) {
        stateManager.incrementTriggerCount("trigger-1");
      }

      expect(stateManager.getState("trigger-1")?.triggerCount).toBe(10);
    });
  });
});
