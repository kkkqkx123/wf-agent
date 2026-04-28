/**
 * TimeoutController 测试
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { TimeoutController } from "../TimeoutController.js";

describe("TimeoutController", () => {
  let timeoutController: TimeoutController;

  beforeEach(() => {
    timeoutController = new TimeoutController();
  });

  describe("constructor", () => {
    it("Instances should be created using the default configuration", () => {
      const controller = new TimeoutController();
      expect(controller).toBeInstanceOf(TimeoutController);
    });

    it("Instances should be created using a custom configuration", () => {
      const controller = new TimeoutController({
        defaultTimeout: 60000,
      });
      expect(controller).toBeInstanceOf(TimeoutController);
    });
  });

  describe("executeWithTimeout", () => {
    it("Functions that should successfully execute fast completion", async () => {
      const fn = async () => {
        return "success";
      };

      const result = await timeoutController.executeWithTimeout(fn, 5000);
      expect(result).toBe("success");
    });

    it("Should throw an error on timeout", async () => {
      const fn = async () => {
        await new Promise(resolve => setTimeout(resolve, 2000));
        return "success";
      };

      await expect(timeoutController.executeWithTimeout(fn, 100)).rejects.toThrow(
        "Execution timeout after 100ms",
      );
    });

    it("The default timeout should be used", async () => {
      const controller = new TimeoutController({ defaultTimeout: 100 });
      const fn = async () => {
        await new Promise(resolve => setTimeout(resolve, 200));
        return "success";
      };

      await expect(controller.executeWithTimeout(fn)).rejects.toThrow(
        "Execution timeout after 100ms",
      );
    });

    it("AbortSignal should be handled before the timeout period", async () => {
      const fn = async () => {
        await new Promise(resolve => setTimeout(resolve, 5000));
        return "success";
      };

      const abortController = new AbortController();
      // Abort after 100ms
      setTimeout(() => abortController.abort(), 100);

      await expect(
        timeoutController.executeWithTimeout(fn, 5000, abortController.signal),
      ).rejects.toThrow(/aborted/);
    }, 10000);

    it("The timeout should be ignored when the function executes successfully", async () => {
      const fn = async () => {
        await new Promise(resolve => setTimeout(resolve, 50));
        return "success";
      };

      const result = await timeoutController.executeWithTimeout(fn, 5000);
      expect(result).toBe("success");
    });

    it("Errors thrown by functions should be handled", async () => {
      const fn = async () => {
        throw new Error("Function error");
      };

      await expect(timeoutController.executeWithTimeout(fn, 5000)).rejects.toThrow(
        "Function error",
      );
    });

    it("AbortSignal should be handled before the timeout period", async () => {
      const fn = async () => {
        await new Promise(resolve => setTimeout(resolve, 5000));
        return "success";
      };

      const abortController = new AbortController();
      // Abort after 100ms
      setTimeout(() => abortController.abort(), 100);

      await expect(
        timeoutController.executeWithTimeout(fn, 5000, abortController.signal),
      ).rejects.toThrow("Execution aborted by signal");
    });

    it("should support returning Promise objects", async () => {
      const fn = () => {
        return Promise.resolve("promise result");
      };

      const result = await timeoutController.executeWithTimeout(fn, 5000);
      expect(result).toBe("promise result");
    });

    it("should support returning non-Promise objects", async () => {
      const fn = async () => {
        return "direct result";
      };

      const result = await timeoutController.executeWithTimeout(fn, 5000);
      expect(result).toBe("direct result");
    });
  });

  describe("createDefault", () => {
    it("A default timeout controller instance should be created", () => {
      const defaultController = TimeoutController.createDefault();
      expect(defaultController).toBeInstanceOf(TimeoutController);
    });
  });
});
