import { defineConfig } from "vitest/config";
import { fileURLToPath } from "url";
import { dirname, resolve } from "path";
import { baseTestConfig } from "../vitest.config.base.mjs";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

export default defineConfig({
  test: {
    ...baseTestConfig,
    root: __dirname,
    include: ["__tests__/e2e/**/*.e2e.test.ts"],
    exclude: ["node_modules", "dist", "coverage", "**/*.d.ts", "**/__shared/**"],
    testTimeout: 60000,
    hookTimeout: 30000,
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
