import { beforeAll, afterAll } from "vitest";
import { mkdirSync, rmSync, existsSync, readdirSync } from "fs";
import { resolve } from "path";

// Set environment variables to prevent CLI parsing during tests
process.env["CLI_MODE"] = "programmatic";
process.env["TEST_MODE"] = "true";

// Set the root directory for test output.
const TEST_OUTPUT_DIR = resolve(__dirname, "../outputs");
const TEST_STORAGE_DIR = resolve(__dirname, "../storage");
const MAX_OUTPUT_AGE_DAYS = 7;

(globalThis as unknown as { TEST_OUTPUT_DIR: string }).TEST_OUTPUT_DIR = TEST_OUTPUT_DIR;

/**
 * Clean up old test outputs to prevent disk space accumulation
 * Removes output directories older than MAX_OUTPUT_AGE_DAYS
 */
function cleanupOldOutputs(): void {
  if (!existsSync(TEST_OUTPUT_DIR)) {
    return;
  }

  const now = Date.now();
  const maxAge = MAX_OUTPUT_AGE_DAYS * 24 * 60 * 60 * 1000;

  try {
    const entries = readdirSync(TEST_OUTPUT_DIR, { withFileTypes: true });
    for (const entry of entries) {
      if (entry.isDirectory()) {
        const dirPath = resolve(TEST_OUTPUT_DIR, entry.name);
        try {
          const stats = readdirSync(dirPath, { withFileTypes: true }).map(d => {
            try {
              return { name: d.name, mtime: readdirSync(resolve(dirPath, d.name), { withFileTypes: true })[0]?.mtime ?? new Date(0) };
            } catch {
              return { name: d.name, mtime: new Date(0) };
            }
          });
          const latestMtime = stats.length > 0 ? stats.reduce((latest, s) => s.mtime > latest ? s.mtime : latest, stats[0].mtime) : new Date(0);
          const age = now - latestMtime.getTime();
          
          if (age > maxAge) {
            rmSync(dirPath, { recursive: true, force: true });
            console.log(`[CLEANUP] Removed old test output: ${entry.name}`);
          }
        } catch (error) {
          // Skip directories we can't read
        }
      }
    }
  } catch (error) {
    console.warn(`[CLEANUP] Failed to cleanup old outputs: ${error}`);
  }
}

/**
 * Clean up old test storage to prevent disk space accumulation
 */
function cleanupOldStorage(): void {
  if (!existsSync(TEST_STORAGE_DIR)) {
    return;
  }

  const now = Date.now();
  const maxAge = MAX_OUTPUT_AGE_DAYS * 24 * 60 * 60 * 1000;

  try {
    const entries = readdirSync(TEST_STORAGE_DIR, { withFileTypes: true });
    for (const entry of entries) {
      if (entry.isDirectory()) {
        const dirPath = resolve(TEST_STORAGE_DIR, entry.name);
        try {
          const stats = readdirSync(dirPath, { withFileTypes: true }).map(d => {
            try {
              return { name: d.name, mtime: readdirSync(resolve(dirPath, d.name), { withFileTypes: true })[0]?.mtime ?? new Date(0) };
            } catch {
              return { name: d.name, mtime: new Date(0) };
            }
          });
          const latestMtime = stats.length > 0 ? stats.reduce((latest, s) => s.mtime > latest ? s.mtime : latest, stats[0].mtime) : new Date(0);
          const age = now - latestMtime.getTime();
          
          if (age > maxAge) {
            rmSync(dirPath, { recursive: true, force: true });
            console.log(`[CLEANUP] Removed old test storage: ${entry.name}`);
          }
        } catch (error) {
          // Skip directories we can't read
        }
      }
    }
  } catch (error) {
    console.warn(`[CLEANUP] Failed to cleanup old storage: ${error}`);
  }
}

beforeAll(async () => {
  // Initialize the test environment.
  console.log("Setting up integration test environment...");
  console.log(
    `Test output directory: ${(globalThis as unknown as { TEST_OUTPUT_DIR: string }).TEST_OUTPUT_DIR}`,
  );

  // Clean up old test outputs and storage (older than 7 days)
  cleanupOldOutputs();
  cleanupOldStorage();

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
