import { describe, expect, it } from "vitest";
import { formatVersion, parseSettingsView, SETTINGS_HEIGHT, windowHeight } from "./settings";

describe("parseSettingsView", () => {
  it("accepts a complete payload", () => {
    expect(parseSettingsView({ always_on_top: true, autostart: false, version: "0.2.0", window_width: 260, window_height: 150 })).toEqual({
      always_on_top: true,
      autostart: false,
      version: "0.2.0",
      window_width: 260,
      window_height: 150,
    });
  });

  it("ignores unknown fields", () => {
    expect(parseSettingsView({ always_on_top: false, autostart: true, version: "1", window_width: 260, window_height: 150, extra: 1 })).toEqual({
      always_on_top: false,
      autostart: true,
      version: "1",
      window_width: 260,
      window_height: 150,
    });
  });

  it("rejects anything that is not the expected shape", () => {
    expect(parseSettingsView(null)).toBeNull();
    expect(parseSettingsView(undefined)).toBeNull();
    expect(parseSettingsView("x")).toBeNull();
    expect(parseSettingsView({})).toBeNull();
    expect(parseSettingsView({ always_on_top: 1, autostart: false, version: "1" })).toBeNull();
    expect(parseSettingsView({ always_on_top: true, autostart: "no", version: "1" })).toBeNull();
    expect(parseSettingsView({ always_on_top: true, autostart: false })).toBeNull();
    const ok = { always_on_top: true, autostart: false, version: "1" };
    expect(parseSettingsView(ok)).toBeNull();
    expect(parseSettingsView({ ...ok, window_width: 260, window_height: 0 })).toBeNull();
    expect(parseSettingsView({ ...ok, window_width: "260", window_height: 150 })).toBeNull();
    expect(parseSettingsView({ ...ok, window_width: 260, window_height: Number.NaN })).toBeNull();
  });
});

describe("formatVersion", () => {
  it("prefixes a v", () => {
    expect(formatVersion("0.2.0")).toBe("v0.2.0");
  });

  it("does not double the prefix and trims", () => {
    expect(formatVersion(" v0.2.0 ")).toBe("v0.2.0");
  });

  it("is empty when there is no version", () => {
    expect(formatVersion("")).toBe("");
    expect(formatVersion("  ")).toBe("");
  });
});

describe("windowHeight", () => {
  it("uses the starting height for the usage view", () => {
    expect(windowHeight("usage", 150)).toBe(150);
    expect(windowHeight("usage", 143)).toBe(143);
  });

  it("uses the settings height for the settings view", () => {
    expect(windowHeight("settings", 150)).toBe(SETTINGS_HEIGHT);
  });

  it("never shrinks below the starting height", () => {
    expect(windowHeight("settings", 300)).toBe(300);
  });
});
