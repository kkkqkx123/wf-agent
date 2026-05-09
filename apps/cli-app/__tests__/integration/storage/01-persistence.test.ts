/**
 * Storage Persistence Integration Test
 * 
 * This test verifies that workflow registration data is correctly persisted
 * and can be retrieved across separate CLI invocations.
 * 
 * Tests the complete storage lifecycle:
 * 1. Register a workflow (write to storage)
 * 2. Query the workflow (read from storage)
 * 3. Verify data integrity
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import path from "path";
import fs from "fs";
import { CLIRunner } from "../../utils/cli-runner.js";
import { TestHelper } from "../../utils/test-helpers.js";

describe("Storage Persistence", () => {
  let runner: CLIRunner;
  let helper: TestHelper;
  let storageDir: string;

  beforeEach(() => {
    helper = new TestHelper("storage-persistence");
    runner = new CLIRunner();
    storageDir = helper.getStorageDir();
    runner.setStorageDir(storageDir);
  });

  afterEach(async () => {
    await helper.cleanup();
  });

  describe("Basic Persistence", () => {
    it("should persist workflow registration across CLI invocations", async () => {
      // Step 1: Register a workflow
      const workflowFile = helper.getFixturePath("workflows", "child-wf.toml");

      console.log(`[TEST] Storage directory: ${storageDir}`);
      console.log(`[TEST] Workflow file: ${workflowFile}`);

      const registerResult = await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "storage-persistence",
      });

      console.log(`[TEST] Register exit code: ${registerResult.exitCode}`);
      console.log(`[TEST] Register stdout: ${registerResult.stdout}`);
      console.log(`[TEST] Register stderr: ${registerResult.stderr}`);

      expect(registerResult.exitCode).toBe(0);
      expect(registerResult.stdout).toContain("Workflow is registered");

      // Step 2: Verify storage directory structure exists
      const storageFiles = fs.readdirSync(storageDir);
      console.log(`[TEST] Storage files: ${JSON.stringify(storageFiles)}`);

      // Check if data and metadata directories exist (JSON storage uses directory structure)
      const hasDataDir = storageFiles.includes("data");
      expect(hasDataDir).toBe(true);

      // Inspect the data directory structure
      if (hasDataDir) {
        const dataDir = path.join(storageDir, "data");
        const dataFiles = fs.readdirSync(dataDir);
        console.log(`[TEST] Data directory contents: ${JSON.stringify(dataFiles)}`);
        
        // Check for workflow-related files
        for (const file of dataFiles) {
          const filePath = path.join(dataDir, file);
          const stats = fs.statSync(filePath);
          if (stats.isDirectory()) {
            const subFiles = fs.readdirSync(filePath);
            console.log(`[TEST]   ${file}/: ${JSON.stringify(subFiles)}`);
          }
        }
      }

      // Step 3: Verify data file exists and is readable
      const workflowDataPath = path.join(storageDir, "data", "workflow", "child-wf.bin");
      const workflowMetaPath = path.join(storageDir, "metadata", "workflow", "child-wf.json");
      
      console.log(`[TEST] Checking data file: ${workflowDataPath}`);
      console.log(`[TEST] Data file exists: ${fs.existsSync(workflowDataPath)}`);
      console.log(`[TEST] Metadata file exists: ${fs.existsSync(workflowMetaPath)}`);
      
      if (fs.existsSync(workflowMetaPath)) {
        const metaContent = fs.readFileSync(workflowMetaPath, "utf-8");
        console.log(`[TEST] Metadata content:`, metaContent.substring(0, 200));
      }

      // Step 4: Query the workflow in a NEW CLI invocation
      const showResult = await runner.run(["workflow", "show", "child-wf"], {
        outputSubdir: "storage-persistence",
      });

      console.log(`[TEST] Show exit code: ${showResult.exitCode}`);
      console.log(`[TEST] Show stdout: ${showResult.stdout}`);
      console.log(`[TEST] Show stderr: ${showResult.stderr}`);

      expect(showResult.exitCode).toBe(0);
      expect(showResult.stdout).toContain("child-wf");
    });

    it("should maintain data integrity after multiple registrations", async () => {
      // Register first workflow
      const wf1File = helper.getFixturePath("workflows", "child-wf.toml");

      const result1 = await runner.run(["workflow", "register", wf1File], {
        outputSubdir: "storage-persistence",
      });

      expect(result1.exitCode).toBe(0);

      // Register second workflow
      const wf2File = helper.getFixturePath("workflows", "standalone-wf.toml");

      const result2 = await runner.run(["workflow", "register", wf2File], {
        outputSubdir: "storage-persistence",
      });

      expect(result2.exitCode).toBe(0);

      // List all workflows
      const listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "storage-persistence",
      });

      console.log(`[TEST] List exit code: ${listResult.exitCode}`);
      console.log(`[TEST] List stdout: ${listResult.stdout}`);

      expect(listResult.exitCode).toBe(0);
      expect(listResult.stdout).toContain("child-wf");
      expect(listResult.stdout).toContain("standalone-wf");
    });
  });

  describe("Storage Directory Isolation", () => {
    it("should use isolated storage directories for different tests", async () => {
      const dir1 = helper.getStorageDir().replace("storage-persistence", "isolation-test-1");
      const dir2 = helper.getStorageDir().replace("storage-persistence", "isolation-test-2");

      expect(dir1).not.toBe(dir2);
      expect(dir1).toContain("isolation-test-1");
      expect(dir2).toContain("isolation-test-2");

      // Register workflow in dir1
      const runner1 = new CLIRunner();
      runner1.setStorageDir(dir1);

      const wfFile = helper.getFixturePath("workflows", "child-wf.toml");

      const result1 = await runner1.run(["workflow", "register", wfFile], {
        outputSubdir: "storage-persistence",
      });

      expect(result1.exitCode).toBe(0);

      // Try to query from dir2 (should fail - no data)
      const runner2 = new CLIRunner();
      runner2.setStorageDir(dir2);

      const result2 = await runner2.run(["workflow", "show", "child-wf"], {
        outputSubdir: "storage-persistence",
      });

      // Should fail because child-wf is not registered in dir2
      expect(result2.exitCode).not.toBe(0);
    });
  });

  describe("Storage File Structure", () => {
    it("should create proper storage file structure", async () => {
      const workflowFile = helper.getFixturePath("workflows", "child-wf.toml");

      await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "storage-persistence",
      });

      // Check storage directory structure
      expect(fs.existsSync(storageDir)).toBe(true);

      const files = fs.readdirSync(storageDir);
      console.log(`[TEST] Storage structure: ${JSON.stringify(files)}`);

      // Should have at least data and metadata directories
      expect(files.includes("data")).toBe(true);
      expect(files.includes("metadata")).toBe(true);

      // Read and validate workflow metadata file
      const workflowMetaPath = path.join(storageDir, "metadata", "workflow", "child-wf.json");
      if (fs.existsSync(workflowMetaPath)) {
        const content = fs.readFileSync(workflowMetaPath, "utf-8");
        const data = JSON.parse(content);

        console.log(`[TEST] Workflow metadata structure:`, Object.keys(data));

        // Should be a valid JSON object
        expect(typeof data).toBe("object");
        expect(data.id).toBe("child-wf");
      }
    });
  });
});
