import { describe, it, expect, beforeEach, afterEach, beforeAll } from "vitest";
import { CLIRunner, TestHelper, createTestHelper } from "../__shared/index.js";
import { resolve } from "path";

describe("Test Framework Validation", () => {
  let helper: TestHelper;
  let runner: CLIRunner;
  const testOutputDir = resolve(__dirname, "../outputs/test-framework");

  beforeAll(() => {
    runner = new CLIRunner(undefined, testOutputDir);
  });

  beforeEach(() => {
    helper = createTestHelper("test-framework", testOutputDir);
  });

  afterEach(async () => {
    await helper.cleanup();
  });

  it("should execute CLI help command successfully", async () => {
    const result = await runner.run(["--help"], {
      outputSubdir: "test-framework",
    });

    expect(result.exitCode).toBe(0);
    expect(result.stdout).toBeTruthy();
    expect(result.stderr).toBe("");
    expect(result.duration).toBeGreaterThan(0);
    expect(result.outputFilePath).toBeTruthy();
  });

  it("should create and use temporary directory", async () => {
    const tempDir = helper.getTempDir();
    expect(tempDir).toBeTruthy();
    expect(tempDir).toContain("test-framework");

    const testContent = "Test content for temporary file";
    const tempFile = await helper.writeTempFile("test.txt", testContent);
    expect(tempFile).toBeTruthy();
    expect(helper.existsTempFile("test.txt")).toBe(true);

    const readContent = helper.readTempFile("test.txt");
    expect(readContent).toBe(testContent);
  });

  it("should handle fixture files correctly", async () => {
    const fixtureContent = '[workflow]\nname = "test-workflow"\ndescription = "Test fixture"';
    const fixturePath = await helper.writeFixture("test.toml", fixtureContent, "workflows");
    expect(fixturePath).toBeTruthy();
    expect(helper.existsFixture("workflows", "test.toml")).toBe(true);

    const readContent = helper.readFixture("workflows", "test.toml");
    expect(readContent).toBe(fixtureContent);
  });

  it("should extract IDs from output using patterns", async () => {
    const testOutput = "Agent Loop has been initiated: agent-12345";
    const extractedId = helper.extractId(testOutput, /Agent Loop has been initiated: ([\w-]+)/);
    expect(extractedId).toBe("agent-12345");

    const notFoundId = helper.extractId(testOutput, /Workflow ID: ([\w-]+)/);
    expect(notFoundId).toBe(null);
  });

  it("should record and log test execution details", async () => {
    const result = await runner.run(["--version"], {
      outputSubdir: "test-framework",
    });

    expect(result.exitCode).toBe(0);
    expect(result.duration).toBeGreaterThan(0);
    expect(result.outputFilePath).toBeTruthy();
  });

  it("should handle timeout gracefully", async () => {
    const result = await runner.run(["--help"], {
      timeout: 1000,
      outputSubdir: "test-framework",
    });

    expect(result).toBeDefined();
    expect(result.duration).toBeLessThan(2000);
  });

  it("should preserve test isolation", async () => {
    const helper1 = createTestHelper("isolation-test-1", testOutputDir);
    const helper2 = createTestHelper("isolation-test-2", testOutputDir);

    const file1 = await helper1.writeTempFile("test.txt", "content-1");
    const file2 = await helper2.writeTempFile("test.txt", "content-2");

    expect(file1).not.toBe(file2);
    expect(helper1.readTempFile("test.txt")).toBe("content-1");
    expect(helper2.readTempFile("test.txt")).toBe("content-2");

    await helper1.cleanup();
    await helper2.cleanup();
  });
});