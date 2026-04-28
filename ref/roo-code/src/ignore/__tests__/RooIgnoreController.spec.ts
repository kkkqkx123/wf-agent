// npx vitest core/ignore/__tests__/RooIgnoreController.spec.ts

import { beforeEach, describe, expect, it, vi, type Mock } from "vitest"

import { RooIgnoreController, LOCK_TEXT_SYMBOL, IgnoreMode } from "../RooIgnoreController"
import * as vscode from "vscode"
import * as path from "path"
import * as fs from "fs/promises"
import * as fsSync from "fs"
import { fileExistsAtPath } from "../../../utils/fs"

// Mock dependencies
vi.mock("fs/promises")
vi.mock("fs")
vi.mock("../../../utils/fs")

// Mock vscode
vi.mock("vscode", () => {
	const mockDisposable = { dispose: vi.fn() }
	const mockEventEmitter = {
		event: vi.fn(),
		fire: vi.fn(),
	}

	return {
		workspace: {
			createFileSystemWatcher: vi.fn(() => ({
				onDidCreate: vi.fn(() => mockDisposable),
				onDidChange: vi.fn(() => mockDisposable),
				onDidDelete: vi.fn(() => mockDisposable),
				dispose: vi.fn(),
			})),
		},
		RelativePattern: vi.fn().mockImplementation((base, pattern) => ({
			base,
			pattern,
		})),
		EventEmitter: vi.fn().mockImplementation(() => mockEventEmitter),
		Disposable: {
			from: vi.fn(),
		},
	}
})

describe("RooIgnoreController", () => {
	const TEST_CWD = "/test/path"
	let controller: RooIgnoreController
	let mockFileExists: Mock<typeof fileExistsAtPath>
	let mockReadFile: Mock<typeof fs.readFile>
	let mockWatcher: any

	beforeEach(() => {
		// Reset mocks
		vi.clearAllMocks()

		// Setup mock file watcher
		mockWatcher = {
			onDidCreate: vi.fn().mockReturnValue({ dispose: vi.fn() }),
			onDidChange: vi.fn().mockReturnValue({ dispose: vi.fn() }),
			onDidDelete: vi.fn().mockReturnValue({ dispose: vi.fn() }),
			dispose: vi.fn(),
		}

		// @ts-expect-error - Mocking
		vscode.workspace.createFileSystemWatcher.mockReturnValue(mockWatcher)

		// Setup fs mocks
		mockFileExists = fileExistsAtPath as Mock<typeof fileExistsAtPath>
		mockReadFile = fs.readFile as Mock<typeof fs.readFile>

		// Setup fsSync mocks with default behavior (return path as-is, like regular files)
		const mockRealpathSync = vi.mocked(fsSync.realpathSync)
		mockRealpathSync.mockImplementation((filePath) => filePath.toString())

		// Create controller with gitignore enabled by default
		controller = new RooIgnoreController(TEST_CWD, IgnoreMode.Both)
	})

	describe("initialization", () => {
		/**
		 * Tests the controller initialization when .rooignore exists
		 */
		it("should load .rooignore patterns on initialization when file exists", async () => {
			// Setup mocks to simulate existing .rooignore file
			mockFileExists.mockResolvedValue(true)
			mockReadFile.mockResolvedValue("node_modules\n.git\nsecrets.json")

			// Initialize controller
			await controller.initialize()

			// Verify file was checked and read
			expect(mockFileExists).toHaveBeenCalledWith(path.join(TEST_CWD, ".rooignore"))
			expect(mockReadFile).toHaveBeenCalledWith(path.join(TEST_CWD, ".rooignore"), "utf8")

			// Verify content was stored
			expect(controller.rooIgnoreContent).toBe("node_modules\n.git\nsecrets.json")

			// Test that ignore patterns were applied
			expect(controller.validateAccess("node_modules/package.json")).toBe(false)
			expect(controller.validateAccess("src/app.ts")).toBe(true)
			expect(controller.validateAccess(".git/config")).toBe(false)
			expect(controller.validateAccess("secrets.json")).toBe(false)
		})

		/**
		 * Tests the controller behavior when .rooignore doesn't exist
		 */
		it("should allow all access when .rooignore doesn't exist", async () => {
			// Setup mocks to simulate missing .rooignore file
			mockFileExists.mockResolvedValue(false)

			// Create a new controller with gitignore disabled
			const emptyController = new RooIgnoreController(TEST_CWD, IgnoreMode.Rooignore)
			await emptyController.initialize()

			// Verify no content was stored
			expect(emptyController.rooIgnoreContent).toBeUndefined()

			// All files should be accessible
			expect(emptyController.validateAccess("node_modules/package.json")).toBe(true)
			expect(emptyController.validateAccess("secrets.json")).toBe(true)
		})

		/**
		 * Tests the file watcher setup
		 */
		it("should set up file watcher for .rooignore changes", async () => {
			// Check that watcher was created with correct pattern
			expect(vscode.workspace.createFileSystemWatcher).toHaveBeenCalledWith(
				expect.objectContaining({
					base: TEST_CWD,
					pattern: ".rooignore",
				}),
			)

			// Verify event handlers were registered
			expect(mockWatcher.onDidCreate).toHaveBeenCalled()
			expect(mockWatcher.onDidChange).toHaveBeenCalled()
			expect(mockWatcher.onDidDelete).toHaveBeenCalled()
		})

		/**
		 * Tests error handling during initialization
		 */
		it("should handle errors when loading .rooignore", async () => {
			// Setup mocks to simulate error
			mockFileExists.mockResolvedValue(true)
			mockReadFile.mockRejectedValue(new Error("Test file read error"))

			// Spy on console.error
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => { })

			// Initialize controller - shouldn't throw
			await controller.initialize()

			// Verify error was logged
			expect(consoleSpy).toHaveBeenCalledWith("Unexpected error loading ignore files:", expect.any(Error))

			// Cleanup
			consoleSpy.mockRestore()
		})
	})

	describe("validateAccess", () => {
		beforeEach(async () => {
			// Setup .rooignore content
			mockFileExists.mockResolvedValue(true)
			mockReadFile.mockResolvedValue("node_modules\n.git\nsecrets/**\n*.log")
			await controller.initialize()
		})

		/**
		 * Tests basic path validation
		 */
		it("should correctly validate file access based on ignore patterns", () => {
			// Test different path patterns
			expect(controller.validateAccess("node_modules/package.json")).toBe(false)
			expect(controller.validateAccess("node_modules")).toBe(false)
			expect(controller.validateAccess("src/node_modules/file.js")).toBe(false)
			expect(controller.validateAccess(".git/HEAD")).toBe(false)
			expect(controller.validateAccess("secrets/api-keys.json")).toBe(false)
			expect(controller.validateAccess("logs/app.log")).toBe(false)

			// These should be allowed
			expect(controller.validateAccess("src/app.ts")).toBe(true)
			expect(controller.validateAccess("package.json")).toBe(true)
			expect(controller.validateAccess("secret-file.json")).toBe(true)
		})

		/**
		 * Tests handling of absolute paths
		 */
		it("should handle absolute paths correctly", () => {
			// Test with absolute paths
			const absolutePath = path.join(TEST_CWD, "node_modules/package.json")
			expect(controller.validateAccess(absolutePath)).toBe(false)

			const allowedAbsolutePath = path.join(TEST_CWD, "src/app.ts")
			expect(controller.validateAccess(allowedAbsolutePath)).toBe(true)
		})

		/**
		 * Tests handling of paths outside cwd
		 */
		it("should allow access to paths outside cwd", async () => {
			// Setup initial state
			mockFileExists.mockResolvedValue(true)
			mockReadFile.mockResolvedValue("node_modules\n.git\nsecrets/**\n*.log")
			await controller.initialize()

			// Path traversal outside cwd
			expect(controller.validateAccess("../outside-project/file.txt")).toBe(true)

			// Completely different path
			expect(controller.validateAccess("/etc/hosts")).toBe(true)
		})

		/**
		 * Tests the default behavior when no .rooignore exists
		 */
		it("should allow all access when no .rooignore content", async () => {
			// Create a new controller with no .rooignore (gitignore enabled)
			mockFileExists.mockResolvedValue(false)
			const emptyController = new RooIgnoreController(TEST_CWD, IgnoreMode.Both)
			await emptyController.initialize()

			// All paths should be allowed
			expect(emptyController.validateAccess("node_modules/package.json")).toBe(true)
			expect(emptyController.validateAccess("secrets/api-keys.json")).toBe(true)
			expect(emptyController.validateAccess(".git/HEAD")).toBe(true)
		})

		/**
		 * Tests symlink resolution
		 */
		it("should block symlinks pointing to ignored files", () => {
			// Mock fsSync.realpathSync to simulate symlink resolution
			const mockRealpathSync = vi.mocked(fsSync.realpathSync)
			mockRealpathSync.mockImplementation((filePath) => {
				// Simulate "config.json" being a symlink to "node_modules/package.json"
				if (filePath.toString().endsWith("config.json")) {
					return path.join(TEST_CWD, "node_modules/package.json")
				}
				return filePath.toString()
			})

			// Direct access to ignored file should be blocked
			expect(controller.validateAccess("node_modules/package.json")).toBe(false)

			// Symlink to ignored file should also be blocked
			expect(controller.validateAccess("config.json")).toBe(false)
		})
	})

	describe("validateCommand", () => {
		beforeEach(async () => {
			// Setup .rooignore content
			mockFileExists.mockResolvedValue(true)
			mockReadFile.mockResolvedValue("node_modules\n.git\nsecrets/**\n*.log")
			await controller.initialize()
		})

		/**
		 * Tests validation of file reading commands
		 */
		it("should block file reading commands accessing ignored files", () => {
			// Cat command accessing ignored file
			expect(controller.validateCommand("cat node_modules/package.json")).toBe("node_modules/package.json")

			// Grep command accessing ignored file
			expect(controller.validateCommand("grep pattern .git/config")).toBe(".git/config")

			// Commands accessing allowed files should return undefined
			expect(controller.validateCommand("cat src/app.ts")).toBeUndefined()
			expect(controller.validateCommand("less README.md")).toBeUndefined()
		})

		/**
		 * Tests commands with various arguments and flags
		 */
		it("should handle command arguments and flags correctly", () => {
			// Command with flags
			expect(controller.validateCommand("cat -n node_modules/package.json")).toBe("node_modules/package.json")

			// Command with multiple files (only first ignored file is returned)
			expect(controller.validateCommand("grep pattern src/app.ts node_modules/index.js")).toBe(
				"node_modules/index.js",
			)

			// Command with PowerShell parameter style
			expect(controller.validateCommand("Get-Content -Path secrets/api-keys.json")).toBe("secrets/api-keys.json")

			// Arguments with colons are skipped due to the implementation
			// Adjust test to match actual implementation which skips arguments with colons
			expect(controller.validateCommand("Select-String -Path secrets/api-keys.json -Pattern key")).toBe(
				"secrets/api-keys.json",
			)
		})

		/**
		 * Tests validation of non-file-reading commands
		 */
		it("should allow non-file-reading commands", () => {
			// Commands that don't access files directly
			expect(controller.validateCommand("ls -la")).toBeUndefined()
			expect(controller.validateCommand("echo 'Hello'")).toBeUndefined()
			expect(controller.validateCommand("cd node_modules")).toBeUndefined()
			expect(controller.validateCommand("npm install")).toBeUndefined()
		})

		/**
		 * Tests behavior when no .rooignore exists
		 */
		it("should allow all commands when no .rooignore exists", async () => {
			// Create a new controller with no .rooignore
			mockFileExists.mockResolvedValue(false)
			const emptyController = new RooIgnoreController(TEST_CWD, IgnoreMode.Both)
			await emptyController.initialize()

			// All commands should be allowed
			expect(emptyController.validateCommand("cat node_modules/package.json")).toBeUndefined()
			expect(emptyController.validateCommand("grep pattern .git/config")).toBeUndefined()
		})
	})

	describe("filterPaths", () => {
		beforeEach(async () => {
			// Setup .rooignore content
			mockFileExists.mockResolvedValue(true)
			mockReadFile.mockResolvedValue("node_modules\n.git\nsecrets/**\n*.log")
			await controller.initialize()
		})

		/**
		 * Tests filtering an array of paths
		 */
		it("should filter out ignored paths from an array", () => {
			const paths = [
				"src/app.ts",
				"node_modules/package.json",
				"README.md",
				".git/HEAD",
				"secrets/keys.json",
				"build/app.js",
				"logs/error.log",
			]

			const filtered = controller.filterPaths(paths)

			// Expected filtered result
			expect(filtered).toEqual(["src/app.ts", "README.md", "build/app.js"])

			// Length should be reduced
			expect(filtered.length).toBe(3)
		})

		/**
		 * Tests error handling in filterPaths
		 */
		it("should handle errors in filterPaths and fail closed", () => {
			// Mock validateAccess to throw an error
			vi.spyOn(controller, "validateAccess").mockImplementation(() => {
				throw new Error("Test error")
			})

			// Spy on console.error
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => { })

			// Should return empty array on error (fail closed)
			const result = controller.filterPaths(["file1.txt", "file2.txt"])
			expect(result).toEqual([])

			// Verify error was logged
			expect(consoleSpy).toHaveBeenCalledWith("Error filtering paths:", expect.any(Error))

			// Cleanup
			consoleSpy.mockRestore()
		})

		/**
		 * Tests empty array handling
		 */
		it("should handle empty arrays", () => {
			const result = controller.filterPaths([])
			expect(result).toEqual([])
		})
	})

	describe("getInstructions", () => {
		/**
		 * Tests instructions generation with .rooignore
		 */
		it("should generate formatted instructions when .rooignore exists", async () => {
			// Setup .rooignore content
			mockFileExists.mockResolvedValue(true)
			mockReadFile.mockResolvedValue("node_modules\n.git\nsecrets/**")
			await controller.initialize()

			const instructions = controller.getInstructions()

			// Verify instruction format
			expect(instructions).toContain("# .rooignore")
			expect(instructions).toContain(LOCK_TEXT_SYMBOL)
			expect(instructions).toContain("node_modules")
			expect(instructions).toContain(".git")
			expect(instructions).toContain("secrets/**")
		})

		/**
		 * Tests behavior when no .rooignore exists
		 */
		it("should return undefined when no .rooignore exists", async () => {
			// Setup no .rooignore
			mockFileExists.mockResolvedValue(false)
			await controller.initialize()

			const instructions = controller.getInstructions()
			expect(instructions).toBeUndefined()
		})
	})

	describe("dispose", () => {
		/**
		 * Tests proper cleanup of resources
		 */
		it("should dispose all registered disposables", () => {
			// Create spy for dispose methods
			const disposeSpy = vi.fn()

			// Manually add disposables to test
			controller["disposables"] = [{ dispose: disposeSpy }, { dispose: disposeSpy }, { dispose: disposeSpy }]

			// Call dispose
			controller.dispose()

			// Verify all disposables were disposed
			expect(disposeSpy).toHaveBeenCalledTimes(3)

			// Verify disposables array was cleared
			expect(controller["disposables"]).toEqual([])
		})
	})

	describe("file watcher", () => {
		/**
		 * Tests behavior when .rooignore is created
		 */
		it("should reload .rooignore when file is created", async () => {
			// Setup initial state without .rooignore
			mockFileExists.mockResolvedValue(false)
			await controller.initialize()

			// Verify initial state
			expect(controller.rooIgnoreContent).toBeUndefined()
			expect(controller.validateAccess("node_modules/package.json")).toBe(true)

			// Setup for the test
			mockFileExists.mockResolvedValue(false) // Initially no file exists

			// Create and initialize controller with no .rooignore (gitignore enabled)
			controller = new RooIgnoreController(TEST_CWD, IgnoreMode.Both)
			await controller.initialize()

			// Initial state check
			expect(controller.rooIgnoreContent).toBeUndefined()

			// Now simulate file creation
			mockFileExists.mockResolvedValue(true)
			mockReadFile.mockResolvedValue("node_modules")

			// Force reload of .rooignore content manually
			await controller.initialize()

			// Now verify content was updated
			expect(controller.rooIgnoreContent).toBe("node_modules")

			// Verify access validation changed
			expect(controller.validateAccess("node_modules/package.json")).toBe(false)
		})

		/**
		 * Tests behavior when .rooignore is changed
		 */
		it("should reload .rooignore when file is changed", async () => {
			// Setup initial state with .rooignore
			mockFileExists.mockResolvedValue(true)
			mockReadFile.mockResolvedValue("node_modules")
			await controller.initialize()

			// Verify initial state
			expect(controller.validateAccess("node_modules/package.json")).toBe(false)
			expect(controller.validateAccess(".git/config")).toBe(true)

			// Simulate file change
			mockReadFile.mockResolvedValue("node_modules\n.git")

			// Instead of relying on the onChange handler, manually reload
			// This is because the mock watcher doesn't actually trigger the reload in tests
			await controller.initialize()

			// Verify content was updated
			expect(controller.rooIgnoreContent).toBe("node_modules\n.git")

			// Verify access validation changed
			expect(controller.validateAccess("node_modules/package.json")).toBe(false)
			expect(controller.validateAccess(".git/config")).toBe(false)
		})

		/**
		 * Tests behavior when .rooignore is deleted
		 */
		it("should reset when .rooignore is deleted", async () => {
			// Setup initial state with .rooignore
			mockFileExists.mockImplementation((filePath) => {
				const pathStr = filePath.toString()
				if (pathStr.endsWith(".rooignore")) {
					return Promise.resolve(true)
				}
				if (pathStr.endsWith(".gitignore")) {
					return Promise.resolve(false)
				}
				return Promise.resolve(false)
			})
			mockReadFile.mockImplementation((filePath) => {
				const pathStr = filePath.toString()
				if (pathStr.endsWith(".rooignore")) {
					return Promise.resolve("node_modules")
				}
				return Promise.resolve("")
			})
			await controller.initialize()

			// Verify initial state
			expect(controller.validateAccess("node_modules/package.json")).toBe(false)

			// Simulate file deletion - update fileExists mock to return false
			mockFileExists.mockImplementation((filePath) => {
				const pathStr = filePath.toString()
				if (pathStr.endsWith(".rooignore")) {
					return Promise.resolve(false)
				}
				if (pathStr.endsWith(".gitignore")) {
					return Promise.resolve(false)
				}
				return Promise.resolve(false)
			})

			// Find and trigger the onDelete handler
			const onDeleteHandler = mockWatcher.onDidDelete.mock.calls[0][0]
			await onDeleteHandler()

			// Manually trigger reload to ensure the change is processed
			// (In real scenario, the file watcher would trigger this)
			await controller.initialize()

			// Verify content was reset
			expect(controller.rooIgnoreContent).toBeUndefined()

			// Verify access validation changed
			expect(controller.validateAccess("node_modules/package.json")).toBe(true)
		})
	})

	describe("gitignore support", () => {
		/**
			* Tests loading .gitignore patterns
			*/
		it("should load .gitignore patterns when enabled", async () => {
			// Setup mocks to simulate existing .gitignore file
			mockFileExists.mockImplementation((filePath) => {
				const pathStr = filePath.toString()
				if (pathStr.endsWith(".gitignore")) {
					return Promise.resolve(true)
				}
				if (pathStr.endsWith(".rooignore")) {
					return Promise.resolve(false)
				}
				return Promise.resolve(false)
			})
			mockReadFile.mockImplementation((filePath) => {
				const pathStr = filePath.toString()
				if (pathStr.endsWith(".gitignore")) {
					return Promise.resolve("dist\nnode_modules\n*.log")
				}
				return Promise.resolve("")
			})

			// Initialize controller with gitignore enabled
			await controller.initialize()

			// Verify .gitignore was loaded
			expect(controller.gitignoreContent).toBe("dist\nnode_modules\n*.log")

			// Test that gitignore patterns were applied
			expect(controller.validateAccess("dist/bundle.js")).toBe(false)
			expect(controller.validateAccess("node_modules/package.json")).toBe(false)
			expect(controller.validateAccess("app.log")).toBe(false)
			expect(controller.validateAccess("src/app.ts")).toBe(true)
		})

		/**
			* Tests that .rooignore takes precedence over .gitignore
			*/
		it("should apply both .rooignore and .gitignore patterns", async () => {
			// Setup mocks to simulate both files existing
			mockFileExists.mockResolvedValue(true)
			mockReadFile.mockImplementation((filePath) => {
				const pathStr = filePath.toString()
				if (pathStr.endsWith(".gitignore")) {
					return Promise.resolve("dist\nnode_modules")
				}
				if (pathStr.endsWith(".rooignore")) {
					return Promise.resolve("secrets\n.env")
				}
				return Promise.resolve("")
			})

			// Initialize controller
			await controller.initialize()

			// Verify both files were loaded
			expect(controller.gitignoreContent).toBe("dist\nnode_modules")
			expect(controller.rooIgnoreContent).toBe("secrets\n.env")

			// Test that both patterns are applied
			expect(controller.validateAccess("dist/bundle.js")).toBe(false) // gitignore
			expect(controller.validateAccess("node_modules/package.json")).toBe(false) // gitignore
			expect(controller.validateAccess("secrets/api-keys.json")).toBe(false) // rooignore
			expect(controller.validateAccess(".env")).toBe(false) // rooignore
			expect(controller.validateAccess("src/app.ts")).toBe(true)
		})

		/**
		 * Tests disabling .gitignore filtering
		 */
		it("should not apply .gitignore patterns when disabled", async () => {
			// Create controller with gitignore disabled
			const controllerNoGitignore = new RooIgnoreController(TEST_CWD, IgnoreMode.Rooignore)

			// Setup mocks to simulate .gitignore file existing
			mockFileExists.mockImplementation((filePath) => {
				const pathStr = filePath.toString()
				if (pathStr.endsWith(".gitignore")) {
					return Promise.resolve(true)
				}
				if (pathStr.endsWith(".rooignore")) {
					return Promise.resolve(false)
				}
				return Promise.resolve(false)
			})
			mockReadFile.mockImplementation((filePath) => {
				const pathStr = filePath.toString()
				if (pathStr.endsWith(".gitignore")) {
					return Promise.resolve("dist\nnode_modules")
				}
				return Promise.resolve("")
			})

			// Initialize controller
			await controllerNoGitignore.initialize()

			// Verify .gitignore was not loaded
			expect(controllerNoGitignore.gitignoreContent).toBeUndefined()

			// Test that gitignore patterns were NOT applied
			expect(controllerNoGitignore.validateAccess("dist/bundle.js")).toBe(true)
			expect(controllerNoGitignore.validateAccess("node_modules/package.json")).toBe(true)
		})

		/**
			* Tests setIgnoreMode method
			*/
		it("should allow toggling ignore modes", async () => {
			// Setup mocks to simulate .gitignore file existing
			mockFileExists.mockImplementation((filePath) => {
				const pathStr = filePath.toString()
				if (pathStr.endsWith(".gitignore")) {
					return Promise.resolve(true)
				}
				if (pathStr.endsWith(".rooignore")) {
					return Promise.resolve(false)
				}
				return Promise.resolve(false)
			})
			mockReadFile.mockImplementation((filePath) => {
				const pathStr = filePath.toString()
				if (pathStr.endsWith(".gitignore")) {
					return Promise.resolve("dist\nnode_modules")
				}
				return Promise.resolve("")
			})

			// Initialize controller with IgnoreMode.Both (both gitignore and rooignore)
			await controller.initialize()

			// Verify gitignore is enabled and working
			expect(controller.getIgnoreMode()).toBe(IgnoreMode.Both)
			expect(controller.validateAccess("dist/bundle.js")).toBe(false)

			// Disable gitignore by setting mode to Rooignore only
			controller.setIgnoreMode(IgnoreMode.Rooignore)

			// Verify gitignore is disabled
			expect(controller.getIgnoreMode()).toBe(IgnoreMode.Rooignore)
			expect(controller.validateAccess("dist/bundle.js")).toBe(true)

			// Re-enable gitignore - ensure fileExists and readFile mocks still return gitignore content
			// First, reset the mocks to their initial state for this test
			mockFileExists.mockImplementation((filePath) => {
				const pathStr = filePath.toString()
				if (pathStr.endsWith(".gitignore")) {
					return Promise.resolve(true)
				}
				if (pathStr.endsWith(".rooignore")) {
					return Promise.resolve(false)
				}
				return Promise.resolve(false)
			})
			mockReadFile.mockImplementation((filePath) => {
				const pathStr = filePath.toString()
				if (pathStr.endsWith(".gitignore")) {
					return Promise.resolve("dist\nnode_modules")
				}
				return Promise.resolve("")
			})

			// Now re-enable gitignore mode
			controller.setIgnoreMode(IgnoreMode.Both)

			// Verify gitignore is enabled again
			expect(controller.getIgnoreMode()).toBe(IgnoreMode.Both)
			// Note: setIgnoreMode triggers reload, so gitignore patterns should be reapplied
			// Wait for the async reload to complete
			await new Promise(resolve => setTimeout(resolve, 10))
			expect(controller.validateAccess("dist/bundle.js")).toBe(false)
		})

		/**
			* Tests getInstructions includes both .rooignore and .gitignore
			*/
		it("should include both .rooignore and .gitignore in instructions", async () => {
			// Setup mocks to simulate both files existing
			mockFileExists.mockResolvedValue(true)
			mockReadFile.mockImplementation((filePath) => {
				const pathStr = filePath.toString()
				if (pathStr.endsWith(".gitignore")) {
					return Promise.resolve("dist\nnode_modules")
				}
				if (pathStr.endsWith(".rooignore")) {
					return Promise.resolve("secrets\n.env")
				}
				return Promise.resolve("")
			})

			// Initialize controller
			await controller.initialize()

			// Get instructions
			const instructions = controller.getInstructions()

			// Verify instructions include both files
			expect(instructions).toBeDefined()
			expect(instructions).toContain(".rooignore")
			expect(instructions).toContain(".gitignore")
			expect(instructions).toContain("secrets")
			expect(instructions).toContain("dist")
		})

		/**
			* Tests getInstructions returns undefined when no ignore files exist
			*/
		it("should return undefined instructions when no ignore files exist", async () => {
			// Setup mocks to simulate no ignore files
			mockFileExists.mockResolvedValue(false)

			// Initialize controller
			await controller.initialize()

			// Get instructions
			const instructions = controller.getInstructions()

			// Verify instructions are undefined
			expect(instructions).toBeUndefined()
		})
	})
})
