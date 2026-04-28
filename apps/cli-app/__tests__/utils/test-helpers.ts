import { mkdirSync, rmSync, existsSync, writeFileSync, readFileSync } from "fs";
import { join, resolve } from "path";

export class TestHelper {
  private tempDir: string;
  private storageDir: string;

  constructor(testName: string, sharedOutputDir?: string) {
    // Enforce the use of a shared output directory to ensure that all test outputs are located in a consistent location.
    const outputDir = sharedOutputDir || resolve(__dirname, "../outputs");
    this.tempDir = join(outputDir, testName);
    // Create isolated storage directory for each test to ensure test isolation
    // Storage should be under outputs/storage/{testName} for proper isolation
    this.storageDir = resolve(__dirname, "../storage", testName);
    mkdirSync(this.tempDir, { recursive: true });
    mkdirSync(this.storageDir, { recursive: true });
  }

  getTempDir(): string {
    return this.tempDir;
  }

  getStorageDir(): string {
    return this.storageDir;
  }

  getFixturePath(...parts: string[]): string {
    return join(__dirname, "../fixtures", ...parts);
  }

  async cleanup(): Promise<void> {
    if (existsSync(this.tempDir)) {
      rmSync(this.tempDir, { recursive: true, force: true });
    }
    if (existsSync(this.storageDir)) {
      rmSync(this.storageDir, { recursive: true, force: true });
    }
  }

  extractId(output: string, pattern: RegExp): string | null {
    const match = output.match(pattern);
    return match?.[1] ?? null;
  }

  async writeTempFile(filename: string, content: string): Promise<string> {
    const filepath = join(this.tempDir, filename);
    writeFileSync(filepath, content, "utf-8");
    return filepath;
  }

  readTempFile(filename: string): string {
    const filepath = join(this.tempDir, filename);
    return readFileSync(filepath, "utf-8");
  }

  async writeFixture(filename: string, content: string, ...pathParts: string[]): Promise<string> {
    const fixtureDir = join(__dirname, "../fixtures", ...pathParts);
    mkdirSync(fixtureDir, { recursive: true });
    const filepath = join(fixtureDir, filename);
    writeFileSync(filepath, content, "utf-8");
    return filepath;
  }

  readFixture(...parts: string[]): string {
    const filepath = join(__dirname, "../fixtures", ...parts);
    return readFileSync(filepath, "utf-8");
  }

  existsTempFile(filename: string): boolean {
    const filepath = join(this.tempDir, filename);
    return existsSync(filepath);
  }

  existsFixture(...parts: string[]): boolean {
    const filepath = join(__dirname, "../fixtures", ...parts);
    return existsSync(filepath);
  }
}

export function createTestHelper(testName: string, sharedOutputDir?: string): TestHelper {
  return new TestHelper(testName, sharedOutputDir);
}
