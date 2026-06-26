import { describe, it, expect, beforeEach } from "vitest";
import { RecoveryTransactionManager } from "../hierarchy/recovery-transaction.js";
import { CheckpointErrorHandler } from "../hierarchy/error-handler.js";
import type { CheckpointErrorHandlerConfig, CheckpointErrorContext } from "@wf-agent/types";

function createNoopLogger() {
  return {
    debug: () => {},
    info: () => {},
    warn: () => {},
    error: () => {},
  };
}

describe("RecoveryTransactionManager integration", () => {
  describe("CP-INT-10: normal commit lifecycle", () => {
    it("should commit successfully when all operations complete", async () => {
      const tx = new RecoveryTransactionManager("tx-1");
      const compensationCalls: string[] = [];

      tx.begin();
      tx.registerOperation({ entityId: "entity-1", action: "op1" });
      tx.registerOperation({ entityId: "entity-2", action: "op2" });
      tx.addCompensatingAction(async () => { compensationCalls.push("comp-1"); });
      tx.addCompensatingAction(async () => { compensationCalls.push("comp-2"); });

      tx.completeOperation("entity-1");
      tx.completeOperation("entity-2");

      const result = await tx.commit();

      expect(result.status).toBe("completed");
      expect(result.errors).toHaveLength(0);
      expect(compensationCalls).toHaveLength(0);
    });
  });

  describe("CP-INT-11: all-or-nothing rollback", () => {
    it("should call all compensations in reverse order on rollback", async () => {
      const tx = new RecoveryTransactionManager("tx-2", { rollbackStrategy: "all_or_nothing" });
      const compensationCalls: string[] = [];

      tx.begin();
      tx.registerOperation({ entityId: "e1", action: "create" });
      tx.registerOperation({ entityId: "e2", action: "update" });
      tx.registerOperation({ entityId: "e3", action: "delete" });
      tx.addCompensatingAction(async () => { compensationCalls.push("comp-e1"); });
      tx.addCompensatingAction(async () => { compensationCalls.push("comp-e2"); });
      tx.addCompensatingAction(async () => { compensationCalls.push("comp-e3"); });

      tx.completeOperation("e1");
      tx.completeOperation("e2");
      tx.failOperation("e3", "update failed");

      await tx.rollback();

      expect(tx.getStatus()).toBe("rolled_back");
      expect(compensationCalls).toEqual(["comp-e3", "comp-e2", "comp-e1"]);
    });
  });

  describe("CP-INT-12: best-effort partial rollback", () => {
    it("should continue rolling back even when a compensation fails", async () => {
      const tx = new RecoveryTransactionManager("tx-3", { rollbackStrategy: "best_effort" });
      const compensationCalls: string[] = [];

      tx.begin();
      tx.registerOperation({ entityId: "e1", action: "create" });
      tx.registerOperation({ entityId: "e2", action: "update" });

      tx.addCompensatingAction(async () => { compensationCalls.push("comp-e1"); });
      tx.addCompensatingAction(async () => {
        compensationCalls.push("comp-e2");
        throw new Error("Compensation for e2 failed");
      });

      tx.completeOperation("e1");
      tx.completeOperation("e2");

      tx.failOperation("e1", "e1 failed");
      await tx.rollback();

      expect(tx.getStatus()).toBe("rolled_back_with_errors");
      expect(compensationCalls).toEqual(["comp-e2", "comp-e1"]);
    });
  });
});

describe("CheckpointErrorHandler integration", () => {
  describe("CP-INT-13: three strategies", () => {
    let context: CheckpointErrorContext;

    beforeEach(() => {
      context = {
        checkpointId: "cp-1",
        entityId: "entity-1",
        triggerEvent: "test",
        operation: "create",
        timestamp: Date.now(),
      };
    });

    it("strict strategy should throw the error", async () => {
      const config: CheckpointErrorHandlerConfig = { strategy: "strict" };
      const handler = new CheckpointErrorHandler(config, createNoopLogger());
      const error = new Error("Fatal checkpoint error");

      await expect(
        handler.handleError(error, context),
      ).rejects.toThrow("Fatal checkpoint error");
    });

    it("warn strategy should return handled result without throwing", async () => {
      let logged = false;
      const handler = new CheckpointErrorHandler(
        { strategy: "warn" },
        { ...createNoopLogger(), warn: () => { logged = true; } },
      );
      const error = new Error("Warning: checkpoint issue");

      const result = await handler.handleError(error, context);
      expect(result.handled).toBe(true);
      expect(result.shouldRethrow).toBe(false);
      expect(logged).toBe(true);
    });

    it("silent strategy should return handled result with debug logging only", async () => {
      let logged: string[] = [];
      const handler = new CheckpointErrorHandler(
        { strategy: "silent" },
        { ...createNoopLogger(), debug: (msg: string) => { logged.push(msg); } },
      );

      const result = await handler.handleError(new Error("silent error"), context);
      expect(result.handled).toBe(true);
      expect(result.shouldRethrow).toBe(false);
      // silent logs at debug level
      expect(logged.length).toBeGreaterThanOrEqual(1);
    });

    it("should invoke onError callback when provided", async () => {
      const callbackArgs: unknown[] = [];
      const config: CheckpointErrorHandlerConfig = {
        strategy: "callback",
        onError: (err, ctx) => {
          callbackArgs.push(err, ctx);
        },
      };
      const handler = new CheckpointErrorHandler(config, createNoopLogger());
      const error = new Error("callback error");

      const result = await handler.handleError(error, context);
      expect(result.handled).toBe(true);
      expect(result.shouldRethrow).toBe(false);
      expect(callbackArgs).toHaveLength(2);
      expect(callbackArgs[0] instanceof Error).toBe(true);
      expect(callbackArgs[1]).toBe(context);
    });
  });
});
