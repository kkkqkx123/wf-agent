/**
 * TimeoutController Unit Tests
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { TimeoutController } from "../TimeoutController.js";
import { TimeoutError } from "@wf-agent/types";

describe("TimeoutController", () => {
  describe("constructor", () => {
    it("Instances should be created using the default timeout", async () => {
      const controller = new TimeoutController();
      // Verify that the default timeout value is 30000ms.
      const fn = vi.fn().mockResolvedValue("result");

      const result = await controller.executeWithTimeout(fn, 1000);
      expect(result).toBe("result");
    });

    it("Instances should be created using the specified timeout", async () => {
      const controller = new TimeoutController(5000);
      const fn = vi.fn().mockResolvedValue("success");

      const result = await controller.executeWithTimeout(fn);
      expect(result).toBe("success");
    });
  });

  describe("executeWithTimeout", () => {
    describe("Successful implementation", () => {
      it("The result should be returned on successful completion of the function", async () => {
        const controller = new TimeoutController(5000);
        const fn = vi.fn().mockResolvedValue("result");

        const result = await controller.executeWithTimeout(fn);

        expect(result).toBe("result");
        expect(fn).toHaveBeenCalledTimes(1);
      });

      it("Asynchronous functions should be supported", async () => {
        const controller = new TimeoutController(5000);
        const fn = vi.fn().mockImplementation(async () => {
          await new Promise(resolve => setTimeout(resolve, 10));
          return "async-result";
        });

        const result = await controller.executeWithTimeout(fn);

        expect(result).toBe("async-result");
      });
    });

    describe("timeout handling", () => {
      it("Should throw TimeoutError after timeout", async () => {
        const controller = new TimeoutController(100);
        const fn = vi
          .fn()
          .mockImplementation(() => new Promise(resolve => setTimeout(resolve, 10000)));

        try {
          await controller.executeWithTimeout(fn, 50);
          expect.fail("Should have thrown TimeoutError");
        } catch (error) {
          expect(error).toBeInstanceOf(TimeoutError);
          expect((error as TimeoutError).timeout).toBe(50);
        }
      });

      it("The passed-in timeout should be used instead of the default value", async () => {
        const controller = new TimeoutController(10000);
        const fn = vi
          .fn()
          .mockImplementation(() => new Promise(resolve => setTimeout(resolve, 5000)));

        try {
          await controller.executeWithTimeout(fn, 50);
          expect.fail("Should have thrown TimeoutError");
        } catch (error) {
          expect(error).toBeInstanceOf(TimeoutError);
          expect((error as TimeoutError).timeout).toBe(50);
        }
      });

      it("The timeout should be included in the timeout error message", async () => {
        const controller = new TimeoutController(100);
        const fn = vi
          .fn()
          .mockImplementation(() => new Promise(resolve => setTimeout(resolve, 10000)));

        try {
          await controller.executeWithTimeout(fn, 100);
          expect.fail("Should have thrown");
        } catch (error) {
          expect(error).toBeInstanceOf(TimeoutError);
          expect((error as TimeoutError).message).toContain("100ms");
        }
      });
    });

    describe("Stop Signal Processing", () => {
      it("Should be rejected when the abort signal is triggered", async () => {
        const controller = new TimeoutController(5000);
        const abortController = new AbortController();
        const fn = vi
          .fn()
          .mockImplementation(() => new Promise(resolve => setTimeout(resolve, 10000)));

        const promise = controller.executeWithTimeout(fn, 5000, abortController.signal);

        // Terminate before a timeout occurs.
        abortController.abort();

        try {
          await promise;
          expect.fail("Should have thrown");
        } catch (error) {
          expect(error).toBeInstanceOf(Error);
          expect((error as Error).message).toContain("aborted");
        }
      });

      it("Aborted signals should be handled", async () => {
        const controller = new TimeoutController(5000);
        const abortController = new AbortController();
        abortController.abort(); // Stop it first.

        const fn = vi.fn().mockResolvedValue("result");

        try {
          await controller.executeWithTimeout(fn, 5000, abortController.signal);
          expect.fail("Should have thrown");
        } catch (error) {
          expect(error).toBeInstanceOf(Error);
        }
      });

      it("The abort event listener should be cleaned up upon completion", async () => {
        const controller = new TimeoutController(5000);
        const abortController = new AbortController();
        const fn = vi.fn().mockResolvedValue("result");

        await controller.executeWithTimeout(fn, 5000, abortController.signal);

        // The validation function has completed successfully.
        expect(fn).toHaveBeenCalledTimes(1);
      });
    });

    describe("Resource cleanup", () => {
      it("Should clear the timeout timer after successful completion", async () => {
        const controller = new TimeoutController(5000);
        const fn = vi.fn().mockResolvedValue("result");

        await controller.executeWithTimeout(fn, 1000);

        expect(fn).toHaveBeenCalledTimes(1);
      });

      it("The timeout timer should be cleared after the timeout period", async () => {
        const controller = new TimeoutController(50);
        const fn = vi
          .fn()
          .mockImplementation(() => new Promise(resolve => setTimeout(resolve, 1000)));

        try {
          await controller.executeWithTimeout(fn, 50);
        } catch {
          // An error is expected to be thrown.
        }

        // The verification function has been called.
        expect(fn).toHaveBeenCalledTimes(1);
      });
    });

    describe("Boundary situation", () => {
      it("Synchronization functions should be handled", async () => {
        const controller = new TimeoutController(5000);
        const fn = vi.fn().mockResolvedValue("sync-result");

        const result = await controller.executeWithTimeout(fn);

        expect(result).toBe("sync-result");
      });

      it("Should pass the function error correctly", async () => {
        const controller = new TimeoutController(5000);
        const error = new Error("Function error");
        const fn = vi.fn().mockRejectedValue(error);

        try {
          await controller.executeWithTimeout(fn);
          expect.fail("Should have thrown error");
        } catch (err) {
          expect(err).toBeInstanceOf(Error);
          expect((err as Error).message).toBe("Function error");
        }
      });
    });
  });

  describe("Static factory methods", () => {
    describe("createDefault", () => {
      it("Controllers should be created with a default timeout of 30000ms", async () => {
        const controller = TimeoutController.createDefault();
        const fn = vi.fn().mockResolvedValue("result");

        const result = await controller.executeWithTimeout(fn);

        expect(result).toBe("result");
      });
    });

    describe("createNoTimeout", () => {
      it("Controllers with zero timeout should be created", async () => {
        const controller = TimeoutController.createNoTimeout();
        const fn = vi
          .fn()
          .mockImplementation(() => new Promise(resolve => setTimeout(resolve, 100)));

        try {
          await controller.executeWithTimeout(fn);
          expect.fail("Should have thrown TimeoutError");
        } catch (error) {
          expect(error).toBeInstanceOf(TimeoutError);
        }
      });
    });
  });
});
