import fs from "fs/promises"
import * as path from "path"

/**
 * Asynchronously creates all non-existing subdirectories for a given file path
 * and collects them in an array for later deletion.
 *
 * @param filePath - The full path to a file.
 * @param cwd - The current working directory to validate against (optional).
 * @returns A promise that resolves to an array of newly created directories.
 * @throws Error if the path is outside the current working directory.
 */
export async function createDirectoriesForFile(filePath: string, cwd?: string): Promise<string[]> {
	const newDirectories: string[] = []
	const normalizedFilePath = path.normalize(filePath) // Normalize path for cross-platform compatibility
	const directoryPath = path.dirname(normalizedFilePath)

	// Validate that the directory path is within the current working directory if provided
	if (cwd) {
		const normalizedCwd = path.normalize(cwd)
		const absoluteDirPath = path.resolve(directoryPath)
		const absoluteCwd = path.resolve(normalizedCwd)
		
		// Check if the directory path is within the current working directory
		if (!absoluteDirPath.startsWith(absoluteCwd + path.sep) && absoluteDirPath !== absoluteCwd) {
			throw new Error(
				`Cannot create directory outside workspace: ${directoryPath} is not within ${normalizedCwd}`
			)
		}
	}

	let currentPath = directoryPath
	const dirsToCreate: string[] = []

	// Traverse up the directory tree and collect missing directories
	while (!(await fileExistsAtPath(currentPath))) {
		dirsToCreate.push(currentPath)
		currentPath = path.dirname(currentPath)
	}

	// Create directories from the topmost missing one down to the target directory
	for (let i = dirsToCreate.length - 1; i >= 0; i--) {
		const dir = dirsToCreate[i]
		if (dir) {
			await fs.mkdir(dir)
			newDirectories.push(dir)
		}
	}

	return newDirectories
}

/**
 * Helper function to check if a path exists.
 *
 * @param path - The path to check.
 * @returns A promise that resolves to true if the path exists, false otherwise.
 */
export async function fileExistsAtPath(filePath: string): Promise<boolean> {
	try {
		await fs.access(filePath)
		return true
	} catch {
		return false
	}
}
