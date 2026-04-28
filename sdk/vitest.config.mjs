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
    exclude: ["node_modules", "dist", "coverage", "**/*.d.ts"],

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
      "@wf-agent/types": resolve(__dirname, "../packages/types/src"),
      "@wf-agent/types/(.*)": resolve(__dirname, "../packages/types/src/$1"),
      "@wf-agent/common-utils": resolve(__dirname, "../packages/common-utils/src"),
      "@wf-agent/common-utils/id-utils": resolve(
        __dirname,
        "../packages/common-utils/src/utils/id-utils",
      ),
      "@wf-agent/common-utils/timestamp-utils": resolve(
        __dirname,
        "../packages/common-utils/src/utils/timestamp-utils",
      ),
      "@wf-agent/common-utils/result-utils": resolve(
        __dirname,
        "../packages/common-utils/src/utils/result-utils",
      ),
      "@wf-agent/common-utils/token-encoder": resolve(
        __dirname,
        "../packages/common-utils/src/utils/token-encoder",
      ),
      "@wf-agent/common-utils/(.*)": resolve(__dirname, "../packages/common-utils/src/$1"),
      "@wf-agent/tool-executors": resolve(__dirname, "../packages/tool-executors/src"),
      "@wf-agent/tool-executors/(.*)": resolve(__dirname, "../packages/tool-executors/src/$1"),
    },
  },
});
