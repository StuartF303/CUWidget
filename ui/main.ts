import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  barLevel,
  barWidth,
  formatCountdown,
  formatFooter,
  formatPercent,
  formatStatus,
  visibleWindows,
  type Window,
  type WidgetState,
} from "./format";

const DEMO_STATE: WidgetState = {
  status: { kind: "live" },
  five_hour: { utilization: 58, resets_at: new Date(Date.now() + 102 * 60_000).toISOString() },
  seven_day: { utilization: 23, resets_at: new Date(Date.now() + (4 * 24 + 6) * 3_600_000).toISOString() },
  rate_pct_per_hr: 12,
  projection: { kind: "hits_limit_at", at: new Date(Date.now() + 3 * 3_600_000).toISOString() },
};

let state: WidgetState | null = null;

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
    const w: Window | null = windows[key];
    el(`#row-${key} .reset`).textContent = w ? formatCountdown(w.resets_at, now) : "";
  }
}

function render(): void {
  const status = formatStatus(state ? state.status : null);
  el("#status-dot").className = `dot ${status.kind}`;
  el("#status-text").textContent = status.text;

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

async function init(): Promise<void> {
  render();
  setInterval(renderCountdowns, 1000);

  if (new URLSearchParams(location.search).has("demo")) {
    setState(DEMO_STATE);
    return;
  }

  try {
    const initial = await invoke<WidgetState | null>("get_state");
    if (initial) setState(initial);
    await listen<WidgetState>("usage-update", (event) => setState(event.payload));
  } catch {
    // Not running inside Tauri (e.g. a plain browser): stay in the loading state.
  }
}

void init();
