import { describe, it } from "vitest";
import { TriggerTemplateRegistry } from "@/shared/registry/trigger-template-registry.js";
import { registerCustomTriggers } from "@/resources/custom/registration.js";
import { createTestCustomTrigger } from "./integration/resources/__shared/fixtures.js";

describe("debug", () => {
  it("should debug", () => {
    const triggerRegistry = new TriggerTemplateRegistry();
    const triggers = [createTestCustomTrigger("custom_trigger")];
    const result = registerCustomTriggers(triggerRegistry, triggers);
    console.log("success:", JSON.stringify(result.success));
    console.log("failures:", JSON.stringify(result.failures));
    console.log("has:", triggerRegistry.has("custom_trigger"));
    console.log("keys:", triggerRegistry.keys());
  });
});
