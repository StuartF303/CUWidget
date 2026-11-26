export type Status =
  | { kind: "live" }
  | { kind: "stale"; since: string }
  | { kind: "signed_out" };

export interface UsageWindow {
  utilization: number;
  resets_at: string | null;
}

export type Projection =
  | { kind: "hits_limit_at"; at: string }
  | { kind: "ok_until_reset" }
  | { kind: "limit_reached" }
  | { kind: "unknown" };

export interface WidgetState {
  status: Status;
  five_hour: UsageWindow | null;
  seven_day: UsageWindow | null;
  rate_pct_per_hr: number | null;
  projection: Projection;
  /** RFC 3339 time of the last poll attempt. */
  fetched_at: string;
}

export type BarLevel = "normal" | "amber" | "red";

const DASH = "—";

export function formatPercent(utilization: number | null): string {
  return utilization === null ? DASH : `${Math.round(utilization)}%`;
}

export function barWidth(utilization: number | null): number {
  return utilization === null ? 0 : Math.min(100, Math.max(0, utilization));
}

export function barLevel(utilization: number | null): BarLevel {
  if (utilization === null) return "normal";
  if (utilization >= 80) return "red";
  if (utilization >= 50) return "amber";
  return "normal";
}

export function formatCountdown(resetsAt: string | null, now: Date): string {
  if (resetsAt == null) return "reset time unknown";
  const ms = new Date(resetsAt).getTime() - now.getTime();
  if (Number.isNaN(ms)) return "reset time unknown";
  if (ms <= 0) return "resetting…";
  const minutes = Math.floor(ms / 60_000);
  if (minutes < 1) return "resets <1m";
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);
  if (days >= 1) return `resets ${days}d ${hours % 24}h`;
  if (hours >= 1) return `resets ${hours}h ${minutes % 60}m`;
  return `resets ${minutes}m`;
}

/** Local 24-hour HH:MM, or null when the date is missing or invalid. */
function localHhmm(iso: string | null | undefined): string | null {
  if (iso == null) return null;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return null;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** The windows to show: none while loading or signed out. */
export function visibleWindows(state: WidgetState | null): {
  five_hour: UsageWindow | null;
  seven_day: UsageWindow | null;
} {
  if (state === null || state.status.kind === "signed_out") {
    return { five_hour: null, seven_day: null };
  }
  return { five_hour: state.five_hour, seven_day: state.seven_day };
}

export function formatFooter(state: WidgetState | null): string {
  if (state === null || state.status.kind === "signed_out") return "";
  const parts: string[] = [];
  const rate = state.rate_pct_per_hr;
  const p = state.projection;
  // At the limit a rate of zero says nothing useful.
  if (rate !== null && !(p.kind === "limit_reached" && rate <= 0)) {
    parts.push(`${rate >= 10 ? Math.round(rate) : rate.toFixed(1)} %/hr`);
  }
  if (p.kind === "limit_reached") parts.push("limit reached");
  else if (p.kind === "hits_limit_at") {
    const at = localHhmm(p.at);
    if (at !== null) parts.push(`hits limit ~${at}`);
  } else if (p.kind === "ok_until_reset") parts.push("OK until reset");
  return parts.join(" · ");
}

export function formatStatus(status: Status | null): {
  kind: Status["kind"] | "loading";
  text: string;
} {
  if (status === null) return { kind: "loading", text: "loading…" };
  switch (status.kind) {
    case "live":
      return { kind: "live", text: "live" };
    case "stale": {
      const since = localHhmm(status.since);
      return { kind: "stale", text: since === null ? "stale" : `stale since ${since}` };
    }
    case "signed_out":
      return { kind: "signed_out", text: "open Claude Code to sign in" };
  }
}

/** Without a poll attempt for this long a "live" status is no longer believable. */
export const WATCHDOG_MS = 35 * 60_000;

/**
 * The status to display: a "live" state whose last poll attempt is more than
 * 35 minutes old is shown as stale since that attempt (the poller has stopped
 * emitting). A missing or unparsable `fetched_at` never trips the watchdog.
 */
export function effectiveStatus(state: WidgetState | null, now: Date): Status | null {
  if (state === null) return null;
  if (state.status.kind === "live") {
    const age = now.getTime() - new Date(state.fetched_at).getTime();
    if (age > WATCHDOG_MS) return { kind: "stale", since: state.fetched_at };
  }
  return state.status;
}
