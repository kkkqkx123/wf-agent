import { spawn, ChildProcess } from "child_process";
import { mkdirSync, writeFileSync, existsSync } from "fs";
import { join, resolve, dirname } from "path";

export interface CLIRunResult {
  exitCode: number | null;
  stdout: string;
  stderr: string;
  duration: number;
  outputFilePath?: string;
}

export interface CLIRunOptions {
  timeout?: number;
  input?: string;
  cwd?: string;
  env?: Record<string, string>;
  saveOutput?: boolean;
  outputSubdir?: string;
}

export class CLIRunner {
  private cliPath: string;
  private outputDir: string;
  private storageDir: string | undefined;
  private defaultEnv: Record<string, string>;
  private outputFileCounter: number;
  private testConfigPath: string;

  constructor(cliPath?: string, outputDir?: string, storageDir?: string) {
    this.cliPath = cliPath || this.findCLIPath();
    this.outputDir = outputDir || resolve(__dirname, "../outputs");
    this.storageDir = storageDir;
    this.testConfigPath = resolve(__dirname, "../config/test-config.toml");
    this.defaultEnv = {
      ...process.env,
      NODE_ENV: "test",
      TEST_MODE: "true",
      LOG_DIR: this.outputDir,
      DISABLE_LOG_TERMINAL: "true",
      DISABLE_SDK_LOGS: "true",
      SDK_LOG_LEVEL: "silent",
    };
    this.outputFileCounter = 0;
  }

  setStorageDir(storageDir: string | undefined): void {
    this.storageDir = storageDir;
  }

  async run(args: string[], options: CLIRunOptions = {}): Promise<CLIRunResult> {
    const {
      timeout = 30000,
      input,
      cwd = process.cwd(),
      env = {},
      saveOutput = true,
      outputSubdir = "general",
    } = options;

    // Automatically add test config if not already present
    const finalArgs = args.includes("--config") ? args : [...args, "--config", this.testConfigPath];

    const startTime = Date.now();
    const result = await this.executeCommand(finalArgs, {
      timeout,
      input,
      cwd,
      env,
    });

    result.duration = Date.now() - startTime;

    if (saveOutput) {
      const outputFilePath = await this.saveOutput(result, args, outputSubdir);
      result.outputFilePath = outputFilePath;
    }

    return result;
  }

  private async executeCommand(
    args: string[],
    options: { timeout: number; input?: string; cwd: string; env: Record<string, string> },
  ): Promise<CLIRunResult> {
    return new Promise(resolve => {
      // Merge env with STORAGE_DIR if set
      const env = { ...this.defaultEnv, ...options.env };
      if (this.storageDir && !env["STORAGE_DIR"]) {
        env["STORAGE_DIR"] = this.storageDir;
      }
      const child = spawn("node", [this.cliPath, ...args], {
        env,
        cwd: options.cwd,
        stdio: ["pipe", "pipe", "pipe"],
      });

      let stdout = "";
      let stderr = "";

      child.stdout?.on("data", data => {
        stdout += data.toString();
      });

      child.stderr?.on("data", data => {
        stderr += data.toString();
      });

      if (options.input && child.stdin) {
        child.stdin.write(options.input);
        child.stdin.end();
      }

      const timer = setTimeout(() => {
        child.kill();
        resolve({
          exitCode: -1,
          stdout,
          stderr: `Timeout after ${options.timeout}ms`,
          duration: 0,
        });
      }, options.timeout);

      child.on("close", code => {
        clearTimeout(timer);
        resolve({
          exitCode: code,
          stdout,
          stderr,
          duration: 0,
        });
      });

      child.on("error", error => {
        clearTimeout(timer);
        resolve({
          exitCode: -1,
          stdout,
          stderr: error.message,
          duration: 0,
        });
      });
    });
  }

  private async saveOutput(result: CLIRunResult, args: string[], subdir: string): Promise<string> {
    const outputDir = join(this.outputDir, subdir);
    mkdirSync(outputDir, { recursive: true });

    this.outputFileCounter++;
    // Sanitize filename: replace path separators and special chars
    const sanitizedArgs = args
      .map(arg => arg.replace(/[\\/:\*\?"<>\|]/g, "_"))
      .join("_")
      .substring(0, 100); // Limit length
    const filename = `${String(this.outputFileCounter).padStart(3, "0")}_${sanitizedArgs}.log`;
    const filepath = join(outputDir, filename);

    const content = this.formatOutput(result, args);
    writeFileSync(filepath, content, "utf-8");

    return filepath;
  }

  private formatOutput(result: CLIRunResult, args: string[]): string {
    const lines: string[] = [];

    lines.push("=".repeat(80));
    lines.push(`Command: modular-agent ${args.join(" ")}`);
    lines.push("=".repeat(80));
    lines.push("");

    lines.push("Metadata:");
    lines.push(`  Exit Code: ${result.exitCode}`);
    lines.push(`  Duration: ${result.duration}ms`);
    lines.push(`  Timestamp: ${new Date().toISOString()}`);
    lines.push("");

    lines.push("STDOUT:");
    lines.push("-".repeat(80));
    lines.push(result.stdout || "(empty)");
    lines.push("");

    lines.push("STDERR:");
    lines.push("-".repeat(80));
    lines.push(result.stderr || "(empty)");
    lines.push("");

    return lines.join("\n");
  }

  private findCLIPath(): string {
    const possiblePaths = [
      resolve(__dirname, "../../scripts/modular-agent.js"),
      resolve(__dirname, "../../../scripts/modular-agent.js"),
      resolve(__dirname, "../../../../scripts/modular-agent.js"),
    ];

    for (const path of possiblePaths) {
      if (existsSync(path)) {
        return path;
      }
    }

    return "./scripts/modular-agent.js";
  }
}

export const runner = new CLIRunner();

export function runCLI(args: string[], options?: CLIRunOptions): Promise<CLIRunResult> {
  return runner.run(args, options);
}
