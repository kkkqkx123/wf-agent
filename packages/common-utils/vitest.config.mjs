/**
 * Vitest 配置文件 (ESM)
 * 适用于 common-utils 包
 */

import { defineConfig } from "vitest/config";

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
      include: ["src/**/*.ts"],
      exclude: ["**/*.d.ts", "**/*.test.ts", "**/*.spec.ts", "**/index.ts"],
    },

    // 测试超时时间
    testTimeout: 30000,

    // 详细输出
    reporters: ["verbose"],

    // 清除模拟
    clearMocks: true,
    restoreMocks: true,

    // 全局配置
    globals: true,
  },
});
