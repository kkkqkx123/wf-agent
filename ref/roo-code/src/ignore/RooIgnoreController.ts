import path from "path"
import { fileExistsAtPath } from "../../utils/fs"
import fs from "fs/promises"
import fsSync from "fs"
import ignore, { Ignore } from "ignore"
import * as vscode from "vscode"

export const LOCK_TEXT_SYMBOL = "\u{1F512}"

/**
 * Ignore mode options
 */
export enum IgnoreMode {
	/** Only use .gitignore patterns */
	Gitignore = "gitignore",
	/** Only use .rooignore patterns */
	Rooignore = "rooignore",
	/** Use both .gitignore and .rooignore patterns */
	Both = "both",
}

/**
 * Controls LLM access to files by enforcing ignore patterns.
 * Designed to be instantiated once in Cline.ts and passed to file manipulation services.
 * Uses the 'ignore' library to support standard .gitignore syntax in .rooignore files.
 */
export class RooIgnoreController {
	private cwd: string
	private gitignoreInstance: Ignore
	private rooignoreInstance: Ignore
	private disposables: vscode.Disposable[] = []
	rooIgnoreContent: string | undefined
	gitignoreContent: string | undefined
	ignoreMode: IgnoreMode

	constructor(cwd: string, ignoreMode: IgnoreMode = IgnoreMode.Both) {
		this.cwd = cwd
		this.gitignoreInstance = ignore()
		this.rooignoreInstance = ignore()
		this.rooIgnoreContent = undefined
		this.gitignoreContent = undefined
		this.ignoreMode = ignoreMode
		// Set up file watcher for .rooignore and .gitignore
		this.setupFileWatcher()
	}

	/**
	 * Initialize the controller by loading custom patterns
	 * Must be called after construction and before using the controller
	 */
	async initialize(): Promise<void> {
		await this.loadRooIgnore()
	}

	/**
	 * Set up the file watcher for .rooignore and .gitignore changes
	 */
	private setupFileWatcher(): void {
		// Watch .rooignore
		const rooignorePattern = new vscode.RelativePattern(this.cwd, ".rooignore")
		const rooignoreWatcher = vscode.workspace.createFileSystemWatcher(rooignorePattern)
		this.disposables.push(
			rooignoreWatcher.onDidChange(() => {
				this.loadRooIgnore()
			}),
			rooignoreWatcher.onDidCreate(() => {
				this.loadRooIgnore()
			}),
			rooignoreWatcher.onDidDelete(() => {
				this.loadRooIgnore()
			}),
			rooignoreWatcher,
		)

		// Watch .gitignore if mode is Gitignore or Both
		if (this.ignoreMode === IgnoreMode.Gitignore || this.ignoreMode === IgnoreMode.Both) {
			const gitignorePattern = new vscode.RelativePattern(this.cwd, ".gitignore")
			const gitignoreWatcher = vscode.workspace.createFileSystemWatcher(gitignorePattern)
			this.disposables.push(
				gitignoreWatcher.onDidChange(() => {
					this.loadRooIgnore()
				}),
				gitignoreWatcher.onDidCreate(() => {
					this.loadRooIgnore()
				}),
				gitignoreWatcher.onDidDelete(() => {
					this.loadRooIgnore()
				}),
				gitignoreWatcher,
			)
		}
	}

	/**
	 * Load custom patterns from .rooignore and .gitignore if they exist
	 */
	private async loadRooIgnore(): Promise<void> {
		try {
			// Reset ignore instances to prevent duplicate patterns
			this.gitignoreInstance = ignore()
			this.rooignoreInstance = ignore()
			
			// Load .gitignore patterns (if mode is Gitignore or Both)
			if (this.ignoreMode === IgnoreMode.Gitignore || this.ignoreMode === IgnoreMode.Both) {
				const gitignorePath = path.join(this.cwd, ".gitignore")
				if (await fileExistsAtPath(gitignorePath)) {
					const gitignoreContent = await fs.readFile(gitignorePath, "utf8")
					this.gitignoreContent = gitignoreContent
					this.gitignoreInstance.add(gitignoreContent)
				} else {
					this.gitignoreContent = undefined
				}
			} else {
				this.gitignoreContent = undefined
			}
			
			// Load .rooignore patterns (if mode is Rooignore or Both)
			if (this.ignoreMode === IgnoreMode.Rooignore || this.ignoreMode === IgnoreMode.Both) {
				const rooignorePath = path.join(this.cwd, ".rooignore")
				if (await fileExistsAtPath(rooignorePath)) {
					const content = await fs.readFile(rooignorePath, "utf8")
					this.rooIgnoreContent = content
					this.rooignoreInstance.add(content)
					this.rooignoreInstance.add(".rooignore")
				} else {
					this.rooIgnoreContent = undefined
				}
			} else {
				this.rooIgnoreContent = undefined
			}
		} catch (error) {
			// Should never happen: reading file failed even though it exists
			console.error("Unexpected error loading ignore files:", error)
		}
	}

	/**
	 * Check if a file should be accessible to the LLM
	 * Automatically resolves symlinks
	 * @param filePath - Path to check (relative to cwd)
	 * @returns true if file is accessible, false if ignored
	 */
	validateAccess(filePath: string): boolean {
		try {
			const absolutePath = path.resolve(this.cwd, filePath)

			// Follow symlinks to get the real path
			let realPath: string
			try {
				realPath = fsSync.realpathSync(absolutePath)
			} catch {
				// If realpath fails (file doesn't exist, broken symlink, etc.),
				// use the original path
				realPath = absolutePath
			}

			// Convert real path to relative for ignore checking
			const relativePath = path.relative(this.cwd, realPath).toPosix()

			// Check if the real path is ignored by .rooignore (if mode is Rooignore or Both)
			if (this.ignoreMode === IgnoreMode.Rooignore || this.ignoreMode === IgnoreMode.Both) {
				const ignoredByRooignore = this.rooignoreInstance.ignores(relativePath)
				if (ignoredByRooignore) {
					return false
				}
			}

			// Check if the real path is ignored by .gitignore (if mode is Gitignore or Both)
			if (this.ignoreMode === IgnoreMode.Gitignore || this.ignoreMode === IgnoreMode.Both) {
				const ignoredByGitignore = this.gitignoreInstance.ignores(relativePath)
				if (ignoredByGitignore) {
					return false
				}
			}

			return true
		} catch (error) {
			// Allow access to files outside cwd or on errors (backward compatibility)
			return true
		}
	}

	/**
	 * Check if a terminal command should be allowed to execute based on file access patterns
	 * @param command - Terminal command to validate
	 * @returns path of file that is being accessed if it is being accessed, undefined if command is allowed
	 */
	validateCommand(command: string): string | undefined {
		// Always allow if no ignore rules exist
		if (!this.rooIgnoreContent && !this.gitignoreContent) {
			return undefined
		}

		// Split command into parts and get the base command
		const parts = command.trim().split(/\s+/)
		const baseCommand = parts[0]?.toLowerCase()

		// Commands that read file contents
		const fileReadingCommands = [
			// Unix commands
			"cat",
			"less",
			"more",
			"head",
			"tail",
			"grep",
			"awk",
			"sed",
			// PowerShell commands and aliases
			"get-content",
			"gc",
			"type",
			"select-string",
			"sls",
		]

		if (baseCommand && fileReadingCommands.includes(baseCommand)) {
			// Check each argument that could be a file path
			for (let i = 1; i < parts.length; i++) {
				const arg = parts[i]
				if (!arg) {
					continue
				}
				// Skip command flags/options (both Unix and PowerShell style)
				if (arg.startsWith("-") || arg.startsWith("/")) {
					continue
				}
				// Ignore PowerShell parameter names
				if (arg.includes(":")) {
					continue
				}
				// Validate file access
				if (!this.validateAccess(arg)) {
					return arg
				}
			}
		}

		return undefined
	}

	/**
	 * Filter an array of paths, removing those that should be ignored
	 * @param paths - Array of paths to filter (relative to cwd)
	 * @returns Array of allowed paths
	 */
	filterPaths(paths: string[]): string[] {
		try {
			return paths
				.map((p) => ({
					path: p,
					allowed: this.validateAccess(p),
				}))
				.filter((x) => x.allowed)
				.map((x) => x.path)
		} catch (error) {
			console.error("Error filtering paths:", error)
			return [] // Fail closed for security
		}
	}

	/**
	 * Clean up resources when the controller is no longer needed
	 */
	dispose(): void {
		this.disposables.forEach((d) => d.dispose())
		this.disposables = []
	}

	/**
	 * Get formatted instructions about the .rooignore and .gitignore files for the LLM
	 * @returns Formatted instructions or undefined if no ignore files exist
	 */
	getInstructions(): string | undefined {
		if (!this.rooIgnoreContent && !this.gitignoreContent) {
			return undefined
		}

		let instructions = ""
		
		if (this.rooIgnoreContent && (this.ignoreMode === IgnoreMode.Rooignore || this.ignoreMode === IgnoreMode.Both)) {
			instructions += `# .rooignore\n\n(The following is provided by a root-level .rooignore file where the user has specified files and directories that should not be accessed. When using list_files, you'll notice a ${LOCK_TEXT_SYMBOL} next to files that are blocked. Attempting to access the file's contents e.g. through read_file will result in an error.)\n\n${this.rooIgnoreContent}\n.rooignore\n\n`
		}
		
		if (this.gitignoreContent && (this.ignoreMode === IgnoreMode.Gitignore || this.ignoreMode === IgnoreMode.Both)) {
			instructions += `# .gitignore\n\n(The following is provided by a root-level .gitignore file. Files and directories listed here are also filtered from search results to improve performance. If you need to access build artifacts or other ignored files, use execute_command with your own scripts or commands to view them directly.)\n\n${this.gitignoreContent}\n.gitignore`
		}
		
		return instructions || undefined
	}

	/**
	 * Set the ignore mode
	 * @param mode - The ignore mode to use
	 */
	setIgnoreMode(mode: IgnoreMode): void {
		if (this.ignoreMode !== mode) {
			this.ignoreMode = mode
			this.loadRooIgnore()
		}
	}

	/**
	 * Get the current ignore mode
	 * @returns The current ignore mode
	 */
	getIgnoreMode(): IgnoreMode {
		return this.ignoreMode
	}
}
