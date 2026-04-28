/**
 * Vitest 集成测试配置文件 (ESM)
 */

import { defineConfig } from "vitest/config";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  test: {
    // 测试环境
    environment: "node",

    // 测试文件匹配模式
    include: ["**/__tests__/integration/**/*.test.ts"],

    // 排除文件
    exclude: ["node_modules", "dist", "coverage", "**/*.d.ts"],

    // 覆盖率配置
    coverage: {
      provider: "v8",
      reporter: ["text", "lcov", "html"],
      include: ["src/**/*.ts"],
      exclude: ["**/*.d.ts", "**/*.test.ts", "**/*.spec.ts", "**/index.ts"],
    },

    // 集成测试超时时间
    testTimeout: 60000,

    // 清理超时时间
    teardownTimeout: 10000,

    // 详细输出
    reporters: ["verbose"],

    // 清除模拟
    clearMocks: true,
    restoreMocks: true,

    // 全局配置
    globals: true,

    // Setup 文件
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
