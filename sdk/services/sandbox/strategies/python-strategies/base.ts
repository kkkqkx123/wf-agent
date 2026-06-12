/**
 * Python Sandbox Strategies — Shared Base
 *
 * Common utilities and constants shared across Python sandbox strategy implementations.
 */

import { spawnSync } from "node:child_process";

/**
 * Default denied Python modules when policy.python.deniedModules is empty.
 */
export const DEFAULT_DENIED_MODULES: string[] = [
  "os",
  "subprocess",
  "shutil",
  "ctypes",
  "socket",
  "pty",
  "signal",
  "multiprocessing",
  "distutils",
  "sysconfig",
];

/**
 * Check if Python is available on the system by running `python --version`.
 */
export function checkPythonAvailable(): boolean {
  try {
    const result = spawnSync("python", ["--version"], {
      timeout: 5000,
      stdio: "pipe",
      encoding: "utf-8",
    });
    return result.status === 0;
  } catch {
    return false;
  }
}
