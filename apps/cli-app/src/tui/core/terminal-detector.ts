export interface TerminalCapabilities {
  readonly isWindowsConhost: boolean;
  readonly supportsKittyProtocol: boolean;
  readonly supportsAltScreen: boolean;
  readonly supportsBracketedPaste: boolean;
  readonly supportsSynchronizedOutput: boolean;
}

export function detectCapabilities(): TerminalCapabilities {
  const isWindows = process.platform === "win32";
  const term = process.env["TERM"] ?? "";
  const isKnownModern = term.includes("xterm") || term.includes("kitty") || term.includes("alacritty") || term.includes("wezterm");
  const conhostEnv = process.env["WT_SESSION"] || process.env["WT_PROFILE_ID"];
  return {
    isWindowsConhost: isWindows && !conhostEnv,
    supportsKittyProtocol: !isWindows && isKnownModern,
    supportsAltScreen: !(isWindows && !conhostEnv),
    supportsBracketedPaste: true,
    supportsSynchronizedOutput: !(isWindows && !conhostEnv),
  };
}
