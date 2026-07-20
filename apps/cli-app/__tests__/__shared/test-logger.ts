import { mkdirSync, appendFileSync } from "fs";
import { join, dirname } from "path";
import type { CLIRunResult } from "./cli-runner.js";

export type TestStatus = "passed" | "failed" | "skipped";

export interface CommandLog {
  command: string[];
  exitCode: number;
  duration: number;
  stdoutPreview: string;
  stderrPreview: string;
  outputFile: string;
}

export interface TestLog {
  timestamp: string;
  testSuite: string;
  testName: string;
  status: TestStatus;
  duration: number;
  commands: CommandLog[];
  error?: {
    message: string;
    stack?: string;
  };
  outputFiles: string[];
}

export interface TestSummary {
  total: number;
  passed: number;
  failed: number;
  skipped: number;
  duration: number;
  timestamp: string;
}

export class TestLogger {
  private logs: TestLog[] = [];
  private currentTest: TestLog | null = null;
  private logFilePath: string;

  constructor(outputDir: string) {
    this.logFilePath = join(outputDir, "test-logs.jsonl");
    mkdirSync(dirname(this.logFilePath), { recursive: true });
  }

  startTest(testSuite: string, testName: string): void {
    this.currentTest = {
      timestamp: new Date().toISOString(),
      testSuite,
      testName,
      status: "passed",
      duration: 0,
      commands: [],
      outputFiles: [],
    };
  }

  recordCommand(command: string[], result: CLIRunResult): void {
    if (!this.currentTest) return;

    this.currentTest.commands.push({
      command,
      exitCode: result.exitCode!,
      duration: result.duration,
      stdoutPreview: result.stdout.substring(0, 500),
      stderrPreview: result.stderr.substring(0, 500),
      outputFile: result.outputFilePath || "",
    });

    if (result.outputFilePath) {
      this.currentTest.outputFiles.push(result.outputFilePath);
    }
  }

  endTest(status: TestStatus, error?: Error): void {
    if (!this.currentTest) return;

    this.currentTest.status = status;
    this.currentTest.duration = Date.now() - new Date(this.currentTest.timestamp).getTime();

    if (error) {
      this.currentTest.error = {
        message: error.message,
        stack: error.stack,
      };
    }

    this.appendLog(this.currentTest);
    this.logs.push(this.currentTest);
    this.currentTest = null;
  }

  private appendLog(log: TestLog): void {
    const line = JSON.stringify(log) + "\n";
    appendFileSync(this.logFilePath, line, "utf-8");
  }

  getLogs(): TestLog[] {
    return this.logs;
  }

  getSummary(): TestSummary {
    const total = this.logs.length;
    const passed = this.logs.filter(l => l.status === "passed").length;
    const failed = this.logs.filter(l => l.status === "failed").length;
    const skipped = this.logs.filter(l => l.status === "skipped").length;

    return {
      total,
      passed,
      failed,
      skipped,
      duration: this.logs.reduce((sum, log) => sum + log.duration, 0),
      timestamp: new Date().toISOString(),
    };
  }

  clearLogs(): void {
    this.logs = [];
  }
}
