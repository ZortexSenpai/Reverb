// Canonical column catalogue for the songs table. The user's visible set
// (prefs.songColumns) is a subset of these ids, always shown in this order.

import { formatBytes, formatDuration, formatRelativeDate } from "../../lib/format";
import type { MessageKey, T } from "../../lib/i18n/core";
import type { Song } from "../../lib/subsonic/types";

export type SongColumnId =
  | "artist"
  | "album"
  | "year"
  | "genre"
  | "duration"
  | "rating"
  | "plays"
  | "lastPlayed"
  | "added"
  | "bitRate"
  | "format"
  | "size";

export interface SongColumn {
  id: SongColumnId;
  labelKey: MessageKey;
  /** CSS grid track. */
  width: string;
  align?: "right";
  /** Navidrome native-API sort field; absent = client-side sort only. */
  serverSort?: string;
  /** Cell text. Takes the translator: several columns render words. */
  value: (song: Song, t: T) => string;
  compare: (a: Song, b: Song) => number;
}

const text = (v: string | undefined) => v ?? "";
const num = (v: number | undefined) => v ?? 0;

export const SONG_COLUMNS: SongColumn[] = [
  {
    id: "artist",
    labelKey: "column.artist",
    serverSort: "artist",
    width: "minmax(110px, 1.3fr)",
    value: (s) => s.artist ?? "—",
    compare: (a, b) => text(a.artist).localeCompare(text(b.artist)),
  },
  {
    id: "album",
    labelKey: "column.album",
    serverSort: "album",
    width: "minmax(110px, 1.3fr)",
    value: (s) => s.album ?? "—",
    compare: (a, b) =>
      text(a.album).localeCompare(text(b.album)) || num(a.track) - num(b.track),
  },
  {
    id: "year",
    labelKey: "column.year",
    serverSort: "year",
    width: "56px",
    align: "right",
    value: (s) => (s.year != null ? String(s.year) : "—"),
    compare: (a, b) => num(a.year) - num(b.year),
  },
  {
    id: "genre",
    labelKey: "column.genre",
    serverSort: "genre",
    width: "minmax(80px, 0.7fr)",
    value: (s) => s.genre ?? "—",
    compare: (a, b) => text(a.genre).localeCompare(text(b.genre)),
  },
  {
    id: "duration",
    labelKey: "column.duration",
    serverSort: "duration",
    width: "64px",
    align: "right",
    value: (s) => formatDuration(s.duration),
    compare: (a, b) => num(a.duration) - num(b.duration),
  },
  {
    id: "rating",
    labelKey: "column.rating",
    serverSort: "rating",
    // Wide enough for five stars; SongTable renders this one itself.
    width: "92px",
    value: (s) => (s.userRating ? String(s.userRating) : "—"),
    compare: (a, b) => num(a.userRating) - num(b.userRating),
  },
  {
    id: "plays",
    labelKey: "column.plays",
    serverSort: "playCount",
    width: "60px",
    align: "right",
    value: (s, t) => (s.playCount != null ? t.n(s.playCount) : "—"),
    compare: (a, b) => num(a.playCount) - num(b.playCount),
  },
  {
    id: "lastPlayed",
    labelKey: "column.lastPlayed",
    serverSort: "playDate",
    width: "100px",
    value: (s, t) => formatRelativeDate(s.played, t),
    // ISO timestamps compare correctly as strings; missing sorts oldest.
    compare: (a, b) => text(a.played).localeCompare(text(b.played)),
  },
  {
    id: "added",
    labelKey: "column.added",
    serverSort: "createdAt",
    width: "100px",
    value: (s, t) => formatRelativeDate(s.created, t),
    compare: (a, b) => text(a.created).localeCompare(text(b.created)),
  },
  {
    id: "bitRate",
    labelKey: "column.bitRate",
    serverSort: "bitRate",
    width: "76px",
    align: "right",
    value: (s, t) => (s.bitRate != null ? t("format.kbps", { n: s.bitRate }) : "—"),
    compare: (a, b) => num(a.bitRate) - num(b.bitRate),
  },
  {
    id: "format",
    labelKey: "column.format",
    width: "64px",
    value: (s) => s.suffix?.toUpperCase() ?? "—",
    compare: (a, b) => text(a.suffix).localeCompare(text(b.suffix)),
  },
  {
    id: "size",
    labelKey: "column.size",
    serverSort: "size",
    width: "72px",
    align: "right",
    value: (s, t) => formatBytes(s.size, t),
    compare: (a, b) => num(a.size) - num(b.size),
  },
];

/** Visible columns in the user's chosen order (unknown ids are dropped). */
export function activeColumns(ids: string[], exclude: SongColumnId[] = []): SongColumn[] {
  return ids
    .map((id) => SONG_COLUMNS.find((c) => c.id === id))
    .filter((c): c is SongColumn => c != null && !exclude.includes(c.id));
}

/** Comparator for a sort id ("title" or any column id); null = no sort. */
export function compareSongsBy(sortId: string): ((a: Song, b: Song) => number) | null {
  if (sortId === "title") return (a, b) => a.title.localeCompare(b.title);
  return SONG_COLUMNS.find((c) => c.id === sortId)?.compare ?? null;
}

/** Navidrome native-API sort field for a sort id, when one exists. */
export function serverSortFor(sortId: string): string | undefined {
  if (sortId === "title") return "title";
  return SONG_COLUMNS.find((c) => c.id === sortId)?.serverSort;
}

/** Shared grid template: [# ] [title] [...columns] [heart]. */
export function songGridTemplate(columns: SongColumn[]): string {
  return `40px minmax(180px, 2fr) ${columns.map((c) => c.width).join(" ")} 44px`;
}
