import { describe, it, expect } from "vitest";
import { ColorRole, createTheme, isLightTerminal } from "../theme.js";

describe("ColorRole", () => {
  it("should have expected color roles", () => {
    expect(ColorRole.Default).toBe("default");
    expect(ColorRole.Muted).toBe("muted");
    expect(ColorRole.Brand).toBe("brand");
    expect(ColorRole.Add).toBe("add");
    expect(ColorRole.Remove).toBe("remove");
    expect(ColorRole.Warning).toBe("warning");
    expect(ColorRole.Error).toBe("error");
    expect(ColorRole.Highlight).toBe("highlight");
  });
});

describe("createTheme", () => {
  it("should create a dark theme by default", () => {
    const theme = createTheme(false);
    expect(theme.isLight).toBe(false);
    expect(theme.reset).toBe("\x1b[0m");
  });

  it("should create a light theme", () => {
    const theme = createTheme(true);
    expect(theme.isLight).toBe(true);
    expect(theme.fg("hello")).toContain("\x1b[30m");
  });

  it("should wrap text with ANSI codes", () => {
    const dark = createTheme(false);
    expect(dark.fg("test")).toBe("\x1b[37mtest\x1b[0m");
    expect(dark.bold("test")).toBe("\x1b[1mtest\x1b[0m");
    expect(dark.dim("test")).toBe("\x1b[2mtest\x1b[0m");
  });

  it("should apply color roles", () => {
    const dark = createTheme(false);
    expect(dark.muted("x")).toContain("\x1b[90m");
    expect(dark.brand("x")).toContain("\x1b[94m");
    expect(dark.add("x")).toContain("\x1b[32m");
    expect(dark.remove("x")).toContain("\x1b[31m");
    expect(dark.warning("x")).toContain("\x1b[33m");
    expect(dark.error("x")).toContain("\x1b[91m");
    expect(dark.highlight("x")).toContain("\x1b[93m");
  });

  it("should have different color codes for light theme", () => {
    const light = createTheme(true);
    const dark = createTheme(false);
    expect(light.fg("")).not.toBe(dark.fg(""));
    expect(light.fg("")).toContain("\x1b[30m");
    expect(dark.fg("")).toContain("\x1b[37m");
  });

  it("should use dark theme config", () => {
    const dark = createTheme(false);
    expect(dark.config.foreground).toBe("\x1b[37m");
    expect(dark.config.background).toBe("\x1b[40m");
  });

  it("should use light theme config", () => {
    const light = createTheme(true);
    expect(light.config.foreground).toBe("\x1b[30m");
    expect(light.config.background).toBe("\x1b[107m");
  });
});
