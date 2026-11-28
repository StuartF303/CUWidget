import { invoke } from "@tauri-apps/api/core";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  barLevel,
  barWidth,
  effectiveStatus,
  formatCountdown,
  formatFooter,
  formatPercent,
  formatStatus,
  visibleWindows,
  type UsageWindow,
  type WidgetState,
} from "./format";
import {
  formatVersion,
  parseSettingsView,
  WIDGET_WIDTH,
  windowHeight,
  type SettingsView,
  type ViewName,
} from "./settings";

const DEMO_STATE: WidgetState = {
  status: { kind: "live" },
  five_hour: { utilization: 58, resets_at: new Date(Date.now() + 102 * 60_000).toISOString() },
  seven_day: { utilization: 23, resets_at: new Date(Date.now() + (4 * 24 + 6) * 3_600_000).toISOString() },
  rate_pct_per_hr: 12,
  projection: { kind: "hits_limit_at", at: new Date(Date.now() + 3 * 3_600_000).toISOString() },
  fetched_at: new Date().toISOString(),
};

const DEMO_SETTINGS: SettingsView = { always_on_top: false, autostart: false, version: "0.2.0" };

const params = new URLSearchParams(location.search);
const demo = params.has("demo");

let state: WidgetState | null = null;
let view: ViewName = "usage";
let demoSettings: SettingsView = { ...DEMO_SETTINGS };

function el<T extends HTMLElement>(selector: string): T {
  const found = document.querySelector<T>(selector);
  if (!found) throw new Error(`missing element ${selector}`);
  return found;
}

const rowKeys = ["five_hour", "seven_day"] as const;

function renderCountdowns(): void {
  const now = new Date();
  const windows = visibleWindows(state);
  for (const key of rowKeys) {
    const w: UsageWindow | null = windows[key];
    el(`#row-${key} .reset`).textContent = w ? formatCountdown(w.resets_at, now) : "";
  }
}

function renderStatus(): void {
  const status = formatStatus(effectiveStatus(state, new Date()));
  el("#status-dot").className = `dot ${status.kind}`;
  el("#status-text").textContent = status.text;
  el("#status-text").title = status.text;
}

function render(): void {
  renderStatus();

  const windows = visibleWindows(state);
  for (const key of rowKeys) {
    const utilization = windows[key]?.utilization ?? null;
    const fill = el(`#row-${key} .fill`);
    fill.style.width = `${barWidth(utilization)}%`;
    fill.className = `fill ${barLevel(utilization)}`;
    el(`#row-${key} .pct`).textContent = formatPercent(utilization);
  }
  el("#footer").textContent = formatFooter(state);
  renderCountdowns();
}

function setState(next: WidgetState | null): void {
  state = next;
  render();
}

/** Ask the backend; in demo mode a local stand-in answers. */
async function settingsCall(command: string, args?: Record<string, unknown>): Promise<SettingsView | null> {
  if (demo) {
    if (command === "set_always_on_top_cmd") demoSettings = { ...demoSettings, always_on_top: args?.value === true };
    if (command === "set_autostart_cmd") demoSettings = { ...demoSettings, autostart: args?.value === true };
    return demoSettings;
  }
  try {
    return parseSettingsView(await invoke<unknown>(command, args));
  } catch {
    return null;
  }
}

function renderSettings(settings: SettingsView | null): void {
  if (!settings) return;
  el<HTMLInputElement>("#opt-always-on-top").checked = settings.always_on_top;
  el<HTMLInputElement>("#opt-autostart").checked = settings.autostart;
  el("#version").textContent = formatVersion(settings.version);
}

/** Show whatever the backend reports; if it did not answer, ask again. */
async function applySettingsResult(result: SettingsView | null): Promise<void> {
  renderSettings(result ?? (await settingsCall("get_settings")));
}

async function resizeWindow(target: ViewName): Promise<void> {
  try {
    await getCurrentWindow().setSize(new LogicalSize(WIDGET_WIDTH, windowHeight(target)));
  } catch {
    // Not running inside Tauri (e.g. a plain browser): nothing to resize.
  }
}

async function setView(next: ViewName): Promise<void> {
  if (next === view) return;
  view = next;
  const open = next === "settings";
  el("#settings-toggle").setAttribute("aria-expanded", String(open));
  if (open) {
    // Grow first so the panel never shows clipped, then swap the content.
    await resizeWindow(next);
    el("#usage-view").hidden = true;
    el("#settings").hidden = false;
    await applySettingsResult(await settingsCall("get_settings"));
    el("#btn-done").focus();
  } else {
    el("#settings").hidden = true;
    el("#usage-view").hidden = false;
    await resizeWindow(next);
    el("#settings-toggle").focus();
  }
}

function bindSettings(): void {
  const onTop = el<HTMLInputElement>("#opt-always-on-top");
  const autostart = el<HTMLInputElement>("#opt-autostart");
  const reset = el<HTMLButtonElement>("#btn-reset-position");

  el("#settings-toggle").addEventListener("click", () => void setView(view === "usage" ? "settings" : "usage"));
  el("#btn-done").addEventListener("click", () => void setView("usage"));
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && view === "settings") void setView("usage");
  });

  for (const [box, command] of [
    [onTop, "set_always_on_top_cmd"],
    [autostart, "set_autostart_cmd"],
  ] as const) {
    box.addEventListener("change", () => {
      box.disabled = true;
      void settingsCall(command, { value: box.checked })
        .then(applySettingsResult)
        .finally(() => {
          box.disabled = false;
        });
    });
  }

  let resetTimer: number | undefined;
  reset.addEventListener("click", () => {
    void settingsCall("reset_position_cmd").then((result) => {
      void applySettingsResult(result);
      reset.textContent = result ? "Moved to top right" : "Could not move";
      window.clearTimeout(resetTimer);
      resetTimer = window.setTimeout(() => {
        reset.textContent = "Reset position";
      }, 1800);
    });
  });
}

async function init(): Promise<void> {
  render();
  bindSettings();
  setInterval(() => {
    renderCountdowns();
    renderStatus();
  }, 1000);

  if (demo) {
    setState(DEMO_STATE);
    renderSettings(demoSettings);
    if (params.get("view") === "settings") await setView("settings");
    return;
  }

  try {
    await listen<unknown>("settings-changed", (event) => renderSettings(parseSettingsView(event.payload)));
    await listen("open-settings", () => void setView("settings"));
    await listen<WidgetState>("usage-update", (event) => setState(event.payload));
  } catch {
    // Not running inside Tauri (e.g. a plain browser): stay in the loading state.
    return;
  }

  try {
    const initial = await invoke<WidgetState | null>("get_state");
    // An event may have arrived while get_state was in flight; it is newer.
    if (initial && state === null) setState(initial);
  } catch {
    // Live updates still arrive via the event subscription.
  }
}

void init();
