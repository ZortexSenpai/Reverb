// Value formatting. Anything whose output contains words takes the translator
// as an argument rather than reading a store, so these stay pure and the
// caller — which already has `useT()` — decides the language.

import type { T } from "./i18n/core";
import type { RankMovement } from "./subsonic/types";

export function formatDuration(totalSeconds: number | undefined): string {
  if (totalSeconds == null || !Number.isFinite(totalSeconds)) return "–:––";
  const s = Math.max(0, Math.round(totalSeconds));
  const hours = Math.floor(s / 3600);
  const minutes = Math.floor((s % 3600) / 60);
  const seconds = s % 60;
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`
    : `${minutes}:${String(seconds).padStart(2, "0")}`;
}

export function formatMs(ms: number): string {
  return formatDuration(ms / 1000);
}

/** "3 hr 42 min" style for album/playlist headers. */
export function formatLongDuration(totalSeconds: number, t: T): string {
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.round((totalSeconds % 3600) / 60);
  if (hours > 0) return t("format.hoursMinutes", { h: hours, m: minutes });
  return t("format.minutes", { m: minutes });
}

/** "≈ 142 h": an estimated listening time, in minutes below an hour. */
export function formatListeningTime(totalSeconds: number, t: T): string {
  const minutes = Math.round(totalSeconds / 60);
  if (minutes < 60) return t("format.aboutMinutes", { n: t.n(minutes) });
  const hours = totalSeconds / 3600;
  // A tenth of an hour says something about an afternoon, not a year.
  const digits = hours < 10 ? 1 : 0;
  return t("format.aboutHours", { n: t.n(hours, { maximumFractionDigits: digits }) });
}

/** "from 82% of plays": the share of the Period's plays a stat is based on. */
export function formatCoverage(share: number, t: T): string {
  return t("stats.coverage", {
    share: t.n(share, { style: "percent", maximumFractionDigits: 0 }),
  });
}

/** "+12%": a count's change since the Previous period, unsigned when it rounds to 0. */
export function formatChange(change: number, t: T): string {
  return t.n(change, { style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 0 });
}

/** "▲37", "▼3", "=" or "new": a top-list entry's places moved since the Previous period. */
export function formatMovement(movement: RankMovement, t: T): string {
  if (movement.kind === "new") return t("stats.movementNew");
  const { places } = movement;
  if (places === 0) return "=";
  return `${places > 0 ? "▲" : "▼"}${t.n(Math.abs(places))}`;
}

/** "44.1 kHz" — sample rates are quoted in kHz everywhere in the UI. */
export function formatSampleRate(hz: number, t: T): string {
  return `${t.n(hz / 1000, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} kHz`;
}

export function formatBytes(bytes: number | undefined, t: T): string {
  if (bytes == null || !Number.isFinite(bytes)) return "—";
  const oneDecimal = { minimumFractionDigits: 1, maximumFractionDigits: 1 };
  if (bytes >= 1024 ** 3) return `${t.n(bytes / 1024 ** 3, oneDecimal)} GB`;
  if (bytes >= 1024 ** 2) return `${t.n(bytes / 1024 ** 2, oneDecimal)} MB`;
  return `${t.n(Math.round(bytes / 1024))} kB`;
}

/** Compact relative time for "last played" style columns. */
export function formatRelativeDate(iso: string | undefined, t: T): string {
  if (!iso) return "—";
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "—";
  const seconds = Math.max(0, (Date.now() - then) / 1000);
  if (seconds < 3600) {
    return t("format.minutesAgo", { n: Math.max(1, Math.floor(seconds / 60)) });
  }
  if (seconds < 86400) return t("format.hoursAgo", { n: Math.floor(seconds / 3600) });
  const days = Math.floor(seconds / 86400);
  if (days < 30) return t("format.daysAgo", { n: days });
  if (days < 365) return t("format.monthsAgo", { n: Math.floor(days / 30) });
  return t("format.yearsAgo", { n: Math.floor(days / 365) });
}
