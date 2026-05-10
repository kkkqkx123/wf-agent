// npx vitest utils/__tests__/path.spec.ts

import os from "os"
import * as path from "path"

import { arePathsEqual, getReadablePath, getWorkspacePath, getPathRelation, canChangeDirectory, parseCdCommand, removeCdFromCommand } from "../path"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

// Mock modules

vi.mock("vscode", () => ({
	window: {
		activeTextEditor: {
			document: {
				uri: { fsPath: "/test/workspaceFolder/file.ts" },
			},
		},
	},
	workspace: {
		workspaceFolders: [
			{
				uri: { fsPath: "/test/workspace" },
				name: "test",
				index: 0,
			},
		],
		getWorkspaceFolder: vi.fn().mockReturnValue({
			uri: {
				fsPath: "/test/workspaceFolder",
			},
		}),
	},
}))
describe("Path Utilities", () => {
	const originalPlatform = process.platform
	// Helper to mock VS Code configuration

	afterEach(() => {
		Object.defineProperty(process, "platform", {
			value: originalPlatform,
		})
	})

	describe("String.prototype.toPosix", () => {
		it("should convert backslashes to forward slashes", () => {
			const windowsPath = "C:\\Users\\test\\file.txt"
			expect(windowsPath.toPosix()).toBe("C:/Users/test/file.txt")
		})

		it("should not modify paths with forward slashes", () => {
			const unixPath = "/home/user/file.txt"
			expect(unixPath.toPosix()).toBe("/home/user/file.txt")
		})

		it("should preserve extended-length Windows paths", () => {
			const extendedPath = "\\\\?\\C:\\Very\\Long\\Path"
			expect(extendedPath.toPosix()).toBe("\\\\?\\C:\\Very\\Long\\Path")
		})
	})
	describe("getWorkspacePath", () => {
		it("should return the current workspace path", () => {
			const workspacePath = "/Users/test/project"
			expect(getWorkspacePath(workspacePath)).toBe("/Users/test/project")
		})

		it("should return undefined when outside a workspace", () => { })
	})
	describe("arePathsEqual", () => {
		describe("on Windows", () => {
			beforeEach(() => {
				Object.defineProperty(process, "platform", {
					value: "win32",
				})
			})

			it("should compare paths case-insensitively", () => {
				expect(arePathsEqual("C:\\Users\\Test", "c:\\users\\test")).toBe(true)
			})

			it("should handle different path separators", () => {
				// Convert both paths to use forward slashes after normalization
				const path1 = path.normalize("C:\\Users\\Test").replace(/\\/g, "/")
				const path2 = path.normalize("C:/Users/Test").replace(/\\/g, "/")
				expect(arePathsEqual(path1, path2)).toBe(true)
			})

			it("should normalize paths with ../", () => {
				// Convert both paths to use forward slashes after normalization
				const path1 = path.normalize("C:\\Users\\Test\\..\\Test").replace(/\\/g, "/")
				const path2 = path.normalize("C:\\Users\\Test").replace(/\\/g, "/")
				expect(arePathsEqual(path1, path2)).toBe(true)
			})
		})

		describe("on POSIX", () => {
			beforeEach(() => {
				Object.defineProperty(process, "platform", {
					value: "darwin",
				})
			})

			it("should compare paths case-sensitively", () => {
				expect(arePathsEqual("/Users/Test", "/Users/test")).toBe(false)
			})

			it("should normalize paths", () => {
				expect(arePathsEqual("/Users/./Test", "/Users/Test")).toBe(true)
			})

			it("should handle trailing slashes", () => {
				expect(arePathsEqual("/Users/Test/", "/Users/Test")).toBe(true)
			})
		})

		describe("edge cases", () => {
			it("should handle undefined paths", () => {
				expect(arePathsEqual(undefined, undefined)).toBe(true)
				expect(arePathsEqual("/test", undefined)).toBe(false)
				expect(arePathsEqual(undefined, "/test")).toBe(false)
			})

			it("should handle root paths with trailing slashes", () => {
				expect(arePathsEqual("/", "/")).toBe(true)
				expect(arePathsEqual("C:\\", "C:\\")).toBe(true)
			})
		})
	})

	describe("getReadablePath", () => {
		const homeDir = os.homedir()
		const desktop = path.join(homeDir, "Desktop")
		const cwd = process.platform === "win32" ? "C:\\Users\\test\\project" : "/Users/test/project"

		it("should return basename when path equals cwd", () => {
			expect(getReadablePath(cwd, cwd)).toBe("project")
		})

		it("should return relative path when inside cwd", () => {
			const filePath =
				process.platform === "win32"
					? "C:\\Users\\test\\project\\src\\file.txt"
					: "/Users/test/project/src/file.txt"
			expect(getReadablePath(cwd, filePath)).toBe("src/file.txt")
		})

		it("should return absolute path when outside cwd", () => {
			const filePath =
				process.platform === "win32" ? "C:\\Users\\test\\other\\file.txt" : "/Users/test/other/file.txt"
			expect(getReadablePath(cwd, filePath)).toBe(filePath.toPosix())
		})

		it("should handle Desktop as cwd", () => {
			const filePath = path.join(desktop, "file.txt")
			expect(getReadablePath(desktop, filePath)).toBe(filePath.toPosix())
		})

		it("should return empty string when relative path is undefined", () => {
			expect(getReadablePath(cwd)).toBe("")
		})

		it("should return cwd basename when relative path is empty string", () => {
			// Empty string resolves to cwd, which returns basename
			expect(getReadablePath(cwd, "")).toBe("project")
		})

		it("should handle parent directory traversal", () => {
			const filePath =
				process.platform === "win32" ? "C:\\Users\\test\\other\\file.txt" : "/Users/test/other/file.txt"
			expect(getReadablePath(cwd, filePath)).toBe(filePath.toPosix())
		})

		it("should normalize paths with redundant segments", () => {
			const filePath =
				process.platform === "win32"
					? "C:\\Users\\test\\project\\src\\file.txt"
					: "/Users/test/project/./src/../src/file.txt"
			expect(getReadablePath(cwd, filePath)).toBe("src/file.txt")
		})
	})

	describe("getPathRelation", () => {
		describe("on POSIX", () => {
			beforeEach(() => {
				Object.defineProperty(process, "platform", {
					value: "darwin",
				})
			})

			it("should return 'same' for identical paths", () => {
				expect(getPathRelation("/Users/test/project", "/Users/test/project")).toBe("same")
			})

			it("should return 'parent' when path1 is parent of path2", () => {
				expect(getPathRelation("/Users/test", "/Users/test/project")).toBe("parent")
			})

			it("should return 'child' when path1 is child of path2", () => {
				expect(getPathRelation("/Users/test/project", "/Users/test")).toBe("child")
			})

			it("should return 'unrelated' for unrelated paths", () => {
				expect(getPathRelation("/Users/test/project", "/Users/other/project")).toBe("unrelated")
			})

			it("should handle trailing slashes", () => {
				expect(getPathRelation("/Users/test/", "/Users/test/project")).toBe("parent")
			})

			it("should handle normalized paths", () => {
				expect(getPathRelation("/Users/test/./project", "/Users/test/project")).toBe("same")
			})
		})

		describe("on Windows", () => {
			beforeEach(() => {
				Object.defineProperty(process, "platform", {
					value: "win32",
				})
			})

			it("should return 'same' for identical paths (case-insensitive)", () => {
				expect(getPathRelation("C:\\Users\\Test", "c:\\users\\test")).toBe("same")
			})

			it("should return 'parent' when path1 is parent of path2", () => {
				expect(getPathRelation("C:\\Users", "C:\\Users\\Test")).toBe("parent")
			})

			it("should return 'child' when path1 is child of path2", () => {
				expect(getPathRelation("C:\\Users\\Test", "C:\\Users")).toBe("child")
			})

			it("should return 'unrelated' for unrelated paths", () => {
				expect(getPathRelation("C:\\Users\\Test", "D:\\Users\\Test")).toBe("unrelated")
			})
		})
	})

	describe("canChangeDirectory", () => {
		describe("on POSIX", () => {
			beforeEach(() => {
				Object.defineProperty(process, "platform", {
					value: "darwin",
				})
			})

			it("should return true for same path", () => {
				expect(canChangeDirectory("/Users/test/project", "/Users/test/project")).toBe(true)
			})

			it("should return true for parent to child", () => {
				expect(canChangeDirectory("/Users/test", "/Users/test/project")).toBe(true)
			})

			it("should return true for child to parent", () => {
				expect(canChangeDirectory("/Users/test/project", "/Users/test")).toBe(true)
			})

			it("should return false for unrelated paths", () => {
				expect(canChangeDirectory("/Users/test/project", "/Users/other/project")).toBe(false)
			})

			it("should return false for sibling directories", () => {
				expect(canChangeDirectory("/Users/test/project1", "/Users/test/project2")).toBe(false)
			})
		})

		describe("on Windows", () => {
			beforeEach(() => {
				Object.defineProperty(process, "platform", {
					value: "win32",
				})
			})

			it("should return true for same path (case-insensitive)", () => {
				expect(canChangeDirectory("C:\\Users\\Test", "c:\\users\\test")).toBe(true)
			})

			it("should return true for parent to child", () => {
				expect(canChangeDirectory("C:\\Users", "C:\\Users\\Test")).toBe(true)
			})

			it("should return false for different drives", () => {
				expect(canChangeDirectory("C:\\Users\\Test", "D:\\Users\\Test")).toBe(false)
			})
		})
	})

	describe("parseCdCommand", () => {
		const cwd = process.platform === "win32" ? "E:\\Users\\test\\project" : "/Users/test/project"

		it("should parse simple cd command", () => {
			const expected = process.platform === "win32" ? "E:\\Users\\test\\project\\src" : "/Users/test/project/src"
			expect(parseCdCommand("cd src", cwd)).toBe(expected)
		})

		it("should parse cd with absolute path", () => {
			// On Windows, use Windows-style absolute path
			// On POSIX, use POSIX-style absolute path
			const testPath = process.platform === "win32" ? "E:\\Users\\other" : "/Users/other"
			const result = parseCdCommand(`cd ${testPath}`, cwd)
			expect(result).toBe(testPath)
		})

		it("should parse cd with parent directory", () => {
			const expected = process.platform === "win32" ? "E:\\Users\\test" : "/Users/test"
			expect(parseCdCommand("cd ..", cwd)).toBe(expected)
		})

		it("should parse cd with nested relative path", () => {
			const expected = process.platform === "win32" ? "E:\\Users\\test\\other" : "/Users/test/other"
			expect(parseCdCommand("cd ../other", cwd)).toBe(expected)
		})

		it("should parse cd with ~ to home directory", () => {
			const result = parseCdCommand("cd ~", cwd)
			expect(result).toBe(os.homedir())
		})

		it("should return undefined for cd -", () => {
			expect(parseCdCommand("cd -", cwd)).toBeUndefined()
		})

		it("should return undefined for non-cd commands", () => {
			expect(parseCdCommand("ls -la", cwd)).toBeUndefined()
		})

		it("should handle cd with spaces in path", () => {
			const expected = process.platform === "win32" ? "E:\\Users\\test\\project\\my folder" : "/Users/test/project/my folder"
			expect(parseCdCommand("cd my folder", cwd)).toBe(expected)
		})

		it("should handle cd with leading/trailing whitespace", () => {
			const expected = process.platform === "win32" ? "E:\\Users\\test\\project\\src" : "/Users/test/project/src"
			expect(parseCdCommand("  cd src  ", cwd)).toBe(expected)
		})

		it("should be case insensitive for cd", () => {
			const expected = process.platform === "win32" ? "E:\\Users\\test\\project\\src" : "/Users/test/project/src"
			expect(parseCdCommand("CD src", cwd)).toBe(expected)
			expect(parseCdCommand("Cd src", cwd)).toBe(expected)
		})

		it("should handle cd with multiple spaces", () => {
			const expected = process.platform === "win32" ? "E:\\Users\\test\\project\\src" : "/Users/test/project/src"
			expect(parseCdCommand("cd   src", cwd)).toBe(expected)
		})

		it("should return undefined for cd without arguments", () => {
			expect(parseCdCommand("cd", cwd)).toBeUndefined()
		})

		it("should handle cd with current directory", () => {
			const expected = process.platform === "win32" ? "E:\\Users\\test\\project" : "/Users/test/project"
			expect(parseCdCommand("cd .", cwd)).toBe(expected)
		})

		it("should stop at semicolon separator", () => {
			const expected = process.platform === "win32" ? "E:\\Users\\test\\project\\src" : "/Users/test/project/src"
			expect(parseCdCommand("cd src; echo \"test\"", cwd)).toBe(expected)
		})

		it("should stop at pipe separator", () => {
			const expected = process.platform === "win32" ? "E:\\Users\\test\\project\\src" : "/Users/test/project/src"
			expect(parseCdCommand("cd src | ls", cwd)).toBe(expected)
		})

		it("should stop at ampersand separator", () => {
			const expected = process.platform === "win32" ? "E:\\Users\\test\\project\\src" : "/Users/test/project/src"
			expect(parseCdCommand("cd src && ls", cwd)).toBe(expected)
		})

		it("should handle quoted path with separator after", () => {
			const expected = process.platform === "win32" ? "E:\\Users\\test\\project\\my folder" : "/Users/test/project/my folder"
			expect(parseCdCommand("cd \"my folder\"; ls", cwd)).toBe(expected)
		})

		it("should handle single quoted path with separator after", () => {
			const expected = process.platform === "win32" ? "E:\\Users\\test\\project\\my folder" : "/Users/test/project/my folder"
			expect(parseCdCommand("cd 'my folder' && ls", cwd)).toBe(expected)
		})

		it("should handle multiple separators", () => {
			const expected = process.platform === "win32" ? "E:\\Users\\test\\project\\src" : "/Users/test/project/src"
			expect(parseCdCommand("cd src; ls | grep test", cwd)).toBe(expected)
		})
	})

	describe("removeCdFromCommand", () => {
		it("should remove simple cd command with &&", () => {
			expect(removeCdFromCommand("cd src && npx vitest run utils/__tests__/streaming-token-counter.spec.ts")).toBe(
				"npx vitest run utils/__tests__/streaming-token-counter.spec.ts",
			)
		})

		it("should remove cd command with semicolon", () => {
			expect(removeCdFromCommand("cd src; ls -la")).toBe("ls -la")
		})

		it("should remove cd command with pipe", () => {
			expect(removeCdFromCommand("cd src | grep test")).toBe("grep test")
		})

		it("should remove cd command with ||", () => {
			expect(removeCdFromCommand("cd src || echo error")).toBe("echo error")
		})

		it("should remove cd with quoted path and separator", () => {
			expect(removeCdFromCommand('cd "my folder" && ls')).toBe("ls")
		})

		it("should remove cd with single quoted path", () => {
			expect(removeCdFromCommand("cd 'my folder' && ls")).toBe("ls")
		})

		it("should return original command if no cd found", () => {
			expect(removeCdFromCommand("npx vitest run utils/__tests__/streaming-token-counter.spec.ts")).toBe(
				"npx vitest run utils/__tests__/streaming-token-counter.spec.ts",
			)
		})

		it("should return empty string for cd-only command", () => {
			expect(removeCdFromCommand("cd src")).toBe("")
		})

		it("should handle leading whitespace", () => {
			expect(removeCdFromCommand("  cd src && npm run test")).toBe("npm run test")
		})

		it("should preserve remaining command formatting", () => {
			expect(removeCdFromCommand("cd src && npm run test -- --coverage")).toBe("npm run test -- --coverage")
		})

		it("should handle case-insensitive cd", () => {
			expect(removeCdFromCommand("CD src && echo test")).toBe("echo test")
			expect(removeCdFromCommand("Cd src && echo test")).toBe("echo test")
		})

		it("should handle multiple ampersands with spaces", () => {
			expect(removeCdFromCommand("cd src && npm run test && npm run lint")).toBe("npm run test && npm run lint")
		})
	})
})
