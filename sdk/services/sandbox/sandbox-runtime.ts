/**
 * Sandbox Runtime
 *
 * Central orchestrator that coordinates strategy execution and VFS lifecycle.
 * Architecture reference: docs/infra/sandbox/architecture.md §5
 */

import type {
  SandboxConfig,
  SandboxGlobalConfig,
  SandboxPolicy,
  SandboxMode,
  ScriptExecutionResult,
  StrategyExecuteOptions,
  StrategyImplementation,
  ScriptLanguage,
  VFSConfig,
} from "@wf-agent/types";

import { DefaultStrategyResolver } from "./strategy-resolver.js";
import { DEFAULT_SANDBOX_POLICY } from "./default-policy.js";

import { OverlayVFS } from "../vfs/overlay-vfs.js";
import { CheckpointAwareVFS } from "../vfs/checkpoint-vfs.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "SandboxRuntime" });

export interface SandboxRuntimeResult {
  strategy: StrategyImplementation<ScriptExecutionResult> | null;
  vfs: OverlayVFS | null;
  policy: SandboxPolicy;
}

let instance: SandboxRuntime | null = null;

export function getSandboxRuntime(globalConfig?: SandboxGlobalConfig): SandboxRuntime {
  if (!instance) {
    instance = new SandboxRuntime(globalConfig);
  }
  return instance;
}

export function resetSandboxRuntime(): void {
  instance = null;
}

export class SandboxRuntime {
  private resolver: DefaultStrategyResolver;
  private globalConfig: SandboxGlobalConfig | null;
  private vfsInstances = new Map<string, OverlayVFS>();
  private checkpointVfsInstances = new Map<string, CheckpointAwareVFS>();

  constructor(globalConfig?: SandboxGlobalConfig) {
    this.resolver = new DefaultStrategyResolver();
    this.globalConfig = globalConfig ?? null;
  }

  isEnabled(config?: SandboxConfig): boolean {
    if (!config) return false;
    return config.mode !== undefined && config.mode !== "disabled";
  }

  async createRuntime(
    language: ScriptLanguage,
    options: StrategyExecuteOptions,
    config?: SandboxConfig,
  ): Promise<SandboxRuntimeResult> {

    if (!this.isEnabled(config)) {
      return { strategy: null, vfs: null, policy: DEFAULT_SANDBOX_POLICY };
    }

    const resolvedConfig = this.resolveConfig(config);
    const mode = resolvedConfig.mode ?? "strict";

    const mergedPolicy = this.mergePolicy(resolvedConfig);
    const strategy = this.resolveStrategy(language, resolvedConfig);
    let vfs: OverlayVFS | null = null;

    const vfsConfig = this.resolveVFSConfig(resolvedConfig, language);
    if (vfsConfig?.enabled) {
      const workspaceRoot = vfsConfig.workspaceRoot || options.cwd || process.cwd();
      const overlayVfs = new OverlayVFS({
        enabled: true,
        storage: vfsConfig.storage ?? "memory",
        workspaceRoot,
        pathPolicy: vfsConfig.pathPolicy,
      });
      this.vfsInstances.set(workspaceRoot, overlayVfs);
      vfs = overlayVfs;

      logger.debug("VFS initialized for sandbox execution", {
        workspaceRoot,
        storage: vfsConfig.storage,
      });
    }

    return {
      strategy: mode === "lenient" ? this.createLenientWrapper(strategy) : strategy,
      vfs,
      policy: mergedPolicy,
    };
  }

  createCheckpointAwareVFS(workspaceRoot: string): CheckpointAwareVFS | null {
    const vfs = this.vfsInstances.get(workspaceRoot);
    if (!vfs) return null;

    let cpVfs = this.checkpointVfsInstances.get(workspaceRoot);
    if (!cpVfs) {
      cpVfs = new CheckpointAwareVFS(vfs);
      this.checkpointVfsInstances.set(workspaceRoot, cpVfs);
    }
    return cpVfs;
  }

  registerStrategy(
    language: "shell" | "python" | "javascript",
    impl: StrategyImplementation<unknown>,
  ): void {
    this.resolver.registerStrategy(language, impl);
  }

  private resolveConfig(config?: SandboxConfig): SandboxConfig {
    if (!config) return { mode: "disabled" };

    let resolved: SandboxConfig = { ...config };

    if (this.globalConfig?.defaultProfile) {
      const profile = this.globalConfig.profiles?.find(
        (p: { name: string }) => p.name === this.globalConfig!.defaultProfile,
      );
      if (profile) {
        resolved = this.mergeConfig(profile, resolved);
      }
    }

    if (config.profile && this.globalConfig?.profiles) {
      const profile = this.globalConfig.profiles.find((p: { name: string }) => p.name === config.profile);
      if (profile) {
        resolved = this.mergeConfig(profile, resolved);
      }
    }

    resolved = this.applyLegacyMappings(resolved);

    return resolved;
  }

  private mergeConfig(base: { mode?: SandboxMode; policy?: Partial<SandboxPolicy>; shellStrategy?: string[]; pythonStrategy?: string[]; javascriptStrategy?: string[]; vfs?: VFSConfig }, override: SandboxConfig): SandboxConfig {
    return {
      mode: override.mode ?? base.mode ?? "disabled",
      policy: { ...base.policy, ...override.policy },
      shellStrategy: override.shellStrategy ?? base.shellStrategy,
      pythonStrategy: override.pythonStrategy ?? base.pythonStrategy,
      javascriptStrategy: override.javascriptStrategy ?? base.javascriptStrategy,
      vfs: override.vfs ?? base.vfs,
    };
  }

  private applyLegacyMappings(config: SandboxConfig): SandboxConfig {
    if (config.type === "docker") {
      return { ...config, shellStrategy: ["container"] };
    }
    if (config.type === "nodejs") {
      return { ...config, javascriptStrategy: ["vm-context"] };
    }
    if (config.type === "python") {
      return { ...config, pythonStrategy: ["ast-analyzer", "builtin-hook"] };
    }
    return config;
  }

  private mergePolicy(config: SandboxConfig): SandboxPolicy {
    const base = { ...DEFAULT_SANDBOX_POLICY, mode: config.mode ?? "strict" };
    if (!config.policy) return base;

    return {
      ...base,
      mode: config.mode ?? base.mode,
      shell: { ...base.shell, ...config.policy.shell },
      python: { ...base.python, ...config.policy.python },
      javascript: { ...base.javascript, ...config.policy.javascript },
      filesystem: { ...base.filesystem, ...config.policy.filesystem },
      process: { ...base.process, ...config.policy.process },
      network: { ...base.network, ...config.policy.network },
      resource: { ...base.resource, ...config.policy.resource },
    };
  }

  private resolveStrategy(
    language: ScriptLanguage,
    config: SandboxConfig,
  ): StrategyImplementation<ScriptExecutionResult> {
    let preferredIds: string[];

    switch (language) {
      case "shell":
        preferredIds = config.shellStrategy ?? ["static-analyzer"];
        return this.resolver.resolveBest("shell", preferredIds) as StrategyImplementation<ScriptExecutionResult>;
      case "python":
        preferredIds = config.pythonStrategy ?? ["ast-analyzer", "builtin-hook"];
        return this.resolver.resolveBest("python", preferredIds) as StrategyImplementation<ScriptExecutionResult>;
      case "javascript":
        preferredIds = config.javascriptStrategy ?? ["vm-context"];
        return this.resolver.resolveBest("javascript", preferredIds) as StrategyImplementation<ScriptExecutionResult>;
      default:
        return this.resolver.resolveBest("shell", ["static-analyzer"]) as StrategyImplementation<ScriptExecutionResult>;
    }
  }

  private resolveVFSConfig(config: SandboxConfig, _language: ScriptLanguage): VFSConfig | undefined {
    return config.vfs;
  }

  private createLenientWrapper(
    inner: StrategyImplementation<ScriptExecutionResult>,
  ): StrategyImplementation<ScriptExecutionResult> {
    const wrapper: StrategyImplementation<ScriptExecutionResult> = {
      id: inner.id,
      name: inner.name,
      description: `${inner.description} (lenient mode)`,
      priority: inner.priority,
      isAvailable: () => inner.isAvailable(),
      execute: async (options: StrategyExecuteOptions, policy: SandboxPolicy) => {
        const result = await inner.execute(options, policy);
        if (!result.success && result.error?.startsWith("Sandbox denied")) {
          logger.warn("Sandbox violation (lenient mode, allowed)", {
            error: result.error,
            command: options.command,
          });
          return {
            success: true,
            scriptName: result.scriptName,
            stdout: result.stderr ?? "",
            stderr: `[sandbox-lenient-warning] ${result.error}`,
            exitCode: 0,
            executionTime: result.executionTime,
          };
        }
        return result;
      },
    };
    return wrapper;
  }
}