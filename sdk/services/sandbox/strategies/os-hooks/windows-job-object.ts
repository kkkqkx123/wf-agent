/**
 * Windows Job Object Strategy
 *
 * Provides process-group isolation and resource limits via Win32 Job Object API,
 * accessed through the koffi FFI library (optional dependency).
 *
 * Falls back to passthrough when koffi is not installed.
 */

import type { SandboxPolicy, ScriptExecutionResult, StrategyExecuteOptions } from "@wf-agent/types";
import type { StrategyImplementation } from "../../types.js";
import { getTerminalService, type TerminalService } from "../../../terminal/index.js";
import { spawn } from "child_process";
import { executePassthrough, recordAudit } from "./base.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "WindowsJobObjectStrategy" });

export class WindowsJobObjectStrategy implements StrategyImplementation<ScriptExecutionResult> {
  id = "windows-job";
  name = "Windows Job Object (OS Hook)";
  description = "Windows Job Object process group isolation with resource limits via koffi FFI";
  priority = 50;

  private terminalService: TerminalService;
  /** Lazily initialised on first use (Windows only) */
  private koffiBinding: WindowsJobObjectBinding | null = null;

  constructor(terminalService?: TerminalService) {
    this.terminalService = terminalService || getTerminalService();
  }

  isAvailable(): boolean {
    return process.platform === "win32";
  }

  async execute(
    options: StrategyExecuteOptions,
    policy: SandboxPolicy,
  ): Promise<ScriptExecutionResult> {
    const startTime = Date.now();

    if (!this.isAvailable()) {
      return {
        success: false,
        scriptName: "sandbox-os-hook",
        executionTime: Date.now() - startTime,
        error: "WindowsJobObjectStrategy is only available on Windows",
      };
    }

    // Try to use koffi FFI for Job Object isolation.
    // If koffi is not installed (optional dep), fall back to passthrough.
    const binding = this.getKoffiBinding();
    if (!binding) {
      recordAudit({
        timestamp: startTime,
        strategyId: this.id,
        command: options.command,
        allowed: true,
        reason: "koffi binding unavailable, falling back to passthrough",
      });
      logger.warn("koffi binding unavailable, falling back to passthrough (no OS-level isolation)");
      return executePassthrough(this.terminalService, options, startTime);
    }

    recordAudit({
      timestamp: startTime,
      strategyId: this.id,
      command: options.command,
      allowed: true,
      reason: "executing with Windows Job Object isolation",
    });

    return this.executeWithJobObject(options, policy, binding, startTime);
  }

  // ===========================================================================
  // koffi Binding Management
  // ===========================================================================

  private getKoffiBinding(): WindowsJobObjectBinding | null {
    if (this.koffiBinding) return this.koffiBinding;

    try {
      // Dynamic require to avoid bundling koffi's native binaries.
      // koffi is an optional dependency (see sdk/package.json).
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      const koffi = require("koffi") as KoffiModule;
      const k32 = koffi.load("kernel32.dll");

      // ── Win32 API bindings ────────────────────────────────────────────
      // HANDLE CreateJobObjectW(LPSECURITY_ATTRIBUTES, LPCWSTR lpName)
      const CreateJobObjectW = k32.func("void* __stdcall CreateJobObjectW(void*, char*)");

      // BOOL SetInformationJobObject(HANDLE, int, void*, DWORD)
      const SetInformationJobObject = k32.func("int __stdcall SetInformationJobObject(void*, int, void*, int)");

      // BOOL AssignProcessToJobObject(HANDLE, HANDLE)
      const AssignProcessToJobObject = k32.func("int __stdcall AssignProcessToJobObject(void*, void*)");

      // BOOL TerminateJobObject(HANDLE, UINT)
      const TerminateJobObject = k32.func("int __stdcall TerminateJobObject(void*, unsigned int)");

      // BOOL CloseHandle(HANDLE)
      const CloseHandle = k32.func("int __stdcall CloseHandle(void*)");

      // BOOL OpenProcess(DWORD, BOOL, DWORD)
      const OpenProcess = k32.func("void* __stdcall OpenProcess(unsigned int, int, unsigned int)");

      this.koffiBinding = {
        CreateJobObjectW,
        SetInformationJobObject,
        AssignProcessToJobObject,
        TerminateJobObject,
        CloseHandle,
        OpenProcess,

        // ── Struct helpers ──────────────────────────────────────────────
        // JOBOBJECT_EXTENDED_LIMIT_INFORMATION layout (x64):
        //   +0x00: JOBOBJECT_BASIC_LIMIT_INFORMATION (6 × int64 + 3 × uint32 + uint64)
        //   +0x38: IO counters (ignored, keep zeros)
        //   +0x58: BasicLimitInformation again? (per-process limit field)
        createExtendedLimitStruct(limits: JobLimits): ArrayBuffer {
          const buf = new ArrayBuffer(200); // enough for the full struct
          const view = new DataView(buf);

          // PerProcessUserTimeLimit (int64) at offset 0x08
          // We set it in 100-ns intervals (1 ms = 10000)
          const processTimeMs = limits.processTimeMs ?? 0;
          const jobTimeMs = limits.jobTimeMs ?? 0;
          const activeProcessLimit = limits.activeProcessLimit ?? 0;
          const memoryLimitMb = limits.memoryLimitMb ?? 0;

          // PerProcessUserTimeLimit (int64) at offset 0x08
          // We set it in 100-ns intervals (1 ms = 10000)
          if (processTimeMs > 0) {
            view.setBigInt64(8, BigInt(processTimeMs) * 10000n, true);
          }

          // PerJobUserTimeLimit (int64) at offset 0x10
          if (jobTimeMs > 0) {
            view.setBigInt64(16, BigInt(jobTimeMs) * 10000n, true);
          }

          // LimitFlags (uint32) at offset 0x2C
          let flags = 0;
          if (processTimeMs > 0) flags |= 0x00000040; // JOB_OBJECT_LIMIT_PROCESS_TIME
          if (jobTimeMs > 0) flags |= 0x00000080;    // JOB_OBJECT_LIMIT_JOB_TIME
          if (activeProcessLimit > 0) flags |= 0x00000008; // JOB_OBJECT_LIMIT_ACTIVE_PROCESS
          if (memoryLimitMb > 0) flags |= 0x00000200; // JOB_OBJECT_LIMIT_JOB_MEMORY (Win8+)
          view.setUint32(0x2C, flags, true);

          // ActiveProcessLimit (uint32) at offset 0x30
          if (activeProcessLimit > 0) {
            view.setUint32(0x30, activeProcessLimit, true);
          }

          // JobMemoryLimit (uint64) at offset 0x58
          if (memoryLimitMb > 0) {
            // In bytes
            view.setBigInt64(0x58, BigInt(memoryLimitMb) * 1024n * 1024n, true);
          }

          return buf;
        },
      };

      return this.koffiBinding;
    } catch {
      // koffi not installed — fall back to passthrough
      return null;
    }
  }

  // ===========================================================================
  // Job Object Execution
  // ===========================================================================

  private async executeWithJobObject(
    options: StrategyExecuteOptions,
    policy: SandboxPolicy,
    binding: WindowsJobObjectBinding,
    startTime: number,
  ): Promise<ScriptExecutionResult> {
    // ── Step 1: Create job object ────────────────────────────────────────
    const jobName = `wf-agent-job-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    const hJob = binding.CreateJobObjectW(null, jobName);
    if (!hJob || hJob === -1n || hJob === 0n) {
      return {
        success: false,
        scriptName: "sandbox-os-hook",
        executionTime: Date.now() - startTime,
        error: "Failed to create Windows Job Object",
      };
    }

    try {
      // ── Step 2: Set resource limits from policy ───────────────────────
      const limits: JobLimits = this.policyToJobLimits(policy);
      const limitStruct = binding.createExtendedLimitStruct(limits);
      const infoClass = 9; // JobObjectExtendedLimitInformation

      const setResult = binding.SetInformationJobObject(hJob, infoClass, limitStruct, limitStruct.byteLength);
      if (!setResult) {
        return {
          success: false,
          scriptName: "sandbox-os-hook",
          executionTime: Date.now() - startTime,
          error: "Failed to set Job Object limits",
        };
      }

      // ── Step 3: Spawn child process and assign to job ─────────────────
      const execResult = await this.spawnAndAssign(options, binding, hJob, startTime);
      return execResult;
    } finally {
      // Always clean up the job handle
      try {
        binding.CloseHandle(hJob);
      } catch {
        // Swallow close errors
      }
    }
  }

  /**
   * Spawn a child process and immediately assign it to the job object.
   *
   * Uses child_process.spawn (not TerminalService) so we get the real
   * process handle for AssignProcessToJobObject.
   */
  private async spawnAndAssign(
    options: StrategyExecuteOptions,
    binding: WindowsJobObjectBinding,
    hJob: any,
    startTime: number,
  ): Promise<ScriptExecutionResult> {
    return new Promise((resolvePromise) => {
      // Use cmd.exe /c to wrap the command (so echo, dir, etc. work)
      const isCmd = !options.command.includes(" ") && !options.command.includes(".");
      const spawnCmd = process.env["COMSPEC"] || "cmd.exe";
      const spawnArgs = isCmd ? ["/c", options.command] : ["/c", options.command];

      const child = spawn(spawnCmd, spawnArgs, {
        cwd: options.cwd,
        env: options.env as Record<string, string>,
        stdio: ["ignore", "pipe", "pipe"],
        windowsHide: true,
      });

      // Assign child process to the job object immediately
      const PROCESS_ALL_ACCESS = 0x1F0FFF;
      const hProcess = binding.OpenProcess(PROCESS_ALL_ACCESS, 0, child.pid!);
      if (hProcess) {
        binding.AssignProcessToJobObject(hJob, hProcess);
        binding.CloseHandle(hProcess);
      }

      let stdout = "";
      let stderr = "";

      child.stdout?.on("data", (data: Buffer) => { stdout += data.toString(); });
      child.stderr?.on("data", (data: Buffer) => { stderr += data.toString(); });

      // Resource limit timeout
      let timeoutHandle: NodeJS.Timeout | undefined;
      const timeoutMs = options.timeout ?? 30000;

      const cleanup = () => {
        if (timeoutHandle) clearTimeout(timeoutHandle);
      };

      if (timeoutMs > 0) {
        timeoutHandle = setTimeout(() => {
          // Terminate the entire job (kills all associated processes)
          try {
            binding.TerminateJobObject(hJob, 1);
          } catch {
            // Best-effort termination
          }
          cleanup();
          resolvePromise({
            success: false,
            scriptName: "sandbox-os-hook",
            stdout,
            stderr,
            exitCode: -1,
            executionTime: Date.now() - startTime,
            error: `Process timed out after ${timeoutMs}ms`,
          });
        }, timeoutMs);
      }

      child.on("close", (code) => {
        cleanup();
        resolvePromise({
          success: code === 0,
          scriptName: "sandbox-os-hook",
          stdout,
          stderr,
          exitCode: code ?? -1,
          executionTime: Date.now() - startTime,
        });
      });

      child.on("error", (err) => {
        cleanup();
        resolvePromise({
          success: false,
          scriptName: "sandbox-os-hook",
          stdout,
          stderr,
          exitCode: -1,
          executionTime: Date.now() - startTime,
          error: err.message,
        });
      });
    });
  }

  /**
   * Convert SandboxPolicy resource limits into JobLimits structure.
   */
  private policyToJobLimits(policy: SandboxPolicy): JobLimits {
    const limits: JobLimits = {};

    if (policy.resource?.timeoutLimit) {
      // Use as per-process time limit (ms)
      limits.processTimeMs = policy.resource.timeoutLimit;
    }

    return limits;
  }
}

// =========================================================================
// Internal Types (koffi FFI bindings)
// =========================================================================

interface KoffiModule {
  load(lib: string): KoffiLibrary;
}

interface KoffiLibrary {
  func(signature: string): KoffiFunction;
}

type KoffiFunction = (...args: any[]) => any;

interface JobLimits {
  /** Per-process user time limit in milliseconds */
  processTimeMs?: number;
  /** Per-job total time limit in milliseconds */
  jobTimeMs?: number;
  /** Maximum number of active processes in the job */
  activeProcessLimit?: number;
  /** Per-job memory limit in MB */
  memoryLimitMb?: number;
}

interface WindowsJobObjectBinding {
  CreateJobObjectW: (attrs: any, name: string) => any;
  SetInformationJobObject: (handle: any, infoClass: number, data: ArrayBuffer, size: number) => number;
  AssignProcessToJobObject: (handle: any, processHandle: any) => number;
  TerminateJobObject: (handle: any, exitCode: number) => number;
  CloseHandle: (handle: any) => number;
  OpenProcess: (desiredAccess: number, inheritHandle: number, pid: number) => any;
  createExtendedLimitStruct: (limits: JobLimits) => ArrayBuffer;
}
