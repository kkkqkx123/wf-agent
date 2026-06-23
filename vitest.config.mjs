/**
 * Vitest Configuration File (ESM)
 * Root monorepo configuration for unit and integration tests
 */

import { defineConfig } from "vitest/config";
import { fileURLToPath } from "url";
import { dirname, resolve } from "path";
import { baseTestConfig } from "./vitest.config.base.mjs";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

export default defineConfig({
  test: {
    ...baseTestConfig,

    // Test file match patterns
    include: ["**/__tests__/**/*.ts", "**/?(*.)+(spec|test).ts"],

    // Excluded files
    exclude: ["node_modules", "dist", "coverage", "**/*.d.ts", "**/test-d/**/*"],

    // Test timeout (milliseconds)
    testTimeout: 30000,

    // Hook timeout (milliseconds)
    hookTimeout: 30000,

    // Coverage configuration
    coverage: {
      provider: "v8",
      reporter: ["text", "lcov", "html"],
      include: ["packages/*/src/**/*.ts"],
      exclude: ["**/*.d.ts", "**/*.test.ts", "**/*.spec.ts", "**/index.ts"],
      // Coverage thresholds
      thresholds: {
        branches: 80,
        functions: 80,
        lines: 80,
        statements: 80,
      },
    },

    // Auto-exit configuration
    // Prevent test processes from hanging due to unclosed connections
    disableConsoleInterception: false,

    // Alias configuration
    alias: {
      "@wf-agent/common-utils": resolve(__dirname, "packages/common-utils/src"),
      "@wf-agent/types": resolve(__dirname, "packages/types/src"),
      "@wf-agent/tool-executors": resolve(__dirname, "packages/tool-executors/src"),
      "@wf-agent/sdk": resolve(__dirname, "packages/sdk/src"),
      "@wf-agent/storage": resolve(__dirname, "packages/storage/src"),
      "@": resolve(__dirname, "packages/sdk"),
    },
  },
});
