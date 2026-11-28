/** What the backend reports about the settings; it is the only source of truth. */
export interface SettingsView {
  always_on_top: boolean;
  autostart: boolean;
  version: string;
}

export type ViewName = "usage" | "settings";

export const WIDGET_WIDTH = 260;

/** Logical window heights: keep in step with `height` in tauri.conf.json. */
const HEIGHTS: Record<ViewName, number> = {
  usage: 150,
  settings: 172,
};

export function windowHeight(view: ViewName): number {
  return HEIGHTS[view];
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
