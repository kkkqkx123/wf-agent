/**
 * Vitest 集成测试配置文件 (ESM)
 */

import { defineConfig } from "vitest/config";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";
import { baseTestConfig } from "../../vitest.config.base.mjs";

const __dirname = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  test: {
    ...baseTestConfig,

    // Test file matching pattern
    include: ["**/__tests__/integration/**/*.test.ts"],

    // Excluded files
    exclude: ["node_modules", "dist", "coverage", "**/*.d.ts"],

    // Test timeout (milliseconds)
    testTimeout: 60000,

    // Teardown timeout
    teardownTimeout: 10000,

    // Coverage configuration
    coverage: {
      provider: "v8",
      reporter: ["text", "lcov", "html"],
      include: ["src/**/*.ts"],
      exclude: ["**/*.d.ts", "**/*.test.ts", "**/*.spec.ts", "**/index.ts"],
    },

    // Setup files
    setupFiles: ["./__tests__/setup.ts"],
  },

  resolve: {
    alias: {
      "@wf-agent/sdk": resolve(__dirname, "../../sdk"),
      "@wf-agent/types": resolve(__dirname, "../../packages/types/src"),
      "@wf-agent/common-utils": resolve(__dirname, "../../packages/common-utils/src"),
      "@wf-agent/tool-executors": resolve(__dirname, "../../packages/tool-executors/src"),
    },
  },
});
