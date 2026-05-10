import * as path from "path"
import os from "os"
import * as vscode from "vscode"

/*
The Node.js 'path' module resolves and normalizes paths differently depending on the platform:
- On Windows, it uses backslashes (\) as the default path separator.
- On POSIX-compliant systems (Linux, macOS), it uses forward slashes (/) as the default path separator.

While modules like 'upath' can be used to normalize paths to use forward slashes consistently,
this can create inconsistencies when interfacing with other modules (like vscode.fs) that use
backslashes on Windows.

Our approach:
1. We present paths with forward slashes to the AI and user for consistency.
2. We use the 'arePathsEqual' function for safe path comparisons.
3. Internally, Node.js gracefully handles both backslashes and forward slashes.

This strategy ensures consistent path presentation while leveraging Node.js's built-in
path handling capabilities across different platforms.

Note: When interacting with the file system or VS Code APIs, we still use the native path module
to ensure correct behavior on all platforms. The toPosixPath and arePathsEqual functions are
primarily used for presentation and comparison purposes, not for actual file system operations.

Observations:
- Macos isn't so flexible with mixed separators, whereas windows can handle both. ("Node.js does automatically handle path separators on Windows, converting forward slashes to backslashes as needed. However, on macOS and other Unix-like systems, the path separator is always a forward slash (/), and backslashes are treated as regular characters.")
*/

function toPosixPath(p: string) {
	// Extended-Length Paths in Windows start with "\\?\" to allow longer paths and bypass usual parsing. If detected, we return the path unmodified to maintain functionality, as altering these paths could break their special syntax.
	const isExtendedLengthPath = p.startsWith("\\\\?\\")

	if (isExtendedLengthPath) {
		return p
	}

	return p.replace(/\\/g, "/")
}

// Declaration merging allows us to add a new method to the String type
// You must import this file in your entry point (extension.ts) to have access at runtime
declare global {
	interface String {
		toPosix(): string
	}
}

String.prototype.toPosix = function (this: string): string {
	return toPosixPath(this)
}

// Safe path comparison that works across different platforms
export function arePathsEqual(path1?: string, path2?: string): boolean {
	if (!path1 && !path2) {
		return true
	}
	if (!path1 || !path2) {
		return false
	}

	path1 = normalizePath(path1)
	path2 = normalizePath(path2)

	if (process.platform === "win32") {
		return path1.toLowerCase() === path2.toLowerCase()
	}
	return path1 === path2
}

function normalizePath(p: string): string {
	// normalize resolve ./.. segments, removes duplicate slashes, and standardizes path separators
	let normalized = path.normalize(p)
	// however it doesn't remove trailing slashes
	// remove trailing slash, except for root paths
	if (normalized.length > 1 && (normalized.endsWith("/") || normalized.endsWith("\\"))) {
		normalized = normalized.slice(0, -1)
	}
	return normalized
}

export function getReadablePath(cwd: string, relPath?: string): string {
	// If relPath is undefined, return empty string instead of allowing path.resolve
	// to return cwd (which would then show misleading cwd basename in UI)
	if (relPath === undefined) {
		return ""
	}

	// path.resolve is flexible in that it will resolve relative paths like '../../' to the cwd and even ignore the cwd if the relPath is actually an absolute path
	const absolutePath = path.resolve(cwd, relPath)
	if (arePathsEqual(cwd, path.join(os.homedir(), "Desktop"))) {
		// User opened vscode without a workspace, so cwd is the Desktop. Show the full absolute path to keep the user aware of where files are being created
		return absolutePath.toPosix()
	}
	if (arePathsEqual(path.normalize(absolutePath), path.normalize(cwd))) {
		return path.basename(absolutePath).toPosix()
	} else {
		// show the relative path to the cwd
		const normalizedRelPath = path.relative(cwd, absolutePath)
		if (absolutePath.includes(cwd)) {
			return normalizedRelPath.toPosix()
		} else {
			// we are outside the cwd, so show the absolute path (useful for when cline passes in '../../' for example)
			return absolutePath.toPosix()
		}
	}
}

export const toRelativePath = (filePath: string, cwd: string) => {
	const relativePath = path.relative(cwd, filePath).toPosix()
	return filePath.endsWith("/") ? relativePath + "/" : relativePath
}

export const getWorkspacePath = (defaultCwdPath = "") => {
	const cwdPath = vscode.workspace.workspaceFolders?.map((folder) => folder.uri.fsPath).at(0) || defaultCwdPath
	const currentFileUri = vscode.window.activeTextEditor?.document.uri
	if (currentFileUri) {
		const workspaceFolder = vscode.workspace.getWorkspaceFolder(currentFileUri)
		return workspaceFolder?.uri.fsPath || cwdPath
	}
	return cwdPath
}

export const getWorkspacePathForContext = (contextPath?: string): string => {
	// If context path provided, find its workspace
	if (contextPath) {
		const workspaceFolder = vscode.workspace.getWorkspaceFolder(vscode.Uri.file(contextPath))
		if (workspaceFolder) {
			return workspaceFolder.uri.fsPath
		}
		// Debug logging when falling back
		console.debug(`[CodeIndex] No workspace found for context path: ${contextPath}, falling back to default`)
	}

	// Fall back to current behavior
	return getWorkspacePath()
}

/**
 * Represents the relationship between two paths
 */
export type PathRelation = "same" | "parent" | "child" | "unrelated"

/**
 * Determines the relationship between two paths
 * @param path1 First path
 * @param path2 Second path
 * @returns The relationship between the paths
 */
export function getPathRelation(path1: string, path2: string): PathRelation {
	const normalized1 = normalizePath(path1)
	const normalized2 = normalizePath(path2)

	if (arePathsEqual(normalized1, normalized2)) {
		return "same"
	}

	// Check if path1 is a parent of path2
	if (normalized2.startsWith(normalized1 + path.sep)) {
		return "parent"
	}

	// Check if path1 is a child of path2
	if (normalized1.startsWith(normalized2 + path.sep)) {
		return "child"
	}

	return "unrelated"
}

/**
 * Checks if a terminal can change from current directory to target directory
 * @param currentPath The current working directory of the terminal
 * @param targetPath The target directory to change to
 * @returns true if the directory can be changed via cd command
 */
export function canChangeDirectory(currentPath: string, targetPath: string): boolean {
	const relation = getPathRelation(currentPath, targetPath)

	// Same path or parent/child relationship can be changed via cd
	return relation === "same" || relation === "parent" || relation === "child"
}

/**
 * Internal: Parse cd command and find separator position
 * @returns Object containing the unquoted path, separator index and length, or undefined if not a cd command
 */
function parseCdCommandInternal(
	command: string,
): { unquotedPath: string; separatorIndex: number; separatorLength: number } | undefined {
	// Trim leading whitespace
	const trimmedCommand = command.trim()

	// Check if command starts with 'cd'
	if (!/^cd\s+/i.test(trimmedCommand)) {
		return undefined
	}

	// Extract the part after 'cd '
	const afterCd = trimmedCommand.slice(3).trim()

	// Parse the path, stopping at command separators
	// Need to handle quoted paths correctly
	let separatorIndex = -1
	let separatorLength = 0
	let inQuotes = false
	let quoteChar = ""

	for (let i = 0; i < afterCd.length; i++) {
		const char = afterCd.charAt(i)

		// Handle quote state (ignore escaped quotes)
		if ((char === '"' || char === "'") && (i === 0 || afterCd.charAt(i - 1) !== "\\")) {
			if (!inQuotes) {
				inQuotes = true
				quoteChar = char
			} else if (char === quoteChar) {
				inQuotes = false
				quoteChar = ""
			}
			continue
		}

		// If not in quotes, check for separators
		if (!inQuotes) {
			// Check for two-character separators first (&&, ||)
			if (i + 1 < afterCd.length) {
				const twoCharSep = afterCd.slice(i, i + 2)
				if (twoCharSep === "&&" || twoCharSep === "||") {
					separatorIndex = i
					separatorLength = 2
					break
				}
			}

			// Check for single-character separators (;, |, &)
			const char = afterCd.charAt(i)
			if (char === ";" || char === "|" || char === "&") {
				separatorIndex = i
				separatorLength = 1
				break
			}
		}
	}

	const pathEnd = separatorIndex !== -1 ? separatorIndex : afterCd.length
	const targetPath = afterCd.slice(0, pathEnd).trim()

	// Remove quotes if present (single or double quotes)
	let unquotedPath = targetPath
	if ((targetPath.startsWith('"') && targetPath.endsWith('"')) || (targetPath.startsWith("'") && targetPath.endsWith("'"))) {
		unquotedPath = targetPath.slice(1, -1)
	}

	return { unquotedPath, separatorIndex, separatorLength }
}

/**
 * Parses a command to extract the target directory if it starts with 'cd'
 * @param command The command to parse
 * @param currentCwd The current working directory
 * @returns The target directory if cd command is found, otherwise undefined
 */
export function parseCdCommand(command: string, currentCwd: string): string | undefined {
	const parseResult = parseCdCommandInternal(command)
	if (!parseResult) {
		return undefined
	}

	const { unquotedPath } = parseResult

	// Handle special cases
	if (unquotedPath === "~") {
		// Expand ~ to home directory
		return os.homedir()
	}

	if (unquotedPath === "-") {
		// cd - returns to previous directory (not supported in this context)
		return undefined
	}

	// Handle relative paths
	if (!path.isAbsolute(unquotedPath)) {
		return path.resolve(currentCwd, unquotedPath)
	}

	return unquotedPath
}

/**
 * Removes the cd command from the beginning of a command string if present
 * @param command The command string that may start with cd
 * @returns The command with cd part removed, or the original command if no cd is found
 */
export function removeCdFromCommand(command: string): string {
	const parseResult = parseCdCommandInternal(command)
	if (!parseResult) {
		return command
	}

	const { separatorIndex, separatorLength } = parseResult
	const trimmedCommand = command.trim()
	const afterCd = trimmedCommand.slice(3).trim()

	if (separatorIndex !== -1) {
		// Found a separator, remove the cd part and the separator, then the rest of the command
		const restOfCommand = afterCd.slice(separatorIndex + separatorLength).trim()
		return restOfCommand
	}

	// No separator found, entire command is just the cd part
	return ""
}
