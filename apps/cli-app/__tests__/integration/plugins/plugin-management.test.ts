/**
 * Plugin Management Integration Tests
 *
 * Tests the `plugin` command group:
 * - list: list all plugins
 * - show: show a specific plugin
 * - load: load a plugin from a path
 * - find: find a plugin by source
 * - activate: activate a plugin
 * - deactivate: deactivate a plugin
 * - reload: hot-reload a plugin
 * - unload: unload a plugin
 * - config show: show plugin configuration
 * - config update: update plugin configuration
 *
 * Note: These tests depend on the plugin system being enabled in the test config.
 * If plugins are not enabled, the commands will return appropriate error messages.
 */

import { describe, it, expect, beforeEach, afterEach, beforeAll } from "vitest";
import { CLIRunner, TestHelper, createTestHelper } from "../../__shared/index.js";
import { resolve } from "path";

describe("Plugin Management Tests", () => {
  let helper: TestHelper;
  let runner: CLIRunner;
  const testOutputDir = resolve(__dirname, "../../outputs/plugin-management");

  beforeAll(() => {
    runner = new CLIRunner(undefined, testOutputDir);
  });

  beforeEach(() => {
    helper = createTestHelper("plugin-management", testOutputDir);
    runner.setStorageDir(helper.getStorageDir());
  });

  afterEach(async () => {
    await helper.cleanup();
    runner.setStorageDir(undefined);
  });

  describe("1. Plugin List", () => {
    it("should list plugins (may be empty when no plugins are loaded)", async () => {
      const result = await runner.run(["plugin", "list"], {
        outputSubdir: "plugin-management",
      });

      // The plugin list command should succeed even if no plugins are loaded
      expect(result.exitCode).toBe(0);
    });

    it("should list plugins with table format", async () => {
      const result = await runner.run(["plugin", "list", "--table"], {
        outputSubdir: "plugin-management",
      });

      expect(result.exitCode).toBe(0);
    });
  });

  describe("2. Plugin Show", () => {
    it("should fail to show a non-existent plugin", async () => {
      const result = await runner.run(["plugin", "show", "non-existent-plugin"], {
        outputSubdir: "plugin-management",
      });

      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("plugin-show");
    });
  });

  describe("3. Plugin Load", () => {
    it("should fail to load a plugin from a non-existent path", async () => {
      const result = await runner.run(["plugin", "load", "/non/existent/plugin/path.js"], {
        outputSubdir: "plugin-management",
      });

      // Either plugin system is not available, or the path doesn't exist
      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("4. Plugin Find", () => {
    it("should fail to find a non-existent plugin", async () => {
      const result = await runner.run(["plugin", "find", "non-existent-plugin"], {
        outputSubdir: "plugin-management",
      });

      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("5. Plugin Activate", () => {
    it("should fail to activate a non-existent plugin", async () => {
      const result = await runner.run(["plugin", "activate", "non-existent-plugin"], {
        outputSubdir: "plugin-management",
      });

      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("6. Plugin Deactivate", () => {
    it("should fail to deactivate a non-existent plugin", async () => {
      const result = await runner.run(["plugin", "deactivate", "non-existent-plugin"], {
        outputSubdir: "plugin-management",
      });

      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("7. Plugin Reload", () => {
    it("should fail to reload a non-existent plugin", async () => {
      const result = await runner.run(["plugin", "reload", "non-existent-plugin"], {
        outputSubdir: "plugin-management",
      });

      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("8. Plugin Unload", () => {
    it("should fail to unload a non-existent plugin", async () => {
      const result = await runner.run(["plugin", "unload", "non-existent-plugin"], {
        outputSubdir: "plugin-management",
      });

      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("9. Plugin Config Show", () => {
    it("should fail to show config for a non-existent plugin", async () => {
      const result = await runner.run(["plugin", "config", "show", "non-existent-plugin"], {
        outputSubdir: "plugin-management",
      });

      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("10. Plugin Config Update", () => {
    it("should fail to update config for a non-existent plugin", async () => {
      const result = await runner.run(
        ["plugin", "config", "update", "non-existent-plugin", "key=value"],
        { outputSubdir: "plugin-management" },
      );

      expect(result.exitCode).not.toBe(0);
    });

    it("should fail when no config values are provided", async () => {
      const result = await runner.run(
        ["plugin", "config", "update", "some-plugin"],
        { outputSubdir: "plugin-management" },
      );

      expect(result.exitCode).not.toBe(0);
    });
  });
});
