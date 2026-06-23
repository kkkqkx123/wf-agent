// Vitest Configuration for Integration Tests
// Integration tests verify module-to-module collaboration with lightweight
// dependencies (Memory storage backends). They sit between Unit tests
// (single function/class) and E2E tests (full SDK lifecycle).
// Test timeout: 15s (between unit's 10s and e2e's 60s)

import { defineConfig } from "vitest/config";
import { fileURLToPath } from "url";
import { dirname, resolve } from "path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

export default defineConfig({
  test: {
    root: __dirname,
    environment: "node",
    include: ["__tests__/integration/**/*.int.test.ts"],
    exclude: ["node_modules", "dist", "coverage", "**/*.d.ts", "**/__shared/**"],
    testTimeout: 15000,
    hookTimeout: 10000,
    reporters: ["verbose"],
    clearMocks: true,
    restoreMocks: true,
    globals: true,
    sequence: {
      concurrent: false,
    },
    pool: "forks",
    poolOptions: {
      forks: {
        singleFork: true,
      },
    },
  },
  resolve: {
    alias: {
      "@": resolve(__dirname, "."),
      "@utils": resolve(__dirname, "utils"),
      "@core": resolve(__dirname, "core"),
      "@api": resolve(__dirname, "api"),
      "@sdk/core": resolve(__dirname, "core"),
      "@sdk/core/(.*)": resolve(__dirname, "core/$1"),
      "@sdk/shared": resolve(__dirname, "shared"),
      "@sdk/shared/(.*)": resolve(__dirname, "shared/$1"),
      "@sdk/services": resolve(__dirname, "services"),
      "@sdk/services/(.*)": resolve(__dirname, "services/$1"),
      "@sdk/utils": resolve(__dirname, "utils"),
      "@sdk/utils/(.*)": resolve(__dirname, "utils/$1"),
      "@sdk/workflow": resolve(__dirname, "workflow"),
      "@sdk/workflow/(.*)": resolve(__dirname, "workflow/$1"),
      "@sdk/agent": resolve(__dirname, "agent"),
      "@sdk/agent/(.*)": resolve(__dirname, "agent/$1"),
      "@sdk/api": resolve(__dirname, "api"),
      "@sdk/api/(.*)": resolve(__dirname, "api/$1"),
      "@wf-agent/types": resolve(__dirname, "../packages/types/src"),
      "@wf-agent/types/(.*)": resolve(__dirname, "../packages/types/src/$1"),
      "@wf-agent/storage": resolve(__dirname, "../packages/storage/src"),
      "@wf-agent/storage/(.*)": resolve(__dirname, "../packages/storage/src/$1"),
      "@wf-agent/common-utils": resolve(__dirname, "../packages/common-utils/src"),
      "@wf-agent/common-utils/(.*)": resolve(__dirname, "../packages/common-utils/src/$1"),
      "@wf-agent/tool-executors": resolve(__dirname, "../packages/tool-executors/src"),
      "@wf-agent/tool-executors/(.*)": resolve(__dirname, "../packages/tool-executors/src/$1"),
    },
  },
});
