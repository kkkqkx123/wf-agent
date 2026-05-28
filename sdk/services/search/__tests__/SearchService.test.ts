/**
 * Tests for SearchService
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { SearchService } from "../SearchService.js";

// Mock RipgrepExecutor
const mockListFiles = vi.fn();
const mockSearchContent = vi.fn();
const mockInitialize = vi.fn();

vi.mock("../../executors/implementations/ripgrep/index.js", () => ({
  RipgrepExecutor: vi.fn(function MockRipgrepExecutor() {
    return {
      initialize: mockInitialize,
      listFiles: mockListFiles,
      searchContent: mockSearchContent,
    };
  }),
}));

describe("SearchService", () => {
  let searchService: SearchService;

  beforeEach(() => {
    vi.clearAllMocks();
    searchService = new SearchService();
  });

  describe("initialize", () => {
    it("should initialize the ripgrep executor", async () => {
      mockInitialize.mockResolvedValue(undefined);

      await searchService.initialize();

      expect(mockInitialize).toHaveBeenCalledOnce();
    });
  });

  describe("listAllFiles", () => {
    it("should return files from ripgrep executor", async () => {
      mockListFiles.mockResolvedValue([
        { path: "src/index.ts", type: "file", label: "index.ts" },
        { path: "src/utils", type: "folder", label: "utils" },
      ]);

      const result = await searchService.listAllFiles({
        workspacePath: "/test",
      });

      expect(mockListFiles).toHaveBeenCalledWith({
        workspacePath: "/test",
        limit: 10000,
      });
      expect(result).toEqual([
        { path: "src/index.ts", type: "file", label: "index.ts" },
        { path: "src/utils", type: "folder", label: "utils" },
      ]);
    });

    it("should respect custom limit", async () => {
      mockListFiles.mockResolvedValue([]);

      await searchService.listAllFiles({
        workspacePath: "/test",
        limit: 50,
      });

      expect(mockListFiles).toHaveBeenCalledWith({
        workspacePath: "/test",
        limit: 50,
      });
    });

    it("should return empty array on error", async () => {
      mockListFiles.mockRejectedValue(new Error("list error"));

      const result = await searchService.listAllFiles({
        workspacePath: "/test",
      });

      expect(result).toEqual([]);
    });
  });

  describe("searchFiles", () => {
    it("should return top files when query is empty", async () => {
      mockListFiles.mockResolvedValue([
        { path: "a.ts", type: "file", label: "a.ts" },
        { path: "b.ts", type: "file", label: "b.ts" },
        { path: "c.ts", type: "file", label: "c.ts" },
      ]);

      const result = await searchService.searchFiles({
        query: "",
        workspacePath: "/test",
        limit: 2,
      });

      expect(result).toHaveLength(2);
    });

    it("should fuzzy search files matching the query", async () => {
      mockListFiles.mockResolvedValue([
        { path: "src/components/Button.tsx", type: "file", label: "Button.tsx" },
        { path: "src/components/Input.tsx", type: "file", label: "Input.tsx" },
        { path: "README.md", type: "file", label: "README.md" },
      ]);

      const result = await searchService.searchFiles({
        query: "but",
        workspacePath: "/test",
      });

      // "Button.tsx" should match "but" fuzzily
      expect(result.length).toBeGreaterThan(0);
      expect(result[0].path).toBe("src/components/Button.tsx");
    });

    it("should return empty array when no files match the query", async () => {
      mockListFiles.mockResolvedValue([
        { path: "foo.ts", type: "file", label: "foo.ts" },
        { path: "bar.ts", type: "file", label: "bar.ts" },
      ]);

      const result = await searchService.searchFiles({
        query: "zzzz",
        workspacePath: "/test",
      });

      expect(result).toEqual([]);
    });

    it("should respect the limit parameter", async () => {
      const files = Array.from({ length: 50 }, (_, i) => ({
        path: `file${i}.ts`,
        type: "file" as const,
        label: `file${i}.ts`,
      }));
      mockListFiles.mockResolvedValue(files);

      const result = await searchService.searchFiles({
        query: "file",
        workspacePath: "/test",
        limit: 5,
      });

      expect(result.length).toBeLessThanOrEqual(5);
    });

    it("should handle listAllFiles error gracefully", async () => {
      mockListFiles.mockRejectedValue(new Error("list error"));

      const result = await searchService.searchFiles({
        query: "test",
        workspacePath: "/test",
      });

      expect(result).toEqual([]);
    });
  });

  describe("searchContent", () => {
    it("should delegate to ripgrep executor", async () => {
      mockSearchContent.mockResolvedValue("search results");

      const result = await searchService.searchContent({
        cwd: "/test",
        directoryPath: "/test/src",
        pattern: "function",
        filePattern: "*.ts",
        contextLines: 2,
        maxResults: 50,
      });

      expect(mockSearchContent).toHaveBeenCalledWith({
        cwd: "/test",
        directoryPath: "/test/src",
        pattern: "function",
        filePattern: "*.ts",
        contextLines: 2,
        maxResults: 50,
      });
      expect(result).toBe("search results");
    });

    it("should call with minimal options", async () => {
      mockSearchContent.mockResolvedValue("results");

      await searchService.searchContent({
        cwd: "/test",
        directoryPath: "/test/src",
        pattern: "import",
      });

      expect(mockSearchContent).toHaveBeenCalledWith({
        cwd: "/test",
        directoryPath: "/test/src",
        pattern: "import",
      });
    });
  });
});
