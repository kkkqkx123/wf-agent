import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync } from "fs";
import { join, resolve } from "path";

const EXAMPLES_DIR = resolve(__dirname, "../../examples/storage");

export function saveStorageSnapshot(testName: string, storageDir: string): void {
  const metadataDir = join(storageDir, "metadata");
  if (!existsSync(metadataDir)) return;

  const targetDir = join(EXAMPLES_DIR, testName);
  if (existsSync(targetDir)) {
    rmSync(targetDir, { recursive: true, force: true });
  }

  copyMetadataSync(metadataDir, targetDir);
}

function copyMetadataSync(src: string, dest: string): void {
  mkdirSync(dest, { recursive: true });
  const entries = readdirSync(src, { withFileTypes: true });
  for (const entry of entries) {
    const srcPath = join(src, entry.name);
    const destPath = join(dest, entry.name);
    if (entry.isDirectory()) {
      copyMetadataSync(srcPath, destPath);
    } else if (entry.isFile() && entry.name.endsWith(".json")) {
      copyFileSync(srcPath, destPath);
    }
  }
}
