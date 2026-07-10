import { beforeAll, afterAll } from "vitest";
import { mkdirSync } from "fs";
import { resolve } from "path";

process.env["CLI_MODE"] = "programmatic";
process.env["TEST_MODE"] = "true";

const TEST_OUTPUT_DIR = resolve(__dirname, "../outputs");

(globalThis as unknown as { TEST_OUTPUT_DIR: string }).TEST_OUTPUT_DIR = TEST_OUTPUT_DIR;

beforeAll(async () => {
  console.log("Setting up integration test environment...");
  console.log(
    `Test output directory: ${(globalThis as unknown as { TEST_OUTPUT_DIR: string }).TEST_OUTPUT_DIR}`,
  );

  mkdirSync((globalThis as unknown as { TEST_OUTPUT_DIR: string }).TEST_OUTPUT_DIR, {
    recursive: true,
  });
});

afterAll(async () => {
  console.log("Cleaning up integration test environment...");
  console.log(
    `Test output saved to: ${(globalThis as unknown as { TEST_OUTPUT_DIR: string }).TEST_OUTPUT_DIR}`,
  );
});
