// Canonical catalogue of metadata fields the full-screen player can show under
// the track title. The user's visible set (prefs.nowPlayingFields) is a subset
// of these ids, always rendered in this order.
//
// Deliberately unlike songColumns.ts in one respect: `value` returns null for
// missing metadata rather than an em dash. The songs table needs a placeholder
// to keep its grid cells aligned, but this row is a single line of values joined
// by separators, and "FLAC · — · — · 2019" reads as broken rather than as
// "unknown". Callers drop the nulls.

import { formatBytes, formatDuration, formatRelativeDate, formatSampleRate } from "../../lib/format";
import type { MessageKey, T } from "../../lib/i18n/core";
import type { Song } from "../../lib/subsonic/types";

export type NowPlayingFieldId =
  | "format"
  | "bitRate"
  | "sampleRate"
  | "bitDepth"
  | "year"
  | "genre"
  | "duration"
  | "trackNo"
  | "size"
  | "plays"
  | "rating"
  | "lastPlayed"
  | "added";

export interface NowPlayingField {
  id: NowPlayingFieldId;
  labelKey: MessageKey;
  /** Display text, or null when this song has no usable value for the field. */
  value: (song: Song, t: T) => string | null;
}

/** Present and usable as a magnitude — rules out undefined, NaN and 0. */
function pos(v: number | undefined): v is number {
  return v != null && Number.isFinite(v) && v > 0;
}

export const NOW_PLAYING_FIELDS: NowPlayingField[] = [
  {
    id: "format",
    labelKey: "field.format",
    value: (s) => s.suffix?.toUpperCase() ?? null,
  },
  {
    id: "bitRate",
    labelKey: "field.bitRate",
    value: (s, t) => (pos(s.bitRate) ? t("format.kbps", { n: s.bitRate }) : null),
  },
  {
    id: "sampleRate",
    labelKey: "field.sampleRate",
    value: (s, t) => (pos(s.samplingRate) ? formatSampleRate(s.samplingRate, t) : null),
  },
  {
    id: "bitDepth",
    labelKey: "field.bitDepth",
    value: (s, t) => (pos(s.bitDepth) ? t("format.bitDepth", { n: s.bitDepth }) : null),
  },
  {
    id: "year",
    labelKey: "field.year",
    value: (s) => (pos(s.year) ? String(s.year) : null),
  },
  {
    id: "genre",
    labelKey: "field.genre",
    value: (s) => s.genre ?? null,
  },
  {
    id: "duration",
    labelKey: "field.duration",
    value: (s) => (pos(s.duration) ? formatDuration(s.duration) : null),
  },
  {
    id: "trackNo",
    labelKey: "field.trackNo",
    // Disc only earns its place on a multi-disc release, where a bare track
    // number is ambiguous.
    value: (s, t) =>
      pos(s.track)
        ? pos(s.discNumber) && s.discNumber > 1
          ? t("format.discTrack", { disc: String(s.discNumber), track: String(s.track) })
          : t("format.track", { n: String(s.track) })
        : null,
  },
  {
    id: "size",
    labelKey: "field.size",
    value: (s, t) => (pos(s.size) ? formatBytes(s.size, t) : null),
  },
  {
    id: "plays",
    labelKey: "field.plays",
    // 0 is a real answer here, so this checks presence rather than magnitude.
    value: (s, t) =>
      s.playCount != null && Number.isFinite(s.playCount)
        ? t("count.plays", { n: s.playCount })
        : null,
  },
  {
    id: "rating",
    labelKey: "field.rating",
    // Subsonic reports unrated as absent or 0; both mean "no rating to show".
    value: (s, t) => (pos(s.userRating) ? t("format.ratingOutOf", { n: s.userRating }) : null),
  },
  {
    id: "lastPlayed",
    labelKey: "field.lastPlayed",
    value: (s, t) => (s.played ? formatRelativeDate(s.played, t) : null),
  },
  {
    id: "added",
    labelKey: "field.added",
    value: (s, t) => (s.created ? formatRelativeDate(s.created, t) : null),
  },
];

/**
 * The chosen fields' rendered values for one song, in catalogue order, with
 * unknown ids and fields this song has no value for dropped. Ids come back
 * alongside the text so callers get a stable key per entry.
 */
export function activeFieldValues(
  song: Song,
  ids: string[],
  t: T,
): { id: NowPlayingFieldId; value: string }[] {
  const chosen = new Set(ids);
  return NOW_PLAYING_FIELDS.filter((f) => chosen.has(f.id))
    .map((f) => ({ id: f.id, value: f.value(song, t) }))
    .filter((entry): entry is { id: NowPlayingFieldId; value: string } => !!entry.value);
}
