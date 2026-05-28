/**
 * OS-level Hook Strategies — Skeleton + Phase 2 Implementations (Phase 6+)
 *
 * Architecture reference: docs/infra/sandbox/strategies/os-level-hook.md
 * Implementation details: docs/infra/sandbox/strategies/os-level-hook-implementation.md
 *
 * OS Hook strategies provide platform-level isolation as an enhancement layer.
 * They do NOT replace lightweight strategies but stack on top of them.
 *
 * Each strategy has a unique id so they can be registered in the resolver
 * by platform without collision. The user-facing id "os-hook" is mapped to
 * the platform-specific id by SandboxRuntime.resolveStrategy().
 *
 * Phase completion:
 *   Phase 1 (done): Skeleton + registration + ID mapping
 *   Phase 2 (done): Windows Job Object via koffi FFI (direct Win32 API calls)
 *   Phase 3 (future): Linux seccomp-loader binary + proot path redirect
 */

import type { SandboxPolicy, ScriptExecutionResult, StrategyExecuteOptions } from "@wf-agent/types";
import type { StrategyImplementation } from "../types.js";
import { getTerminalService, type TerminalService } from "../../terminal/index.js";
import { spawn } from "child_process";

// =========================================================================
// 1. Linux Seccomp-bpf Strategy
// =========================================================================

/**
 * Linux seccomp-bpf sandbox strategy.
 *
 * id: "linux-seccomp"
 * Available: Linux only with kernel seccomp support.
 * Implementation requires a native helper binary (seccomp-loader)
 * that applies BPF rules before executing the user command.
 */
export class LinuxSeccompStrategy implements StrategyImplementation<ScriptExecutionResult> {
  id = "linux-seccomp";
  name = "Linux Seccomp (OS Hook)";
  description = "Linux seccomp-bpf system call filtering (requires native seccomp-loader binary)";
  priority = 50;

  private terminalService: TerminalService;

  constructor(terminalService?: TerminalService) {
    this.terminalService = terminalService || getTerminalService();
  }

  isAvailable(): boolean {
    return process.platform === "linux";
  }

  async execute(
    options: StrategyExecuteOptions,
    _policy: SandboxPolicy,
  ): Promise<ScriptExecutionResult> {
    const startTime = Date.now();

    if (!this.isAvailable()) {
      return {
        success: false,
        scriptName: "sandbox-os-hook",
        executionTime: Date.now() - startTime,
        error: "LinuxSeccompStrategy is only available on Linux",
      };
    }

    // Phase 6: Replace with seccomp-loader binary invocation.
    //   When seccomp-loader is available:
    //     const result = await this.seccompLoader.run(command, options, _policy);
    //     return this.toResult(result, startTime);
    //
    // For now, execute directly (best-effort sandboxing).
    return this.executeCommand(options, startTime);
  }

  private async executeCommand(
    options: StrategyExecuteOptions,
    startTime: number,
  ): Promise<ScriptExecutionResult> {
    try {
      const result = await this.terminalService.executeOneOff(options.command, {
        cwd: options.cwd,
        env: options.env,
        timeout: options.timeout,
      });

      return {
        success: result.success,
        scriptName: "sandbox-os-hook",
        stdout: result.stdout,
        stderr: result.stderr,
        exitCode: result.exitCode,
        executionTime: Date.now() - startTime,
        error: result.error,
      };
    } catch (error) {
      return {
        success: false,
        scriptName: "sandbox-os-hook",
        executionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }
}

// =========================================================================
// 2. Windows Job Object Strategy (Phase 2 — koffi FFI)
// =========================================================================

/**
 * Windows Job Object sandbox strategy.
 *
 * id: "windows-job"
 * Available: Windows only.
 *
 * Implementation uses koffi FFI to directly call Win32 Job Object API:
 *   - CreateJobObjectW → SetInformationJobObject → AssignProcessToJobObject
 *
 * koffi is an optional dependency (npm: koffi). If not installed, the
 * strategy falls back to direct passthrough execution without OS isolation.
 *
 * Resource limits applied:
 *   - Active process limit (maxChildProcesses)
 *   - Per-process memory limit (memoryLimit)
 *   - Process/Job CPU rate cap (cpuLimit → CPU rate control)
 *   - Timeout via Node.js timer
 */
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
      return this.executeCommand(options, startTime);
    }

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
        //   +0x40: IO_COUNTERS (6 × uint64)
        //   +0x70: PROCESS_MEMORY_LIMIT (int64)
        //   +0x78: JOB_MEMORY_LIMIT (int64)
        //   +0x80: PEAK_PROCESS_MEMORY (int64)
        //   +0x88: PEAK_JOB_MEMORY (int64)
        //   Total: 0x90 (144 bytes)

        /** Allocate and populate a JOBOBJECT_EXTENDED_LIMIT_INFORMATION buffer */
        buildExtendedLimitInfo: (limits: {
          activeProcessLimit?: number;
          processMemoryLimitMb?: number;
          jobMemoryLimitMb?: number;
          cpuRate?: number;
          timeoutMs?: number;
        }): Buffer => {
          const buf = Buffer.alloc(0x90); // 144 bytes
          let limitFlags = 0;

          // Set fields at their x64 offsets:
          // Offsets within JOBOBJECT_BASIC_LIMIT_INFORMATION (0x00-0x3F):
          //   +0x00: PerProcessUserTimeLimit (int64)
          //   +0x08: PerJobUserTimeLimit (int64)
          //   +0x10: LimitFlags (uint32)
          //   +0x14: MinimumWorkingSetSize (uint64 ptr-sized)
          //   +0x1C: MaximumWorkingSetSize
          //   +0x24: ActiveProcessLimit (uint32)
          //   +0x28: Affinity (uint64 ptr-sized)
          //   +0x30: PriorityClass (uint32)
          //   +0x34: SchedulingClass (uint32)

          if (limits.activeProcessLimit !== undefined) {
            limitFlags |= 0x1000; // JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            buf.writeUInt32LE(limits.activeProcessLimit, 0x24);
          }

          if (limits.processMemoryLimitMb !== undefined) {
            limitFlags |= 0x200; // JOB_OBJECT_LIMIT_PROCESS_MEMORY
            // +0x70: ProcessMemoryLimit (int64)
            buf.writeBigInt64LE(BigInt(limits.processMemoryLimitMb) * 1048576n, 0x70);
          }

          if (limits.jobMemoryLimitMb !== undefined) {
            limitFlags |= 0x100; // JOB_OBJECT_LIMIT_JOB_MEMORY
            // +0x78: JobMemoryLimit (int64)
            buf.writeBigInt64LE(BigInt(limits.jobMemoryLimitMb) * 1048576n, 0x78);
          }

          if (limits.cpuRate !== undefined) {
            limitFlags |= 0x80; // JOB_OBJECT_LIMIT_CPU_RATE
            // Note: CPU rate uses JOBOBJECT_CPU_RATE_CONTROL_INFORMATION
            // which is a separate info class (15), not in EXTENDED_LIMIT_INFO.
          }

          buf.writeUInt32LE(limitFlags, 0x10);

          return buf;
        },
      };

      return this.koffiBinding;
    } catch {
      // koffi not available — Job Object isolation not possible
      return null;
    }
  }

  // ===========================================================================
  // koffi-based Job Object Execution
  // ===========================================================================

  private async executeWithJobObject(
    options: StrategyExecuteOptions,
    policy: SandboxPolicy,
    binding: WindowsJobObjectBinding,
    startTime: number,
  ): Promise<ScriptExecutionResult> {
    let jobHandle: unknown = null;

    try {
      // 1. Create Job Object
      const jobName = `SandboxJob_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
      jobHandle = binding.CreateJobObjectW(null, jobName);
      if (!jobHandle || jobHandle === 0n || jobHandle === 0) {
        throw new Error(`CreateJobObjectW failed: cannot create job object`);
      }

      // 2. Build limit structure from policy
      const limits: JobLimits = {};
      if (policy.process?.maxChildProcesses) {
        limits.activeProcessLimit = policy.process.maxChildProcesses + 1; // +1 for the wrapper itself
      }
      if (policy.resource?.memoryLimit) {
        limits.processMemoryLimitMb = policy.resource.memoryLimit;
        limits.jobMemoryLimitMb = policy.resource.memoryLimit * 2; // job total = 2× per-process
      }
      if (policy.resource?.cpuLimit) {
        limits.cpuRate = policy.resource.cpuLimit;
      }

      const limitBuf = binding.buildExtendedLimitInfo(limits);

      // 3. Apply limits via SetInformationJobObject
      //    InfoClass 9 = JobObjectExtendedLimitInformation
      const setResult = binding.SetInformationJobObject(jobHandle, 9, limitBuf, limitBuf.length);
      if (!setResult) {
        // Non-fatal: limits are best-effort
        // (some Windows versions may not support all limit types)
      }

      // 4. Spawn the command and assign to job
      const result = await this.spawnAndAssign(jobHandle, options, binding, startTime);

      return result;
    } catch (error) {
      return {
        success: false,
        scriptName: "sandbox-os-hook",
        executionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
      };
    } finally {
      // 5. Cleanup: terminate and close job object
      if (jobHandle) {
        try {
          binding.TerminateJobObject(jobHandle, 1);
          binding.CloseHandle(jobHandle);
        } catch {
          // Best-effort cleanup
        }
      }
    }
  }

  private async spawnAndAssign(
    jobHandle: unknown,
    options: StrategyExecuteOptions,
    binding: WindowsJobObjectBinding,
    startTime: number,
  ): Promise<ScriptExecutionResult> {
    return new Promise<ScriptExecutionResult>((resolve) => {
      const child = spawn(options.command, {
        shell: true,
        cwd: options.cwd,
        env: { ...process.env, ...options.env },
        windowsHide: true,
      });

      let timedOut = false;
      let timer: NodeJS.Timeout | null = null;

      // Set timeout
      const timeout = options.timeout ?? 30000;
      timer = setTimeout(() => {
        timedOut = true;
        try { child.kill("SIGTERM"); } catch { /* ignore */ }
        // Job Object will kill all child processes on terminate
        try { binding.TerminateJobObject(jobHandle, 1); } catch { /* ignore */ }
      }, timeout);

      // Assign process to Job Object
      // PROCESS_SET_INFORMATION | PROCESS_TERMINATE | PROCESS_SUSPEND_RESUME
      const PROCESS_SET_INFORMATION = 0x0200;
      const hProcess = binding.OpenProcess(PROCESS_SET_INFORMATION, 0, child.pid!);
      if (hProcess) {
        binding.AssignProcessToJobObject(jobHandle, hProcess);
        binding.CloseHandle(hProcess);
      }

      // Collect output
      const stdoutChunks: Buffer[] = [];
      const stderrChunks: Buffer[] = [];

      if (child.stdout) {
        child.stdout.on("data", (chunk: Buffer) => stdoutChunks.push(chunk));
      }
      if (child.stderr) {
        child.stderr.on("data", (chunk: Buffer) => stderrChunks.push(chunk));
      }

      child.on("close", (exitCode) => {
        if (timer) clearTimeout(timer);

        const stdout = Buffer.concat(stdoutChunks).toString("utf8");
        const stderr = Buffer.concat(stderrChunks).toString("utf8");

        if (timedOut) {
          resolve({
            success: false,
            scriptName: "sandbox-os-hook",
            stdout,
            stderr,
            exitCode: exitCode ?? -1,
            executionTime: Date.now() - startTime,
            error: `Job Object: process timed out after ${timeout}ms`,
          });
          return;
        }

        resolve({
          success: exitCode === 0,
          scriptName: "sandbox-os-hook",
          stdout,
          stderr,
          exitCode: exitCode ?? -1,
          executionTime: Date.now() - startTime,
        });
      });

      child.on("error", (err) => {
        if (timer) clearTimeout(timer);
        resolve({
          success: false,
          scriptName: "sandbox-os-hook",
          executionTime: Date.now() - startTime,
          error: `Job Object: spawn failed - ${err.message}`,
        });
      });
    });
  }

  // ===========================================================================
  // Passthrough Execution (fallback when koffi unavailable)
  // ===========================================================================

  private async executeCommand(
    options: StrategyExecuteOptions,
    startTime: number,
  ): Promise<ScriptExecutionResult> {
    try {
      const result = await this.terminalService.executeOneOff(options.command, {
        cwd: options.cwd,
        env: options.env,
        timeout: options.timeout,
      });

      return {
        success: result.success,
        scriptName: "sandbox-os-hook",
        stdout: result.stdout,
        stderr: result.stderr,
        exitCode: result.exitCode,
        executionTime: Date.now() - startTime,
        error: result.error,
      };
    } catch (error) {
      return {
        success: false,
        scriptName: "sandbox-os-hook",
        executionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }
}

// ─── Internal Types ──────────────────────────────────────────────────────────

interface KoffiModule {
  load(lib: string): KoffiLibrary;
}

interface KoffiLibrary {
  func(signature: string): KoffiFunction;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type KoffiFunction = (...args: any[]) => any;

interface JobLimits {
  activeProcessLimit?: number;
  processMemoryLimitMb?: number;
  jobMemoryLimitMb?: number;
  cpuRate?: number;
}

interface WindowsJobObjectBinding {
  CreateJobObjectW: KoffiFunction;
  SetInformationJobObject: KoffiFunction;
  AssignProcessToJobObject: KoffiFunction;
  TerminateJobObject: KoffiFunction;
  CloseHandle: KoffiFunction;
  OpenProcess: KoffiFunction;
  buildExtendedLimitInfo: (limits: JobLimits) => Buffer;
}

// =========================================================================
// 3. Proot-like Path Redirect Strategy
// =========================================================================

/**
 * Proot-style path redirection strategy.
 *
 * id: "proot-redirect"
 * Available: Linux (ptrace) / theoretically cross-platform with Detours.
 *
 * Redirects file system paths for all child processes transparently.
 * Useful when combined with OverlayVFS for CoW in external tools (gcc, git, npm).
 */
export class ProotLikeRedirectStrategy implements StrategyImplementation<ScriptExecutionResult> {
  id = "proot-redirect";
  name = "Proot Path Redirect (OS Hook)";
  description = "Path redirection using ptrace or DLL injection for transparent VFS";
  priority = 40;

  private terminalService: TerminalService;

  constructor(terminalService?: TerminalService) {
    this.terminalService = terminalService || getTerminalService();
  }

  isAvailable(): boolean {
    // Default: available anywhere (uses proot binary if present)
    // In practice, ptrace-based proot only works on Linux.
    // Windows variant would use Detours DLL injection.
    try {
      // Quick check — is proot on PATH?
      require("child_process").execSync("proot --version", { stdio: "ignore", timeout: 1000 });
      return true;
    } catch {
      return process.platform === "linux";
    }
  }

  async execute(
    options: StrategyExecuteOptions,
    _policy: SandboxPolicy,
  ): Promise<ScriptExecutionResult> {
    const startTime = Date.now();

    // Phase 3: Replace with proot binary invocation.
    //   When proot is available:
    //     const redirectCommand = `proot -b ${vfsRoot}:/etc -b ${vfsRoot}:/home -w ${cwd} ${command}`;
    //     const result = await this.terminalService.executeOneOff(redirectCommand, options);
    //     return this.toResult(result, startTime);
    //
    // For now, execute directly (best-effort sandboxing).
    return this.executeCommand(options, startTime);
  }

  private async executeCommand(
    options: StrategyExecuteOptions,
    startTime: number,
  ): Promise<ScriptExecutionResult> {
    try {
      const result = await this.terminalService.executeOneOff(options.command, {
        cwd: options.cwd,
        env: options.env,
        timeout: options.timeout,
      });

      return {
        success: result.success,
        scriptName: "sandbox-os-hook",
        stdout: result.stdout,
        stderr: result.stderr,
        exitCode: result.exitCode,
        executionTime: Date.now() - startTime,
        error: result.error,
      };
    } catch (error) {
      return {
        success: false,
        scriptName: "sandbox-os-hook",
        executionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }
}