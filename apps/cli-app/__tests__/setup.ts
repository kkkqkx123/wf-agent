import { beforeAll, afterAll } from "vitest";
import { mkdirSync } from "fs";
import { resolve } from "path";

// Set the root directory for test output.
const TEST_OUTPUT_DIR = resolve(__dirname, "../outputs");
(globalThis as unknown as { TEST_OUTPUT_DIR: string }).TEST_OUTPUT_DIR = TEST_OUTPUT_DIR;

beforeAll(async () => {
  // Initialize the test environment.
  console.log("Setting up integration test environment...");
  console.log(
    `Test output directory: ${(globalThis as unknown as { TEST_OUTPUT_DIR: string }).TEST_OUTPUT_DIR}`,
  );

  // Create a unified test output directory.
  mkdirSync((globalThis as unknown as { TEST_OUTPUT_DIR: string }).TEST_OUTPUT_DIR, {
    recursive: true,
  });

  // TODO: 配置 Mock SDK（如果需要）
  // setupMockSDK();
});

afterAll(async () => {
  // Clean up the test environment.
  console.log("Cleaning up integration test environment...");
  console.log(
    `Test output saved to: ${(globalThis as unknown as { TEST_OUTPUT_DIR: string }).TEST_OUTPUT_DIR}`,
  );

  // TODO: 清理全局资源（如果需要）
});
