import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { detectCapabilities } from "../terminal-detector.js";

function withEnv(env: Record<string, string | undefined>, platform: string, fn: () => void): void {
  const origEnv: Record<string, string | undefined> = {};
  const origPlatform = process.platform;

  for (const key of Object.keys(env)) {
    origEnv[key] = process.env[key];
    if (env[key] === undefined) {
      delete process.env[key];
    } else {
      process.env[key] = env[key];
    }
  }
  Object.defineProperty(process, "platform", { value: platform, configurable: true });

  try {
    fn();
  } finally {
    for (const key of Object.keys(env)) {
      if (origEnv[key] === undefined) {
        delete process.env[key];
      } else {
        process.env[key] = origEnv[key];
      }
    }
    Object.defineProperty(process, "platform", { value: origPlatform, configurable: true });
  }
}

describe("detectCapabilities", () => {
  it("should detect modern terminal on non-Windows", () => {
    withEnv({ TERM: "xterm-256color", WT_SESSION: undefined, WT_PROFILE_ID: undefined }, "linux", () => {
      const caps = detectCapabilities();
      expect(caps.isWindowsConhost).toBe(false);
      expect(caps.supportsKittyProtocol).toBe(true);
      expect(caps.supportsAltScreen).toBe(true);
      expect(caps.supportsBracketedPaste).toBe(true);
      expect(caps.supportsSynchronizedOutput).toBe(true);
    });
  });

  it("should detect legacy Windows conhost", () => {
    withEnv({ TERM: "xterm", WT_SESSION: undefined, WT_PROFILE_ID: undefined }, "win32", () => {
      const caps = detectCapabilities();
      expect(caps.isWindowsConhost).toBe(true);
      expect(caps.supportsKittyProtocol).toBe(false);
      expect(caps.supportsAltScreen).toBe(false);
      expect(caps.supportsSynchronizedOutput).toBe(false);
    });
  });

  it("should detect Windows Terminal via WT_SESSION", () => {
    withEnv({ WT_SESSION: "abc123", WT_PROFILE_ID: undefined }, "win32", () => {
      const caps = detectCapabilities();
      expect(caps.isWindowsConhost).toBe(false);
      expect(caps.supportsAltScreen).toBe(true);
      expect(caps.supportsSynchronizedOutput).toBe(true);
    });
  });

  it("should detect Windows Terminal via WT_PROFILE_ID", () => {
    withEnv({ WT_SESSION: undefined, WT_PROFILE_ID: "prof1" }, "win32", () => {
      const caps = detectCapabilities();
      expect(caps.isWindowsConhost).toBe(false);
    });
  });

  it("should support kitty protocol on known terminals", () => {
    withEnv({ TERM: "xterm-256color" }, "linux", () => {
      expect(detectCapabilities().supportsKittyProtocol).toBe(true);
    });
    withEnv({ TERM: "kitty" }, "linux", () => {
      expect(detectCapabilities().supportsKittyProtocol).toBe(true);
    });
    withEnv({ TERM: "alacritty" }, "linux", () => {
      expect(detectCapabilities().supportsKittyProtocol).toBe(true);
    });
    withEnv({ TERM: "wezterm" }, "linux", () => {
      expect(detectCapabilities().supportsKittyProtocol).toBe(true);
    });
  });

  it("should not support kitty protocol on unknown terminals", () => {
    withEnv({ TERM: "vt100" }, "linux", () => {
      expect(detectCapabilities().supportsKittyProtocol).toBe(false);
    });
  });

  it("should always support bracketed paste", () => {
    withEnv({}, "linux", () => {
      expect(detectCapabilities().supportsBracketedPaste).toBe(true);
    });
    withEnv({}, "win32", () => {
      expect(detectCapabilities().supportsBracketedPaste).toBe(true);
    });
  });
});
