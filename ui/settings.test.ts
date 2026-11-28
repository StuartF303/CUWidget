import { describe, expect, it } from "vitest";
import { formatVersion, logicalHeight, parseSettingsView, SETTINGS_HEIGHT, WIDGET_WIDTH, windowHeight } from "./settings";

describe("parseSettingsView", () => {
  it("accepts a complete payload", () => {
    expect(parseSettingsView({ always_on_top: true, autostart: false, version: "0.2.0" })).toEqual({
      always_on_top: true,
      autostart: false,
      version: "0.2.0",
    });
  });

  it("ignores unknown fields", () => {
    expect(parseSettingsView({ always_on_top: false, autostart: true, version: "1", extra: 1 })).toEqual({
      always_on_top: false,
      autostart: true,
      version: "1",
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

  it("keeps the widget about 260 px wide", () => {
    expect(WIDGET_WIDTH).toBe(260);
  });
});

describe("logicalHeight", () => {
  it("divides by the scale factor", () => {
    expect(logicalHeight(300, 2)).toBe(150);
    expect(logicalHeight(188, 1.25)).toBe(150);
  });

  it("rejects unusable values", () => {
    expect(logicalHeight(0, 1)).toBeNull();
    expect(logicalHeight(150, 0)).toBeNull();
    expect(logicalHeight(Number.NaN, 1)).toBeNull();
  });
});
