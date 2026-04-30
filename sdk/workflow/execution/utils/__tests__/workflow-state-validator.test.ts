/**
 * WorkflowStateValidator Unit Tests
 * Testing the workflow execution state transition validation tool functions
 */

import { describe, it, expect } from "vitest";
import {
  isValidTransition,
  validateTransition,
  getAllowedTransitions,
  isTerminalStatus,
  isActiveStatus,
} from "../workflow-state-validator.js";
import { RuntimeValidationError } from "@wf-agent/types";
import type { WorkflowExecutionStatus } from "@wf-agent/types";

describe("isValidTransition", () => {
  it("The CREATED -> RUNNING conversion should be allowed.", () => {
    expect(isValidTransition("CREATED", "RUNNING")).toBe(true);
  });

  it("Should allow RUNNING -> PAUSED conversion", () => {
    expect(isValidTransition("RUNNING", "PAUSED")).toBe(true);
  });

  it("Should allow RUNNING -> COMPLETED conversion", () => {
    expect(isValidTransition("RUNNING", "COMPLETED")).toBe(true);
  });

  it("The RUNNING -> FAILED transition should be allowed.", () => {
    expect(isValidTransition("RUNNING", "FAILED")).toBe(true);
  });

  it("The RUNNING -> CANCELLED transition should be allowed.", () => {
    expect(isValidTransition("RUNNING", "CANCELLED")).toBe(true);
  });

  it("Should allow RUNNING -> TIMEOUT transitions", () => {
    expect(isValidTransition("RUNNING", "TIMEOUT")).toBe(true);
  });

  it("Should allow PAUSED -> RUNNING conversion", () => {
    expect(isValidTransition("PAUSED", "RUNNING")).toBe(true);
  });

  it("Should allow PAUSED -> CANCELLED conversion", () => {
    expect(isValidTransition("PAUSED", "CANCELLED")).toBe(true);
  });

  it("Should allow PAUSED -> TIMEOUT conversion", () => {
    expect(isValidTransition("PAUSED", "TIMEOUT")).toBe(true);
  });

  it("CREATED -> COMPLETED conversion should not be allowed", () => {
    expect(isValidTransition("CREATED", "COMPLETED")).toBe(false);
  });

  it("COMPLETED -> RUNNING conversion should not be allowed", () => {
    expect(isValidTransition("COMPLETED", "RUNNING")).toBe(false);
  });

  it("FAILED -> RUNNING conversions should not be allowed", () => {
    expect(isValidTransition("FAILED", "RUNNING")).toBe(false);
  });

  it("The CANCELLED -> RUNNING transition should not be allowed!", () => {
    expect(isValidTransition("CANCELLED", "RUNNING")).toBe(false);
  });

  it("TIMEOUT -> RUNNING transitions should not be allowed", () => {
    expect(isValidTransition("TIMEOUT", "RUNNING")).toBe(false);
  });

  it("CREATED -> PAUSED conversions should not be allowed", () => {
    expect(isValidTransition("CREATED", "PAUSED")).toBe(false);
  });

  it("Returns false for invalid current state", () => {
    expect(isValidTransition("INVALID" as any, "RUNNING")).toBe(false);
  });
});

describe("validateTransition", () => {
  it("Do not throw an error when a state transition is legal", () => {
    expect(() => validateTransition("wfexec-1", "CREATED", "RUNNING")).not.toThrow();
    expect(() => validateTransition("wfexec-1", "RUNNING", "COMPLETED")).not.toThrow();
    expect(() => validateTransition("wfexec-1", "PAUSED", "RUNNING")).not.toThrow();
  });

  it("Throws RuntimeValidationError when a state transition is not legal.", () => {
    expect(() => validateTransition("wfexec-1", "CREATED", "COMPLETED")).toThrow(
      RuntimeValidationError,
    );

    expect(() => validateTransition("wfexec-1", "CREATED", "COMPLETED")).toThrow(
      "Invalid state transition: CREATED -> COMPLETED",
    );
  });

  it("Error messages contain current and target status", () => {
    try {
      validateTransition("wfexec-1", "COMPLETED", "RUNNING");
      expect.fail("Should have thrown");
    } catch (error) {
      expect(error).toBeInstanceOf(RuntimeValidationError);
      expect((error as RuntimeValidationError).message).toContain("COMPLETED");
      expect((error as RuntimeValidationError).message).toContain("RUNNING");
    }
  });

  it("Error messages contain operation and field information", () => {
    try {
      validateTransition("wfexec-1", "FAILED", "RUNNING");
      expect.fail("Should have thrown");
    } catch (error) {
      expect(error).toBeInstanceOf(RuntimeValidationError);
      const validationError = error as RuntimeValidationError;
      expect(validationError.context?.["operation"]).toBe("validateStateTransition");
      expect(validationError.context?.["field"]).toBe("workflowExecution.status");
    }
  });
});

describe("getAllowedTransitions", () => {
  it("Returns all conversions allowed by the CREATED status.", () => {
    const transitions = getAllowedTransitions("CREATED");
    expect(transitions).toEqual(["RUNNING"]);
    expect(transitions).toHaveLength(1);
  });

  it("Returns all transitions allowed by the RUNNING state.", () => {
    const transitions = getAllowedTransitions("RUNNING");
    expect(transitions).toEqual(["PAUSED", "COMPLETED", "FAILED", "CANCELLED", "TIMEOUT"]);
    expect(transitions).toHaveLength(5);
  });

  it("Returns all transitions allowed by the PAUSED status", () => {
    const transitions = getAllowedTransitions("PAUSED");
    expect(transitions).toEqual(["RUNNING", "CANCELLED", "TIMEOUT"]);
    expect(transitions).toHaveLength(3);
  });

  it("Returns all transitions allowed by the COMPLETED state (empty array)", () => {
    const transitions = getAllowedTransitions("COMPLETED");
    expect(transitions).toEqual([]);
    expect(transitions).toHaveLength(0);
  });

  it("Returns all transitions allowed by the FAILED status (empty array)", () => {
    const transitions = getAllowedTransitions("FAILED");
    expect(transitions).toEqual([]);
    expect(transitions).toHaveLength(0);
  });

  it("Returns all transitions allowed by the CANCELLED status (empty array)", () => {
    const transitions = getAllowedTransitions("CANCELLED");
    expect(transitions).toEqual([]);
    expect(transitions).toHaveLength(0);
  });

  it("Returns all transitions allowed by the TIMEOUT state (empty array)", () => {
    const transitions = getAllowedTransitions("TIMEOUT");
    expect(transitions).toEqual([]);
    expect(transitions).toHaveLength(0);
  });

  it("Returns an empty array for invalid states", () => {
    const transitions = getAllowedTransitions("INVALID" as any);
    expect(transitions).toEqual([]);
  });
});

describe("isTerminalStatus", () => {
  it("COMPLETED should be recognized as a terminated state", () => {
    expect(isTerminalStatus("COMPLETED")).toBe(true);
  });

  it("FAILED should be recognized as a terminated state", () => {
    expect(isTerminalStatus("FAILED")).toBe(true);
  });

  it("CANCELLED should be recognized as terminated", () => {
    expect(isTerminalStatus("CANCELLED")).toBe(true);
  });

  it("TIMEOUT should be recognized as a termination state", () => {
    expect(isTerminalStatus("TIMEOUT")).toBe(true);
  });

  it("CREATED should not be recognized as a terminated state.", () => {
    expect(isTerminalStatus("CREATED")).toBe(false);
  });

  it("RUNNING should not be recognized as a termination state.", () => {
    expect(isTerminalStatus("RUNNING")).toBe(false);
  });

  it("PAUSED should not be recognized as a terminated state", () => {
    expect(isTerminalStatus("PAUSED")).toBe(false);
  });

  it("Returns false for invalid states", () => {
    expect(isTerminalStatus("INVALID" as any)).toBe(false);
  });
});

describe("isActiveStatus", () => {
  it("RUNNING should be recognized as active", () => {
    expect(isActiveStatus("RUNNING")).toBe(true);
  });

  it("PAUSED should be recognized as active", () => {
    expect(isActiveStatus("PAUSED")).toBe(true);
  });

  it("CREATED should not be recognized as active.", () => {
    expect(isActiveStatus("CREATED")).toBe(false);
  });

  it("COMPLETED should not be recognized as active.", () => {
    expect(isActiveStatus("COMPLETED")).toBe(false);
  });

  it("FAILED should not be recognized as active", () => {
    expect(isActiveStatus("FAILED")).toBe(false);
  });

  it("CANCELLED should not be recognized as active.", () => {
    expect(isActiveStatus("CANCELLED")).toBe(false);
  });

  it("TIMEOUT should not be recognized as active.", () => {
    expect(isActiveStatus("TIMEOUT")).toBe(false);
  });

  it("Returns false for invalid states", () => {
    expect(isActiveStatus("INVALID" as any)).toBe(false);
  });
});

describe("State transition rule integrity", () => {
  const allStatuses: WorkflowExecutionStatus[] = [
    "CREATED",
    "RUNNING",
    "PAUSED",
    "COMPLETED",
    "FAILED",
    "CANCELLED",
    "TIMEOUT",
  ];

  it("All states have defined transition rules", () => {
    for (const status of allStatuses) {
      const transitions = getAllowedTransitions(status);
      expect(Array.isArray(transitions)).toBe(true);
    }
  });

  it("The termination state does not allow any transitions", () => {
    const terminalStatuses: WorkflowExecutionStatus[] = ["COMPLETED", "FAILED", "CANCELLED", "TIMEOUT"];
    for (const status of terminalStatuses) {
      const transitions = getAllowedTransitions(status);
      expect(transitions).toHaveLength(0);
    }
  });

  it("All state transitions can be validated by isValidTransition", () => {
    for (const fromStatus of allStatuses) {
      const allowedTransitions = getAllowedTransitions(fromStatus);
      for (const toStatus of allowedTransitions) {
        expect(isValidTransition(fromStatus, toStatus)).toBe(true);
      }

      // Verify that the prohibited conversions are not allowed.
      for (const toStatus of allStatuses) {
        if (!allowedTransitions.includes(toStatus)) {
          expect(isValidTransition(fromStatus, toStatus)).toBe(false);
        }
      }
    }
  });
});
