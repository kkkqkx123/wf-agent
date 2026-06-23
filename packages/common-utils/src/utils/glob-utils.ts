import * as fs from "fs/promises";
import * as path from "path";

export async function matchGlobPattern(pattern: string, baseDir: string): Promise<string[]> {
  const absoluteBase = path.resolve(baseDir);

  let normalizedPattern = pattern.replace(/\\/g, "/");

  if (normalizedPattern.startsWith("./")) {
    normalizedPattern = normalizedPattern.slice(2);
  }

  const parts = normalizedPattern.split("/");
  const results: string[] = [];

  async function scan(currentDir: string, partIndex: number): Promise<void> {
    if (partIndex >= parts.length) return;

    const part = parts[partIndex];
    if (part === undefined) return;
    const isLast = partIndex === parts.length - 1;

    if (part === "**") {
      try {
        const entries = await fs.readdir(currentDir, { withFileTypes: true });
        for (const entry of entries) {
          const fullPath = path.join(currentDir, entry.name);
          if (entry.isDirectory()) {
            await scan(fullPath, partIndex);
            await scan(fullPath, partIndex + 1);
          } else if (entry.isFile() && isLast) {
            results.push(fullPath);
          }
        }
      } catch {
        // Ignore errors reading directory
      }
      return;
    }

    if (part === "*") {
      try {
        const entries = await fs.readdir(currentDir, { withFileTypes: true });
        for (const entry of entries) {
          const fullPath = path.join(currentDir, entry.name);
          if (isLast && entry.isFile()) {
            results.push(fullPath);
          } else if (!isLast && entry.isDirectory()) {
            await scan(fullPath, partIndex + 1);
          }
        }
      } catch {
        // Ignore errors reading directory
      }
      return;
    }

    if (part.startsWith("*.")) {
      const ext = part.slice(1);
      try {
        const entries = await fs.readdir(currentDir, { withFileTypes: true });
        for (const entry of entries) {
          const fullPath = path.join(currentDir, entry.name);
          if (isLast && entry.isFile() && entry.name.endsWith(ext)) {
            results.push(fullPath);
          } else if (!isLast && entry.isDirectory()) {
            await scan(fullPath, partIndex + 1);
          }
        }
      } catch {
        // Ignore errors reading directory
      }
      return;
    }

    const prefixMatch = part.match(/^(.+)\*\.(.+)$/);
    if (prefixMatch) {
      const prefix = prefixMatch[1]!;
      const ext = prefixMatch[2]!;
      try {
        const entries = await fs.readdir(currentDir, { withFileTypes: true });
        for (const entry of entries) {
          const fullPath = path.join(currentDir, entry.name);
          if (
            isLast &&
            entry.isFile() &&
            entry.name.startsWith(prefix) &&
            entry.name.endsWith(`.${ext}`)
          ) {
            results.push(fullPath);
          } else if (!isLast && entry.isDirectory()) {
            await scan(fullPath, partIndex + 1);
          }
        }
      } catch {
        // Ignore errors reading directory
      }
      return;
    }

    const literalPath = path.join(currentDir, part);
    try {
      const stats = await fs.stat(literalPath);
      if (isLast && stats.isFile()) {
        results.push(literalPath);
      } else if (!isLast && stats.isDirectory()) {
        await scan(literalPath, partIndex + 1);
      }
    } catch {
      // Ignore errors checking file stat
    }
  }

  await scan(absoluteBase, 0);
  return results;
}
