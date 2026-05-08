/**
 * Integration tests for File IO Service
 * Validates file-based Human Relay workflow and display output functionality
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { FileIOService } from "../../src/services/io/file-io-service.js";
import * as fs from "fs/promises";
import * as path from "path";

describe("File IO Service", () => {
  let fileIO: FileIOService;
  const testBaseDir = ".wf-agent-test";

  beforeEach(async () => {
    // Clean up any existing test directory
    try {
      await fs.rm(testBaseDir, { recursive: true, force: true });
    } catch (error) {
      // Ignore if doesn't exist
    }

    fileIO = new FileIOService({ baseDir: testBaseDir });
  });

  afterEach(async () => {
    // Clean up test directory
    try {
      await fs.rm(testBaseDir, { recursive: true, force: true });
    } catch (error) {
      // Ignore cleanup errors
    }
  });

  describe("Session Path Generation", () => {
    it("should generate correct session paths", () => {
      const sessionId = "test-session-123";
      const paths = fileIO.getSessionPaths(sessionId);

      expect(paths.functional.humanRelayOutput).toContain(testBaseDir);
      expect(paths.functional.humanRelayOutput).toContain("function");
      expect(paths.functional.humanRelayOutput).toContain(sessionId);
      expect(paths.functional.humanRelayOutput).toContain("human-relay-output.txt");

      expect(paths.functional.humanRelayInput).toContain("human-relay-input.txt");
      expect(paths.display.output).toContain("display");
      expect(paths.display.output).toContain("output.md");
    });

    it("should generate different paths for different sessions", () => {
      const paths1 = fileIO.getSessionPaths("session-1");
      const paths2 = fileIO.getSessionPaths("session-2");

      expect(paths1.functional.humanRelayOutput).not.toEqual(paths2.functional.humanRelayOutput);
      expect(paths1.display.output).not.toEqual(paths2.display.output);
    });
  });

  describe("Human Relay Output", () => {
    it("should write human relay prompt to functional file", async () => {
      const sessionId = "test-session-1";
      const prompt = "Please provide your response to the following question...";

      await fileIO.writeHumanRelayOutput({
        sessionId,
        content: prompt,
      });

      const paths = fileIO.getSessionPaths(sessionId);
      const content = await fs.readFile(paths.functional.humanRelayOutput, "utf-8");

      expect(content).toBe(prompt);
    });

    it("should create directories automatically", async () => {
      const sessionId = "nested/session/test";
      const prompt = "Test prompt";

      await fileIO.writeHumanRelayOutput({
        sessionId,
        content: prompt,
      });

      const paths = fileIO.getSessionPaths(sessionId);
      const exists = await fs.access(paths.functional.humanRelayOutput)
        .then(() => true)
        .catch(() => false);

      expect(exists).toBe(true);
    });

    it("should write pure text without formatting", async () => {
      const sessionId = "test-session-2";
      const prompt = "Line 1\nLine 2\nLine 3";

      await fileIO.writeHumanRelayOutput({
        sessionId,
        content: prompt,
      });

      const paths = fileIO.getSessionPaths(sessionId);
      const content = await fs.readFile(paths.functional.humanRelayOutput, "utf-8");

      // Should preserve exact format without adding markdown
      expect(content).toBe(prompt);
      expect(content).not.toContain("#");
      expect(content).not.toContain("**");
    });
  });

  describe("Display Output", () => {
    it("should create output.md with proper structure", async () => {
      const sessionId = "test-session-3";
      const sections = [
        {
          title: "工具调用结果: readFile",
          content: "[10:30:00] 工具 \"readFile\" 执行完成\n结果已保存",
        },
        {
          title: "节点完成: node-1",
          content: "[10:30:01] 节点 \"node-1\" 执行完成\n耗时: 150ms",
        },
      ];

      await fileIO.updateDisplayOutput({
        sessionId,
        sections,
        append: false,
      });

      const paths = fileIO.getSessionPaths(sessionId);
      const content = await fs.readFile(paths.display.output, "utf-8");

      expect(content).toContain("# Workflow Execution Output");
      expect(content).toContain("======");
      expect(content).toContain("## 工具调用结果: readFile");
      expect(content).toContain("## 节点完成: node-1");
      expect(content).toContain("══════════════════════════════");
    });

    it("should append sections when append=true", async () => {
      const sessionId = "test-session-4";

      // Write first section
      await fileIO.updateDisplayOutput({
        sessionId,
        sections: [
          {
            title: "First Section",
            content: "Content 1",
          },
        ],
        append: false,
      });

      // Append second section
      await fileIO.updateDisplayOutput({
        sessionId,
        sections: [
          {
            title: "Second Section",
            content: "Content 2",
          },
        ],
        append: true,
      });

      const paths = fileIO.getSessionPaths(sessionId);
      const content = await fs.readFile(paths.display.output, "utf-8");

      expect(content).toContain("First Section");
      expect(content).toContain("Second Section");
      expect(content).toContain("Content 1");
      expect(content).toContain("Content 2");
    });

    it("should overwrite when append=false", async () => {
      const sessionId = "test-session-5";

      // Write first content
      await fileIO.updateDisplayOutput({
        sessionId,
        sections: [
          {
            title: "Old Section",
            content: "Old Content",
          },
        ],
        append: false,
      });

      // Overwrite with new content
      await fileIO.updateDisplayOutput({
        sessionId,
        sections: [
          {
            title: "New Section",
            content: "New Content",
          },
        ],
        append: false,
      });

      const paths = fileIO.getSessionPaths(sessionId);
      const content = await fs.readFile(paths.display.output, "utf-8");

      expect(content).toContain("New Section");
      expect(content).toContain("New Content");
      expect(content).not.toContain("Old Section");
      expect(content).not.toContain("Old Content");
    });
  });

  describe("Human Relay Input Watcher", () => {
    it("should detect file changes and call onResponse", async () => {
      const sessionId = "test-session-6";
      const paths = fileIO.getSessionPaths(sessionId);

      let responseReceived = "";

      // Start watching
      fileIO.watchHumanRelayInput({
        sessionId,
        timeout: 5000,
        onResponse: (content) => {
          responseReceived = content;
        },
      });

      // Wait a bit for watcher to initialize
      await new Promise((resolve) => setTimeout(resolve, 100));

      // Write response to input file
      await fs.writeFile(paths.functional.humanRelayInput, "User response here", "utf-8");

      // Wait for watcher to detect change
      await new Promise((resolve) => setTimeout(resolve, 200));

      expect(responseReceived).toBe("User response here");
    });

    it("should timeout if no response received", async () => {
      const sessionId = "test-session-7";
      let timedOut = false;

      fileIO.watchHumanRelayInput({
        sessionId,
        timeout: 500, // Short timeout for testing
        onResponse: () => {},
        onTimeout: () => {
          timedOut = true;
        },
      });

      // Wait for timeout
      await new Promise((resolve) => setTimeout(resolve, 700));

      expect(timedOut).toBe(true);
    });

    it("should ignore empty content", async () => {
      const sessionId = "test-session-8";
      const paths = fileIO.getSessionPaths(sessionId);
      let responseReceived = false;

      fileIO.watchHumanRelayInput({
        sessionId,
        timeout: 2000,
        onResponse: () => {
          responseReceived = true;
        },
      });

      await new Promise((resolve) => setTimeout(resolve, 100));

      // Write empty content
      await fs.writeFile(paths.functional.humanRelayInput, "   \n  \n  ", "utf-8");

      await new Promise((resolve) => setTimeout(resolve, 200));

      // Should not trigger response for whitespace-only content
      expect(responseReceived).toBe(false);
    });
  });

  describe("Multi-Session Isolation", () => {
    it("should maintain separate files for different sessions", async () => {
      const session1 = "session-A";
      const session2 = "session-B";

      await fileIO.writeHumanRelayOutput({
        sessionId: session1,
        content: "Session A content",
      });

      await fileIO.writeHumanRelayOutput({
        sessionId: session2,
        content: "Session B content",
      });

      const paths1 = fileIO.getSessionPaths(session1);
      const paths2 = fileIO.getSessionPaths(session2);

      const content1 = await fs.readFile(paths1.functional.humanRelayOutput, "utf-8");
      const content2 = await fs.readFile(paths2.functional.humanRelayOutput, "utf-8");

      expect(content1).toBe("Session A content");
      expect(content2).toBe("Session B content");
      expect(content1).not.toBe(content2);
    });
  });

  describe("Error Handling", () => {
    it("should handle invalid session IDs gracefully", async () => {
      const sessionId = "";
      
      // Should not throw
      await expect(
        fileIO.writeHumanRelayOutput({
          sessionId,
          content: "test",
        })
      ).resolves.not.toThrow();
    });

    it("should handle very long prompts", async () => {
      const sessionId = "test-session-9";
      const longPrompt = "A".repeat(10000);

      await expect(
        fileIO.writeHumanRelayOutput({
          sessionId,
          content: longPrompt,
        })
      ).resolves.not.toThrow();

      const paths = fileIO.getSessionPaths(sessionId);
      const content = await fs.readFile(paths.functional.humanRelayOutput, "utf-8");
      expect(content.length).toBe(10000);
    });
  });
});
