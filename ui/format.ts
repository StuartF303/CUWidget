export type Status =
  | { kind: "live" }
  | { kind: "stale"; since: string }
  | { kind: "signed_out" };

export interface Window {
  utilization: number;
  resets_at: string | null;
}

export type Projection =
  | { kind: "hits_limit_at"; at: string }
  | { kind: "ok_until_reset" }
  | { kind: "unknown" };

export interface WidgetState {
  status: Status;
  five_hour: Window | null;
  seven_day: Window | null;
  rate_pct_per_hr: number | null;
  projection: Projection;
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
  if (resetsAt === null) return "reset time unknown";
  const ms = new Date(resetsAt).getTime() - now.getTime();
  if (ms <= 0) return "resetting…";
  const minutes = Math.floor(ms / 60_000);
  if (minutes < 1) return "resets <1m";
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);
  if (days >= 1) return `resets ${days}d ${hours % 24}h`;
  if (hours >= 1) return `resets ${hours}h ${minutes % 60}m`;
  return `resets ${minutes}m`;
}

function localHhmm(iso: string): string {
  const d = new Date(iso);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** The windows to show: none while loading or signed out. */
export function visibleWindows(state: WidgetState | null): {
  five_hour: Window | null;
  seven_day: Window | null;
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
  if (rate !== null) {
    parts.push(`${rate >= 10 ? Math.round(rate) : rate.toFixed(1)} %/hr`);
  }
  const p = state.projection;
  if (p.kind === "hits_limit_at") parts.push(`hits limit ~${localHhmm(p.at)}`);
  else if (p.kind === "ok_until_reset") parts.push("OK until reset");
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
    case "stale":
      return { kind: "stale", text: `stale since ${localHhmm(status.since)}` };
    case "signed_out":
      return { kind: "signed_out", text: "sign in to Claude Code" };
  }
}
