/** What the backend reports about the settings; it is the only source of truth. */
export interface SettingsView {
  always_on_top: boolean;
  autostart: boolean;
  version: string;
}

export type ViewName = "usage" | "settings";

export const WIDGET_WIDTH = 260;

/** Logical height of the settings view. The usage-view height comes from tauri.conf.json. */
export const SETTINGS_HEIGHT = 172;

/** Window height for a view; the usage height is whatever the window started at. */
export function windowHeight(view: ViewName, usageHeight: number): number {
  return view === "settings" ? Math.max(SETTINGS_HEIGHT, usageHeight) : usageHeight;
}

/** Logical height from a physical inner height and the scale factor; null if unusable. */
export function logicalHeight(physicalHeight: number, scaleFactor: number): number | null {
  if (!(physicalHeight > 0) || !(scaleFactor > 0)) return null;
  return Math.round(physicalHeight / scaleFactor);
}

/** Validate an untrusted payload from the backend; null when it is not a settings view. */
export function parseSettingsView(raw: unknown): SettingsView | null {
  if (typeof raw !== "object" || raw === null) return null;
  const r = raw as Record<string, unknown>;
  if (
    typeof r.always_on_top !== "boolean" ||
    typeof r.autostart !== "boolean" ||
    typeof r.version !== "string"
  ) {
    return null;
  }
  return { always_on_top: r.always_on_top, autostart: r.autostart, version: r.version };
}

export function formatVersion(version: string): string {
  const v = version.trim().replace(/^v/i, "");
  return v === "" ? "" : `v${v}`;
}
