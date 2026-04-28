/**
 * Shell Detector
 * 
 * Detects available shells on the current system and provides
 * shell-specific configuration for command execution.
 */

import { existsSync } from "fs";
import { homedir, platform } from "os";
import { env } from "process";
import type { ShellType, ShellInfo } from "./types.js";

/**
 * Shell configuration mapping
 */
const SHELL_CONFIGS: Record<ShellType, Omit<ShellInfo, "available">> = {
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
    commandFlag: "-e",
  },
};

/**
 * Shell Detector
 * 
 * Provides shell detection and configuration for the terminal service.
 */
export class ShellDetector {
  private cachedAvailableShells: Map<ShellType, boolean> = new Map();

  /**
   * Get the default shell for the current platform
   */
  getDefaultShell(): ShellType {
    const currentPlatform = platform();

    if (currentPlatform === "win32") {
      // On Windows, prefer PowerShell
      return "powershell";
    }

    // On Unix-like systems, prefer bash
    // Check SHELL environment variable for user preference
    const shellEnv = env["SHELL"];
    if (shellEnv) {
      if (shellEnv.includes("zsh")) return "zsh";
      if (shellEnv.includes("fish")) return "fish";
      if (shellEnv.includes("bash")) return "bash";
    }

    return "bash";
  }

  /**
   * Check if a shell is available on the system
   */
  async isShellAvailable(shellType: ShellType): Promise<boolean> {
    // Check cache first
    if (this.cachedAvailableShells.has(shellType)) {
      return this.cachedAvailableShells.get(shellType)!;
    }

    const config = SHELL_CONFIGS[shellType];
    const available = this.checkShellAvailability(config.path, shellType);

    // Cache the result
    this.cachedAvailableShells.set(shellType, available);

    return available;
  }

  /**
   * Get the executable path for a shell type
   */
  getShellPath(shellType: ShellType): string {
    const config = SHELL_CONFIGS[shellType];

    // Handle special cases
    if (shellType === "git-bash") {
      // Try common Git Bash installation paths
      const gitBashPaths = [
        "C:\\Program Files\\Git\\bin\\bash.exe",
        "C:\\Program Files (x86)\\Git\\bin\\bash.exe",
        `${homedir()}\\AppData\\Local\\Programs\\Git\\bin\\bash.exe`,
      ];

      for (const path of gitBashPaths) {
        if (existsSync(path)) {
          return path;
        }
      }
    }

    return config.path;
  }

  /**
   * Get shell arguments for executing a command
   */
  getShellArgs(shellType: ShellType, command: string): string[] {
    const config = SHELL_CONFIGS[shellType];
    return [config.commandFlag, command];
  }

  /**
   * Get the command flag for a shell type
   */
  getCommandFlag(shellType: ShellType): string {
    return SHELL_CONFIGS[shellType].commandFlag;
  }

  /**
   * Get all available shells on the system
   */
  async getAvailableShells(): Promise<ShellType[]> {
    const available: ShellType[] = [];

    for (const shellType of Object.keys(SHELL_CONFIGS) as ShellType[]) {
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
    const config = SHELL_CONFIGS[shellType];
    const available = await this.isShellAvailable(shellType);
    const path = this.getShellPath(shellType);

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

    // Fallback to default shell
    return this.getDefaultShell();
  }

  /**
   * Check shell availability based on platform
   */
  private checkShellAvailability(path: string, shellType: ShellType): boolean {
    const currentPlatform = platform();

    // Windows-specific shells
    if (["cmd", "powershell", "pwsh", "git-bash", "wsl"].includes(shellType)) {
      if (currentPlatform !== "win32") {
        // WSL can be checked on non-Windows, but others are Windows-only
        if (shellType !== "wsl") {
          return false;
        }
      }
    }

    // Unix-specific shells
    if (["bash", "zsh", "fish", "sh"].includes(shellType)) {
      if (currentPlatform === "win32") {
        // On Windows, these might be available via Git Bash or WSL
        // For simplicity, we check if the path exists
        if (!existsSync(path)) {
          // Check Git Bash path for bash
          if (shellType === "bash") {
            const gitBashPaths = [
              "C:\\Program Files\\Git\\bin\\bash.exe",
              "C:\\Program Files (x86)\\Git\\bin\\bash.exe",
              `${homedir()}\\AppData\\Local\\Programs\\Git\\bin\\bash.exe`,
            ];
            return gitBashPaths.some((p) => existsSync(p));
          }
          return false;
        }
      }
    }

    // For built-in Windows commands, they're always available
    if (shellType === "cmd" && currentPlatform === "win32") {
      return true;
    }

    if (shellType === "powershell" && currentPlatform === "win32") {
      return true;
    }

    // For other shells, check if the path exists
    if (currentPlatform === "win32") {
      // On Windows, check if the executable exists
      if (shellType === "pwsh") {
        // PowerShell Core might be installed
        return existsSync(path) || existsSync("pwsh.exe");
      }
      if (shellType === "git-bash") {
        const gitBashPaths = [
          "C:\\Program Files\\Git\\bin\\bash.exe",
          "C:\\Program Files (x86)\\Git\\bin\\bash.exe",
          `${homedir()}\\AppData\\Local\\Programs\\Git\\bin\\bash.exe`,
        ];
        return gitBashPaths.some((p) => existsSync(p));
      }
      if (shellType === "wsl") {
        // WSL is typically available if wsl.exe exists
        return true; // Assume WSL is available on modern Windows
      }
      return existsSync(path);
    }

    // On Unix-like systems, check if the path exists
    return existsSync(path);
  }

  /**
   * Clear the availability cache
   */
  clearCache(): void {
    this.cachedAvailableShells.clear();
  }
}

/**
 * Default shell detector instance
 */
export const shellDetector = new ShellDetector();
