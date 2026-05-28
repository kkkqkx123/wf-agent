/**
 * JavaScript VM Context Strategy
 *
 * Uses Node.js built-in `node:vm` module to create a restricted execution context.
 * Provides module whitelist/blacklist, read-only fs proxy, safe console, and
 * timeout enforcement.
 *
 * Architecture reference: docs/infra/sandbox/strategies/javascript-sandbox.md
 */

import vm from "node:vm";
import type { SandboxPolicy, JavaScriptPolicy, ScriptExecutionResult, StrategyExecuteOptions, VFSProvider } from "@wf-agent/types";
import type { StrategyImplementation } from "../types.js";

/**
 * Default denied JS modules when policy.javascript.deniedModules is empty.
 */
const DEFAULT_DENIED_MODULES: string[] = [
  "child_process",
  "cluster",
  "worker_threads",
  "v8",
  "vm",
  "inspector",
  "module",
  "process",
];

/**
 * Read-only file system operation names that should be blocked.
 */
const READONLY_FS_BLOCKED: Set<string> = new Set([
  "writeFile",
  "writeFileSync",
  "appendFile",
  "appendFileSync",
  "mkdir",
  "mkdirSync",
  "rmdir",
  "rmdirSync",
  "unlink",
  "unlinkSync",
  "rename",
  "renameSync",
  "chmod",
  "chmodSync",
  "copyFile",
  "copyFileSync",
  "symlink",
  "symlinkSync",
  "truncate",
  "truncateSync",
  "ftruncate",
  "ftruncateSync",
  "fchmod",
  "fchmodSync",
  "fchown",
  "fchownSync",
  "futimes",
  "futimesSync",
  "lutimes",
  "lutimesSync",
  "link",
  "linkSync",
  "mkdtemp",
  "mkdtempSync",
  "open",
  "openSync",
  "close",
  "closeSync",
  "fsync",
  "fsyncSync",
  "fdatasync",
  "fdatasyncSync",
]);

/**
 * JavaScript VM Context Strategy
 *
 * Creates a restricted vm.Context with:
 *   - Module whitelist/blacklist for require()
 *   - Read-only fs proxy (blocks write operations)
 *   - Safe console (captures output to buffer)
 *   - Restricted setTimeout/setInterval
 *   - Disabled eval, Function, global, globalThis
 *   - Safe process and Buffer subsets
 */
export class JavaScriptVmContextStrategy implements StrategyImplementation<ScriptExecutionResult> {
  id = "vm-context";
  name = "JavaScript VM Context";
  description = "JavaScript sandbox using Node.js vm.createContext with restricted globals";
  priority = 30;

  isAvailable(): boolean {
    return true;
  }

  async execute(
    options: StrategyExecuteOptions,
    policy: SandboxPolicy,
  ): Promise<ScriptExecutionResult> {
    const startTime = Date.now();
    const code = options.command;
    const jsPolicy: JavaScriptPolicy = {
      allowedModules: policy.javascript?.allowedModules ?? [],
      deniedModules: policy.javascript?.deniedModules ?? DEFAULT_DENIED_MODULES,
      allowChildProcess: policy.javascript?.allowChildProcess ?? false,
      allowFSWrite: policy.javascript?.allowFSWrite ?? false,
      allowDynamicEval: policy.javascript?.allowDynamicEval ?? false,
    };

    if (!code) {
      return {
        success: false,
        scriptName: "sandbox-javascript",
        executionTime: Date.now() - startTime,
        error: "Empty JavaScript code",
      };
    }

    const outputBuffer: string[] = [];
    const vfs = options.vfs ?? null;

    const sandboxGlobals: Record<string, unknown> = {
      console: {
        log: (...args: unknown[]) => this.captureOutput(outputBuffer, "log", args),
        error: (...args: unknown[]) => this.captureOutput(outputBuffer, "error", args),
        warn: (...args: unknown[]) => this.captureOutput(outputBuffer, "warn", args),
        info: (...args: unknown[]) => this.captureOutput(outputBuffer, "info", args),
      },
      setTimeout: (fn: () => void, ms: number) => {
        const maxDelay = 30000;
        return setTimeout(fn, Math.min(ms, maxDelay));
      },
      setInterval: (fn: () => void, ms: number) => {
        const maxDelay = 30000;
        return setInterval(fn, Math.min(Math.max(ms, 100), maxDelay));
      },
      clearTimeout: (id: ReturnType<typeof setTimeout>) => clearTimeout(id),
      clearInterval: (id: ReturnType<typeof setInterval>) => clearInterval(id),
      Buffer: {
        from: (data: string, encoding?: BufferEncoding) => Buffer.from(data, encoding),
        isBuffer: (obj: unknown) => Buffer.isBuffer(obj),
        byteLength: (str: string, encoding?: BufferEncoding) => Buffer.byteLength(str, encoding),
        concat: (list: Uint8Array[], totalLength?: number) => Buffer.concat(list, totalLength),
      },
      process: {
        env: { NODE_ENV: "sandbox" },
        cwd: () => "/workspace",
        argv: ["sandbox"],
        pid: -1,
        ppid: -1,
        platform: process.platform,
        arch: process.arch,
        version: process.version,
        versions: { ...process.versions },
        hrtime: (time?: [number, number]) => (time ? process.hrtime(time) : process.hrtime()),
        uptime: () => 0,
        memoryUsage: () => ({ rss: 0, heapTotal: 0, heapUsed: 0, external: 0 }),
      },
      require: (moduleName: string) => this.restrictedRequire(moduleName, jsPolicy, vfs),
      global: {},
      globalThis: {},
      eval: undefined,
      Function: undefined as unknown,
    };

    // Expose VFS as a sandbox global when available
    if (vfs) {
      sandboxGlobals["vfs"] = this.createVFSHelper(vfs);
    }

    const sandbox = vm.createContext(sandboxGlobals);

    try {
      vm.runInNewContext(code, sandbox, {
        timeout: options.timeout ?? policy.resource?.timeoutLimit ?? 30000,
        filename: "sandbox.js",
        breakOnSigint: true,
      });

      return {
        success: true,
        scriptName: "sandbox-javascript",
        stdout: outputBuffer.join("\n"),
        executionTime: Date.now() - startTime,
        exitCode: 0,
      };
    } catch (error) {
      if (error instanceof Error && "code" in error && error.code === "ERR_SCRIPT_EXECUTION_TIMEOUT") {
        return {
          success: false,
          scriptName: "sandbox-javascript",
          executionTime: Date.now() - startTime,
          error: "Script execution timeout",
          stderr: outputBuffer.join("\n"),
        };
      }
      return {
        success: false,
        scriptName: "sandbox-javascript",
        executionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
        stderr: outputBuffer.join("\n"),
      };
    }
  }

  /**
   * Capture console output into the buffer.
   */
  private captureOutput(buffer: string[], _level: string, args: unknown[]): void {
    const line = args.map(a => (typeof a === "object" ? JSON.stringify(a, null, 2) : String(a))).join(" ");
    buffer.push(line);
  }

  /**
   * Restricted require with module whitelist/blacklist and readonly fs proxy.
   */
  private restrictedRequire(moduleName: string, policy: JavaScriptPolicy, vfs: VFSProvider | null): unknown {
    const denylist = new Set(policy.deniedModules);
    if (policy.deniedModules.length === 0) {
      for (const m of DEFAULT_DENIED_MODULES) {
        denylist.add(m);
      }
    }

    if (policy.allowedModules.length > 0 && !policy.allowedModules.includes(moduleName)) {
      throw new Error(`Module not allowed: ${moduleName}`);
    }

    if (denylist.has(moduleName)) {
      throw new Error(`Module denied: ${moduleName}`);
    }

    if (moduleName === "child_process" && !policy.allowChildProcess) {
      throw new Error("child_process module is not allowed");
    }

    if (moduleName === "fs" && !policy.allowFSWrite) {
      return vfs ? this.createVFSBackedFS(vfs) : this.createReadonlyFS();
    }

    try {
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      return require(moduleName);
    } catch {
      throw new Error(`Module not found: ${moduleName}`);
    }
  }

  /**
   * Create a read-only proxy for the fs module.
   */
  private createReadonlyFS(): Record<string, unknown> {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const fs = require("fs") as Record<string, unknown>;

    return new Proxy(fs, {
      get: (target, prop: string) => {
        if (READONLY_FS_BLOCKED.has(prop)) {
          return () => {
            throw new Error(`Read-only filesystem: ${String(prop)} not allowed`);
          };
        }
        return target[prop];
      },
    });
  }

  /**
   * Create a VFS-backed fs proxy for the sandbox.
   * Routes file operations through the VFS overlay instead of the real filesystem.
   * Async methods (readFile, writeFile, stat) return Promises.
   * Sync methods throw with a message directing to use the async API.
   */
  private createVFSBackedFS(vfs: VFSProvider): Record<string, unknown> {
    const fsProxy: Record<string, unknown> = {};

    // Read operations — delegate to VFS, return Promise
    fsProxy["readFile"] = async (path: string, ..._args: unknown[]) => {
      const data = await vfs.readFile(path);
      if (data === null) {
        throw new Error(`ENOENT: no such file or directory, open '${path}'`);
      }
      return data;
    };

    fsProxy["stat"] = async (path: string) => {
      const entry = await vfs.stat(path);
      if (entry === null) {
        throw new Error(`ENOENT: no such file or directory, stat '${path}'`);
      }
      return entry;
    };

    fsProxy["exists"] = vfs.exists.bind(vfs);

    // Write operations — delegate to VFS delta layer, return Promise
    fsProxy["writeFile"] = async (path: string, data: Uint8Array | string) => {
      const buf = typeof data === "string" ? Buffer.from(data, "utf-8") : Buffer.from(data);
      await vfs.writeFile(path, buf);
    };

    fsProxy["appendFile"] = async (path: string, data: Uint8Array | string) => {
      const existing = await vfs.readFile(path);
      const existingBuf = existing ? Buffer.from(existing) : Buffer.alloc(0);
      const newData = typeof data === "string" ? Buffer.from(data, "utf-8") : Buffer.from(data);
      await vfs.writeFile(path, Buffer.concat([existingBuf, newData]));
    };

    // Read directory
    fsProxy["readdir"] = async (path: string) => {
      // VFSProvider doesn't have readdir, so fall back to real fs readdir
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      const realFs = require("fs") as { readdir: (p: string, cb: (e: Error | null, f: string[]) => void) => void };
      return new Promise<string[]>((resolve, reject) => {
        realFs.readdir(path, (err, files) => {
          if (err) reject(err);
          else resolve(files);
        });
      });
    };

    // Sync methods are not supported with async VFS — throw clear error
    const syncBlocked = [
      "readFileSync", "writeFileSync", "appendFileSync", "statSync",
      "existsSync", "readdirSync", "mkdirSync", "rmdirSync",
      "unlinkSync", "renameSync", "copyFileSync", "chmodSync",
      "openSync", "closeSync", "fsyncSync",
    ];
    for (const method of syncBlocked) {
      fsProxy[method] = () => {
        throw new Error(
          `VFS-backed filesystem: ${method} is not supported. Use the async version (${method.replace("Sync", "")}) instead.`,
        );
      };
    }

    // mkdir, rmdir, unlink — throw clear message (VFSProvider doesn't expose these)
    const forbidden = ["mkdir", "rmdir", "unlink", "rename", "chmod", "copyFile", "truncate"];
    for (const method of forbidden) {
      fsProxy[method] = () => {
        throw new Error(`VFS-backed filesystem: ${method} is not supported via require('fs'). Use the vfs sandbox global instead.`);
      };
    }

    return fsProxy;
  }

  /**
   * Create a VFS helper object exposing VFS operations as sandbox globals.
   * Available as `vfs.readFile(path)`, `vfs.writeFile(path, data)`, etc.
   * All methods return Promises.
   */
  private createVFSHelper(vfs: VFSProvider): Record<string, unknown> {
    return {
      readFile: async (path: string) => {
        const data = await vfs.readFile(path);
        if (data === null) {
          throw new Error(`ENOENT: no such file or directory, read '${path}'`);
        }
        return data;
      },
      writeFile: async (path: string, data: Uint8Array | string) => {
        const buf = typeof data === "string" ? Buffer.from(data, "utf-8") : Buffer.from(data);
        await vfs.writeFile(path, buf);
      },
      stat: async (path: string) => {
        const entry = await vfs.stat(path);
        if (entry === null) {
          throw new Error(`ENOENT: no such file or directory, stat '${path}'`);
        }
        return entry;
      },
      exists: async (path: string) => vfs.exists(path),
    };
  }
}