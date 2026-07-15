/**
 * Vitest 配置文件 (ESM)
 * 适用于 SDK 模块
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
    exclude: ["node_modules", "dist", "coverage", "**/*.d.ts", "**/__shared/**"],

    // 覆盖率配置
    coverage: {
      provider: "v8",
      reporter: ["text", "lcov", "html"],
      include: ["core/**/*.ts", "api/**/*.ts", "utils/**/*.ts"],
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

  // 解析配置
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
      "@sdk/di": resolve(__dirname, "di"),
      "@sdk/di/(.*)": resolve(__dirname, "di/$1"),
      "@sdk/plugin": resolve(__dirname, "plugin"),
      "@sdk/metrics": resolve(__dirname, "metrics"),
      "@sdk/metrics/(.*)": resolve(__dirname, "metrics/$1"),
      "@sdk/resources": resolve(__dirname, "resources"),
      "@wf-agent/types": resolve(__dirname, "../types/src"),
      "@wf-agent/types/(.*)": resolve(__dirname, "../types/src/$1"),
      "@wf-agent/common-utils": resolve(__dirname, "../common-utils/src"),
      "@wf-agent/common-utils/id-utils": resolve(
        __dirname,
        "../common-utils/src/utils/id-utils",
      ),
      "@wf-agent/common-utils/timestamp-utils": resolve(
        __dirname,
        "../common-utils/src/utils/timestamp-utils",
      ),
      "@wf-agent/common-utils/result-utils": resolve(
        __dirname,
        "../common-utils/src/utils/result-utils",
      ),
      "@wf-agent/common-utils/token-encoder": resolve(
        __dirname,
        "../common-utils/src/utils/token-encoder",
      ),
      "@wf-agent/common-utils/(.*)": resolve(__dirname, "../common-utils/src/$1"),
      "@wf-agent/tool-executors": resolve(__dirname, "../tool-executors/src"),
      "@wf-agent/tool-executors/(.*)": resolve(__dirname, "../tool-executors/src/$1"),
      "@wf-agent/storage": resolve(__dirname, "../storage/src"),
      "@wf-agent/storage/(.*)": resolve(__dirname, "../storage/src/$1"),
    },
  },
});
