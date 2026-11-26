import { describe, expect, it } from "vitest";
import {
  barLevel,
  barWidth,
  formatCountdown,
  formatFooter,
  formatPercent,
  formatStatus,
  visibleWindows,
  type WidgetState,
} from "./format";

const now = new Date("2026-01-22T08:00:00Z");
const after = (ms: number) => new Date(now.getTime() + ms).toISOString();
const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

// Local-time ISO string, so HH:MM assertions hold in any time zone.
const localIso = (h: number, m: number) => new Date(2026, 0, 22, h, m).toISOString();

describe("formatPercent", () => {
  it("rounds to the nearest integer", () => {
    expect(formatPercent(57.6)).toBe("58%");
    expect(formatPercent(23.4)).toBe("23%");
    expect(formatPercent(0)).toBe("0%");
  });
  it("shows the raw number above 100", () => {
    expect(formatPercent(104.5)).toBe("105%");
  });
  it("shows a dash for a missing window", () => {
    expect(formatPercent(null)).toBe("—");
  });
});

describe("barWidth", () => {
  it("clamps to 0-100", () => {
    expect(barWidth(58)).toBe(58);
    expect(barWidth(104.5)).toBe(100);
    expect(barWidth(-3)).toBe(0);
  });
  it("is empty for a missing window", () => {
    expect(barWidth(null)).toBe(0);
  });
});

describe("barLevel", () => {
  it("is normal below 50, amber from 50, red from 80", () => {
    expect(barLevel(49.9)).toBe("normal");
    expect(barLevel(50)).toBe("amber");
    expect(barLevel(79.9)).toBe("amber");
    expect(barLevel(80)).toBe("red");
    expect(barLevel(120)).toBe("red");
  });
  it("is normal for a missing window", () => {
    expect(barLevel(null)).toBe("normal");
  });
});

describe("formatCountdown", () => {
  it("shows days and hours from one day", () => {
    expect(formatCountdown(after(4 * DAY + 6 * HOUR + 30 * MIN), now)).toBe("resets 4d 6h");
    expect(formatCountdown(after(DAY), now)).toBe("resets 1d 0h");
  });
  it("shows hours and minutes from one hour", () => {
    expect(formatCountdown(after(HOUR + 42 * MIN + 10_000), now)).toBe("resets 1h 42m");
    expect(formatCountdown(after(HOUR), now)).toBe("resets 1h 0m");
    expect(formatCountdown(after(DAY - MIN), now)).toBe("resets 23h 59m");
  });
  it("shows minutes under an hour", () => {
    expect(formatCountdown(after(42 * MIN + 59_000), now)).toBe("resets 42m");
    expect(formatCountdown(after(MIN), now)).toBe("resets 1m");
  });
  it("shows <1m under a minute", () => {
    expect(formatCountdown(after(59_999), now)).toBe("resets <1m");
    expect(formatCountdown(after(1), now)).toBe("resets <1m");
  });
  it("shows resetting once past or due", () => {
    expect(formatCountdown(after(0), now)).toBe("resetting…");
    expect(formatCountdown(after(-5 * MIN), now)).toBe("resetting…");
  });
  it("handles an unknown reset time", () => {
    expect(formatCountdown(null, now)).toBe("reset time unknown");
  });
});

const base: WidgetState = {
  status: { kind: "live" },
  five_hour: null,
  seven_day: null,
  rate_pct_per_hr: null,
  projection: { kind: "unknown" },
};

describe("formatFooter", () => {
  it("joins rate and projected limit time", () => {
    const s: WidgetState = {
      ...base,
      rate_pct_per_hr: 12,
      projection: { kind: "hits_limit_at", at: localIso(19, 55) },
    };
    expect(formatFooter(s)).toBe("12 %/hr · hits limit ~19:55");
  });
  it("pads local hours and minutes to two digits", () => {
    const s: WidgetState = { ...base, projection: { kind: "hits_limit_at", at: localIso(7, 5) } };
    expect(formatFooter(s)).toBe("hits limit ~07:05");
  });
  it("uses an integer rate at 10 and above, one decimal below", () => {
    expect(formatFooter({ ...base, rate_pct_per_hr: 12.4 })).toBe("12 %/hr");
    expect(formatFooter({ ...base, rate_pct_per_hr: 10 })).toBe("10 %/hr");
    expect(formatFooter({ ...base, rate_pct_per_hr: 3.44 })).toBe("3.4 %/hr");
    expect(formatFooter({ ...base, rate_pct_per_hr: 0 })).toBe("0.0 %/hr");
  });
  it("shows OK until reset", () => {
    const s: WidgetState = { ...base, rate_pct_per_hr: 3.4, projection: { kind: "ok_until_reset" } };
    expect(formatFooter(s)).toBe("3.4 %/hr · OK until reset");
  });
  it("omits the rate when null and the projection when unknown", () => {
    expect(formatFooter({ ...base, projection: { kind: "ok_until_reset" } })).toBe("OK until reset");
    expect(formatFooter({ ...base, rate_pct_per_hr: 5 })).toBe("5.0 %/hr");
  });
  it("is empty when both are absent", () => {
    expect(formatFooter(base)).toBe("");
  });
});

describe("formatStatus", () => {
  it("is neutral loading before any state", () => {
    expect(formatStatus(null)).toEqual({ kind: "loading", text: "loading…" });
  });
  it("live", () => {
    expect(formatStatus({ kind: "live" })).toEqual({ kind: "live", text: "live" });
  });
  it("stale shows local HH:MM", () => {
    expect(formatStatus({ kind: "stale", since: localIso(4, 1) })).toEqual({
      kind: "stale",
      text: "stale since 04:01",
    });
  });
  it("signed out", () => {
    expect(formatStatus({ kind: "signed_out" })).toEqual({
      kind: "signed_out",
      text: "sign in to Claude Code",
    });
  });
});

describe("signed out and loading", () => {
  const w = { utilization: 90, resets_at: null };
  const stale: WidgetState = { ...base, five_hour: w, seven_day: w, rate_pct_per_hr: 5 };

  it("shows both windows while a state is present", () => {
    expect(visibleWindows(stale)).toEqual({ five_hour: w, seven_day: w });
  });
  it("hides windows and footer when signed out", () => {
    const s: WidgetState = { ...stale, status: { kind: "signed_out" } };
    expect(visibleWindows(s)).toEqual({ five_hour: null, seven_day: null });
    expect(formatFooter(s)).toBe("");
  });
  it("hides windows and footer before any state", () => {
    expect(visibleWindows(null)).toEqual({ five_hour: null, seven_day: null });
    expect(formatFooter(null)).toBe("");
  });
});
