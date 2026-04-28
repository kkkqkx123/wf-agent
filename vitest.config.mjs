/**
 * Vitest 配置文件 (ESM)
 * 适用于 monorepo 架构
 */

import { defineConfig } from "vitest/config";
import { fileURLToPath } from "url";
import { dirname, resolve } from "path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

export default defineConfig({
  test: {
    // 测试环境
    environment: "node",

    // 测试文件匹配模式
    include: ["**/__tests__/**/*.ts", "**/?(*.)+(spec|test).ts"],

    // 排除文件
    exclude: ["node_modules", "dist", "coverage", "**/*.d.ts"],

    // 覆盖率配置
    coverage: {
      provider: "v8",
      reporter: ["text", "lcov", "html"],
      include: ["packages/*/src/**/*.ts", "sdk/*/src/**/*.ts"],
      exclude: ["**/*.d.ts", "**/*.test.ts", "**/*.spec.ts", "**/index.ts"],
      // 覆盖率阈值
      thresholds: {
        branches: 80,
        functions: 80,
        lines: 80,
        statements: 80,
      },
    },

    // 测试超时时间（毫秒）
    testTimeout: 30000,

    // 钩子超时时间（毫秒）
    hookTimeout: 30000,

    // 详细输出
    reporters: ["verbose"],

    // 清除模拟
    clearMocks: true,
    restoreMocks: true,

    // 全局配置
    globals: true,

    // 自动退出配置
    // 防止测试进程因未关闭的连接而挂起
    disableConsoleInterception: false,

    // 别名配置
    alias: {
      "@wf-agent/common-utils": resolve(__dirname, "packages/common-utils/src"),
      "@wf-agent/types": resolve(__dirname, "packages/types/src"),
      "@wf-agent/tool-executors": resolve(__dirname, "packages/tool-executors/src"),
      "@wf-agent/sdk": resolve(__dirname, "sdk/src"),
    },
  },
});
