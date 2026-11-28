/** What the backend reports about the settings; it is the only source of truth. */
export interface SettingsView {
  always_on_top: boolean;
  autostart: boolean;
  version: string;
  /** Configured logical window size (tauri.conf.json); the usage view's size. */
  window_width: number;
  window_height: number;
}

export type ViewName = "usage" | "settings";

/** Logical height of the settings view. The usage-view height comes from tauri.conf.json. */
export const SETTINGS_HEIGHT = 172;

/** Window height for a view; the usage height is the configured one. */
export function windowHeight(view: ViewName, usageHeight: number): number {
  return view === "settings" ? Math.max(SETTINGS_HEIGHT, usageHeight) : usageHeight;
}

function isSize(n: unknown): n is number {
  return typeof n === "number" && Number.isFinite(n) && n > 0;
}

/** Validate an untrusted payload from the backend; null when it is not a settings view. */
export function parseSettingsView(raw: unknown): SettingsView | null {
  if (typeof raw !== "object" || raw === null) return null;
  const r = raw as Record<string, unknown>;
  if (
    typeof r.always_on_top !== "boolean" ||
    typeof r.autostart !== "boolean" ||
    typeof r.version !== "string" ||
    !isSize(r.window_width) ||
    !isSize(r.window_height)
  ) {
    return null;
  }
  return {
    always_on_top: r.always_on_top,
    autostart: r.autostart,
    version: r.version,
    window_width: r.window_width,
    window_height: r.window_height,
  };
}

export function formatVersion(version: string): string {
  const v = version.trim().replace(/^v/i, "");
  return v === "" ? "" : `v${v}`;
}
