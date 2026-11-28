import { describe, expect, it } from "vitest";
import { formatVersion, parseSettingsView, WIDGET_WIDTH, windowHeight } from "./settings";

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
  it("is taller for the settings view than the usage view", () => {
    expect(windowHeight("settings")).toBeGreaterThan(windowHeight("usage"));
  });

  it("keeps the widget about 260 px wide", () => {
    expect(WIDGET_WIDTH).toBe(260);
  });
});
