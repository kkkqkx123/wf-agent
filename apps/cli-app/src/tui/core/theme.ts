const RESET = "\x1b[0m";
const BOLD = "\x1b[1m";
const DIM = "\x1b[2m";

export enum ColorRole {
  Default = "default",
  Muted = "muted",
  Brand = "brand",
  Add = "add",
  Remove = "remove",
  Warning = "warning",
  Error = "error",
  Highlight = "highlight",
}

export interface ThemeConfig {
  foreground: string;
  background: string;
  roles: Record<ColorRole, string>;
}

export interface Theme {
  config: ThemeConfig;
  isLight: boolean;
  fg: (text: string) => string;
  bg: (text: string) => string;
  muted: (text: string) => string;
  brand: (text: string) => string;
  add: (text: string) => string;
  remove: (text: string) => string;
  warning: (text: string) => string;
  error: (text: string) => string;
  highlight: (text: string) => string;
  bold: (text: string) => string;
  dim: (text: string) => string;
  reset: string;
}

function wrap(code: string): (text: string) => string {
  return (text: string) => `${code}${text}${RESET}`;
}

const DARK_THEME: ThemeConfig = {
  foreground: "\x1b[37m",
  background: "\x1b[40m",
  roles: {
    [ColorRole.Default]: "\x1b[37m",
    [ColorRole.Muted]: "\x1b[90m",
    [ColorRole.Brand]: "\x1b[94m",
    [ColorRole.Add]: "\x1b[32m",
    [ColorRole.Remove]: "\x1b[31m",
    [ColorRole.Warning]: "\x1b[33m",
    [ColorRole.Error]: "\x1b[91m",
    [ColorRole.Highlight]: "\x1b[93m",
  },
};

const LIGHT_THEME: ThemeConfig = {
  foreground: "\x1b[30m",
  background: "\x1b[107m",
  roles: {
    [ColorRole.Default]: "\x1b[30m",
    [ColorRole.Muted]: "\x1b[90m",
    [ColorRole.Brand]: "\x1b[34m",
    [ColorRole.Add]: "\x1b[32m",
    [ColorRole.Remove]: "\x1b[31m",
    [ColorRole.Warning]: "\x1b[33m",
    [ColorRole.Error]: "\x1b[91m",
    [ColorRole.Highlight]: "\x1b[93m",
  },
};

export function isLightTerminal(): boolean {
  const colorfgbg = process.env["COLORFGBG"];
  if (colorfgbg) {
    const parts = colorfgbg.split(";");
    const bg = parts[parts.length - 1];
    if (bg === "15" || bg === "7") return true;
    if (bg === "0" || bg === "8") return false;
  }
  const term = process.env["TERM"] ?? "";
  if (term.includes("light") || term.includes("-l")) return true;
  return false;
}

export function createTheme(isLight?: boolean): Theme {
  if (isLight === undefined) {
    isLight = isLightTerminal();
  }
  const config = isLight ? LIGHT_THEME : DARK_THEME;
  const roles = config.roles;

  return {
    config,
    isLight,
    fg: wrap(config.foreground),
    bg: wrap(config.background),
    muted: wrap(roles[ColorRole.Muted]),
    brand: wrap(roles[ColorRole.Brand]),
    add: wrap(roles[ColorRole.Add]),
    remove: wrap(roles[ColorRole.Remove]),
    warning: wrap(roles[ColorRole.Warning]),
    error: wrap(roles[ColorRole.Error]),
    highlight: wrap(roles[ColorRole.Highlight]),
    bold: (text: string) => `${BOLD}${text}${RESET}`,
    dim: (text: string) => `${DIM}${text}${RESET}`,
    reset: RESET,
  };
}
