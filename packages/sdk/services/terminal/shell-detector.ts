/**
 * Shell Detector
 *
 * Detects available shells on the current system and provides
 * shell-specific configuration for command execution.
 *
 * Path resolution priority (highest to lowest):
 *   1. Custom path override (from config / constructor)
 *   2. Default hardcoded path (from SHELL_CONFIGS)
 *   3. `where`/`which` lookup in system PATH
 *   4. Executable name only (relies on OS PATH resolution)
 */

import { existsSync } from "fs";
import { homedir, platform } from "os";
import { env } from "process";
import { execSync } from "child_process";
import type { ShellType, ShellInfo, ShellPathOverrides } from "./types.js";

/**
 * Shell configuration mapping (default hardcoded paths)
 */
const DEFAULT_SHELL_CONFIGS: Record<ShellType, Omit<ShellInfo, "available">> = {
  bash: {
    type: "bash",
    path: "/bin/bash",
    commandFlag: "-c",
  },
  zsh: {
    type: "zsh",
    path: "/bin/zsh",
    commandFlag: "-c",
  },
  fish: {
    type: "fish",
    path: "/bin/fish",
    commandFlag: "-c",
  },
  sh: {
    type: "sh",
    path: "/bin/sh",
    commandFlag: "-c",
  },
  cmd: {
    type: "cmd",
    path: "cmd.exe",
    commandFlag: "/c",
  },
  powershell: {
    type: "powershell",
    path: "powershell.exe",
    commandFlag: "-Command",
  },
  pwsh: {
    type: "pwsh",
    path: "pwsh.exe",
    commandFlag: "-Command",
  },
  "git-bash": {
    type: "git-bash",
    path: "C:\\Program Files\\Git\\bin\\bash.exe",
    commandFlag: "-c",
  },
  wsl: {
    type: "wsl",
    path: "wsl.exe",
    commandFlag: "--",
  },
};

/** Shell types that are built into Windows */
const WINDOWS_BUILTIN_SHELLS: readonly ShellType[] = ["cmd", "powershell"];

/** Shell types that are Windows-only */
const WINDOWS_ONLY_SHELLS: readonly ShellType[] = ["cmd", "powershell", "pwsh", "git-bash", "wsl"];

/**
 * Shell Detector
 *
 * Provides shell detection and configuration for the terminal service.
 */
export class ShellDetector {
  private cachedAvailableShells: Map<ShellType, boolean> = new Map();
  private readonly pathOverrides: ShellPathOverrides;
  private readonly currentPlatform: NodeJS.Platform;

  constructor(pathOverrides?: ShellPathOverrides) {
    this.pathOverrides = pathOverrides ?? {};
    this.currentPlatform = platform();
  }

  /**
   * Get the default shell for the current platform
   */
  getDefaultShell(): ShellType {
    if (this.currentPlatform === "win32") {
      return "powershell";
    }

    const shellEnv = env["SHELL"];
    if (shellEnv) {
      if (shellEnv.includes("zsh")) return "zsh";
      if (shellEnv.includes("fish")) return "fish";
      if (shellEnv.includes("bash")) return "bash";
    }

    return "bash";
  }

  /**
   * Resolve the executable path for a shell type.
   *
   * Priority:
   *   1. Custom override (from ShellPathOverrides)
   *   2. Default hardcoded path (from DEFAULT_SHELL_CONFIGS)
   *   3. Platform-specific heuristics (multi-path search)
   *   4. `where`/`which` lookup
   *   5. Executable name only (OS PATH fallback)
   */
  resolveShellPath(shellType: ShellType): string {
    // 1. Custom override
    const override = this.pathOverrides[shellType];
    if (override) {
      return override;
    }

    const config = DEFAULT_SHELL_CONFIGS[shellType];

    // 2. Platform-specific heuristic paths
    const heuristic = this.resolveHeuristicPath(shellType, config);
    if (heuristic) {
      return heuristic;
    }

    // 3. Default config path
    if (existsSync(config.path)) {
      return config.path;
    }

    // 4. where/which lookup
    const whichPath = this.lookupInPath(shellType);
    if (whichPath) {
      return whichPath;
    }

    // 5. Executable name only (OS PATH fallback)
    return config.path.split("/").pop()?.split("\\").pop() ?? config.path;
  }

  /**
   * Check if a shell is available on the system
   */
  async isShellAvailable(shellType: ShellType): Promise<boolean> {
    if (this.cachedAvailableShells.has(shellType)) {
      return this.cachedAvailableShells.get(shellType)!;
    }

    const available = this.checkShellAvailability(shellType);

    this.cachedAvailableShells.set(shellType, available);
    return available;
  }

  /**
   * Get the highest-priority executable path for a shell type
   * (delegates to resolveShellPath)
   */
  getShellPath(shellType: ShellType): string {
    return this.resolveShellPath(shellType);
  }

  /**
   * Get shell arguments for executing a command
   */
  getShellArgs(shellType: ShellType, command: string): string[] {
    const config = this.getEffectiveConfig(shellType);
    return [config.commandFlag, command];
  }

  /**
   * Get the command flag for a shell type
   */
  getCommandFlag(shellType: ShellType): string {
    return this.getEffectiveConfig(shellType).commandFlag;
  }

  /**
   * Get all available shells on the system
   */
  async getAvailableShells(): Promise<ShellType[]> {
    const available: ShellType[] = [];

    for (const shellType of Object.keys(DEFAULT_SHELL_CONFIGS) as ShellType[]) {
      if (await this.isShellAvailable(shellType)) {
        available.push(shellType);
      }
    }

    return available;
  }

  /**
   * Get shell information for a shell type
   */
  async getShellInfo(shellType: ShellType): Promise<ShellInfo> {
    const config = this.getEffectiveConfig(shellType);
    const available = await this.isShellAvailable(shellType);
    const path = this.resolveShellPath(shellType);

    return {
      type: shellType,
      path,
      available,
      commandFlag: config.commandFlag,
    };
  }

  /**
   * Resolve shell type with fallback
   *
   * If the requested shell is not available, returns the default shell.
   */
  async resolveShellType(shellType?: ShellType): Promise<ShellType> {
    if (!shellType) {
      return this.getDefaultShell();
    }

    const available = await this.isShellAvailable(shellType);
    if (available) {
      return shellType;
    }

    return this.getDefaultShell();
  }

  /**
   * Clear the availability cache
   */
  clearCache(): void {
    this.cachedAvailableShells.clear();
  }

  // ==========================================================================
  // Private Helpers
  // ==========================================================================

  /**
   * Get the effective config for a shell type (merged with overrides)
   */
  private getEffectiveConfig(shellType: ShellType): Omit<ShellInfo, "available"> {
    return DEFAULT_SHELL_CONFIGS[shellType];
  }

  /**
   * Check shell availability based on platform and path existence
   */
  private checkShellAvailability(shellType: ShellType): boolean {
    // Platform compatibility gate
    if (!this.isPlatformCompatible(shellType)) {
      return false;
    }

    // Windows built-in shells are always available
    if (WINDOWS_BUILTIN_SHELLS.includes(shellType) && this.currentPlatform === "win32") {
      return true;
    }

    // Try override first
    const override = this.pathOverrides[shellType];
    if (override && existsSync(override)) {
      return true;
    }

    // Try heuristic paths
    const config = DEFAULT_SHELL_CONFIGS[shellType];
    const heuristic = this.resolveHeuristicPath(shellType, config);
    if (heuristic) {
      return true;
    }

    // Try default path
    if (existsSync(config.path)) {
      return true;
    }

    // Try where/which
    const whichPath = this.lookupInPath(shellType);
    if (whichPath) {
      return true;
    }

    // WSL special case: assume available on modern Windows
    if (shellType === "wsl" && this.currentPlatform === "win32") {
      return true;
    }

    return false;
  }

  /**
   * Platform compatibility check
   */
  private isPlatformCompatible(shellType: ShellType): boolean {
    if (this.currentPlatform === "win32") {
      return true; // Windows can access Windows-only + Unix shells via WSL/Git Bash
    }
    if (WINDOWS_ONLY_SHELLS.includes(shellType)) {
      return shellType === "wsl"; // Only WSL is checkable on non-Windows
    }
    return true; // Unix shells on Unix
  }

  /**
   * Platform-specific heuristic path resolution
   *
   * Handles cases like Git Bash on Windows, pwsh cross-platform, etc.
   */
  private resolveHeuristicPath(
    shellType: ShellType,
    config: Omit<ShellInfo, "available">,
  ): string | null {
    // Git Bash on Windows — try common installation paths
    if (shellType === "git-bash") {
      const gitBashPaths = [
        "C:\\Program Files\\Git\\bin\\bash.exe",
        "C:\\Program Files (x86)\\Git\\bin\\bash.exe",
        `${homedir()}\\AppData\\Local\\Programs\\Git\\bin\\bash.exe`,
      ];
      for (const p of gitBashPaths) {
        if (existsSync(p)) return p;
      }
      return null;
    }

    // bash on Windows — check Git Bash paths
    if (shellType === "bash" && this.currentPlatform === "win32") {
      const gitBashPaths = [
        "C:\\Program Files\\Git\\bin\\bash.exe",
        "C:\\Program Files (x86)\\Git\\bin\\bash.exe",
        `${homedir()}\\AppData\\Local\\Programs\\Git\\bin\\bash.exe`,
      ];
      for (const p of gitBashPaths) {
        if (existsSync(p)) return p;
      }
    }

    // pwsh (PowerShell Core) — cross-platform
    if (shellType === "pwsh") {
      if (existsSync(config.path)) return config.path;
      if (existsSync("pwsh.exe")) return "pwsh.exe";
      const pwshUnix = "/usr/bin/pwsh";
      if (existsSync(pwshUnix)) return pwshUnix;
      const pwshLinux = "/usr/local/bin/pwsh";
      if (existsSync(pwshLinux)) return pwshLinux;
    }

    return null;
  }

  /**
   * Lookup shell executable in system PATH using `where` (Windows) or `which` (Unix)
   */
  private lookupInPath(shellType: ShellType): string | null {
    const config = DEFAULT_SHELL_CONFIGS[shellType];
    // Extract just the executable name from the config path
    const execName = config.path.split("/").pop()?.split("\\").pop() ?? config.path;

    try {
      const cmd = this.currentPlatform === "win32" ? `where ${execName}` : `which ${execName}`;
      const result = execSync(cmd, { encoding: "utf-8", timeout: 3000 });
      const firstMatch = result.split("\n")[0]?.trim();
      return firstMatch && firstMatch.length > 0 ? firstMatch : null;
    } catch {
      return null;
    }
  }
}

/**
 * Default shell detector instance (no path overrides)
 */
export const shellDetector = new ShellDetector();
