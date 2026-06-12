import { describe, it, expect } from "vitest";
import type { TriggerAction } from "@wf-agent/types";
import { sendNotificationHandler } from "../send-notification-handler.js";

describe("send-notification-handler", () => {
  it("should send notification successfully with all parameters", async () => {
    const action: TriggerAction = {
      type: "send_notification",
      parameters: {
        message: "Test notification",
        recipients: ["user-1", "user-2"],
        level: "warning",
      },
    };

    const result = await sendNotificationHandler(action, "trigger-1");

    expect(result.success).toBe(true);
    expect(result.triggerId).toBe("trigger-1");
    expect(result.result).toEqual({
      message: "Notification sent successfully",
      notification: {
        message: "Test notification",
        recipients: ["user-1", "user-2"],
        level: "warning",
        timestamp: expect.any(Number),
        status: "sent",
      },
    });
  });

  it("should send notification with default parameters", async () => {
    const action: TriggerAction = {
      type: "send_notification",
      parameters: { message: "Simple message" },
    };

    const result = await sendNotificationHandler(action, "trigger-2");

    expect(result.success).toBe(true);
    expect(result.result).toEqual({
      message: "Notification sent successfully",
      notification: {
        message: "Simple message",
        recipients: [],
        level: "info",
        timestamp: expect.any(Number),
        status: "sent",
      },
    });
  });

  it("should fail when message is missing", async () => {
    const action: TriggerAction = {
      type: "send_notification",
      parameters: {},
    };

    const result = await sendNotificationHandler(action, "trigger-3");

    expect(result.success).toBe(false);
    expect(result.error).toContain("message is required");
  });

  it("should handle empty message", async () => {
    const action: TriggerAction = {
      type: "send_notification",
      parameters: { message: "" },
    };

    const result = await sendNotificationHandler(action, "trigger-4");

    expect(result.success).toBe(false);
    expect(result.error).toContain("message is required");
  });
});