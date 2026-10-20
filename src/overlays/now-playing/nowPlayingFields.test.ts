import { describe, expect, it } from "vitest";
import { translator } from "../../lib/i18n/core";
import type { Song } from "../../lib/subsonic/types";
import { activeFieldValues, NOW_PLAYING_FIELDS } from "./nowPlayingFields";

const t = translator("en");

/** A song with every field the catalogue can read populated. */
const full: Song = {
  id: "1",
  title: "Test",
  album: "Album",
  artist: "Artist",
  track: 4,
  discNumber: 1,
  year: 2019,
  genre: "Ambient",
  suffix: "flac",
  size: 42_000_000,
  duration: 272,
  bitRate: 1411,
  samplingRate: 44100,
  bitDepth: 16,
  userRating: 4,
  playCount: 7,
  played: new Date().toISOString(),
  created: new Date().toISOString(),
};

const ids = (song: Song, want: string[]) => activeFieldValues(song, want, t).map((f) => f.id);
const values = (song: Song, want: string[]) =>
  activeFieldValues(song, want, t).map((f) => f.value);

describe("activeFieldValues", () => {
  // Figures go through the locale's number format, same as everywhere else —
  // so a bitrate is grouped here exactly as it is in the songs table.
  it("formats the audio fields the way the rest of the UI quotes them", () => {
    expect(values(full, ["format", "bitRate", "sampleRate", "bitDepth"])).toEqual([
      "FLAC",
      "1,411 kbps",
      "44.1 kHz",
      "16-bit",
    ]);
    expect(
      activeFieldValues(full, ["bitRate", "sampleRate"], translator("de")).map((f) => f.value),
    ).toEqual(["1.411 kbit/s", "44,1 kHz"]);
  });

  it("returns catalogue order, not the order the ids were given in", () => {
    expect(ids(full, ["year", "format", "duration", "bitRate"])).toEqual([
      "format",
      "bitRate",
      "year",
      "duration",
    ]);
  });

  it("drops ids that are not in the catalogue", () => {
    expect(ids(full, ["format", "nonsense", "waveform"])).toEqual(["format"]);
  });

  it("returns nothing when no fields are selected", () => {
    expect(activeFieldValues(full, [], t)).toEqual([]);
  });

  // The reason `value` returns null instead of an em dash: the row is joined by
  // separators, so placeholders would render as "FLAC · — · —".
  it("omits fields the song has no value for rather than showing a placeholder", () => {
    const sparse: Song = { id: "2", title: "Sparse", suffix: "mp3" };
    expect(values(sparse, ["format", "bitRate", "sampleRate", "year", "genre"])).toEqual(["MP3"]);
  });

  it("treats a zero bitrate or year as missing, not as a value to show", () => {
    const zeroed: Song = { id: "3", title: "Zeroed", bitRate: 0, year: 0, samplingRate: 0 };
    expect(activeFieldValues(zeroed, ["bitRate", "year", "sampleRate"], t)).toEqual([]);
  });

  // Subsonic reports unrated tracks as absent or 0, and both mean "no rating".
  it("hides an unrated track's rating but shows a real one", () => {
    expect(values({ id: "4", title: "A", userRating: 0 }, ["rating"])).toEqual([]);
    expect(values({ id: "5", title: "B", userRating: 3 }, ["rating"])).toEqual(["3/5"]);
  });

  // Unlike a rating, zero plays is a fact worth showing.
  it("shows a zero play count and pluralises it", () => {
    expect(values({ id: "6", title: "A", playCount: 0 }, ["plays"])).toEqual(["0 plays"]);
    expect(values({ id: "7", title: "B", playCount: 1 }, ["plays"])).toEqual(["1 play"]);
  });

  it("names the disc only when there is more than one", () => {
    expect(values({ id: "8", title: "A", track: 4, discNumber: 1 }, ["trackNo"])).toEqual([
      "Track 4",
    ]);
    expect(values({ id: "9", title: "B", track: 4, discNumber: 2 }, ["trackNo"])).toEqual([
      "Disc 2 · Track 4",
    ]);
  });

  it("renders through the translator it is handed, not a fixed language", () => {
    const de = translator("de");
    expect(activeFieldValues(full, ["trackNo", "plays"], de).map((f) => f.value)).toEqual([
      "Track 4",
      "7 Wiedergaben",
    ]);
  });

  it("has unique ids, so selections cannot collide", () => {
    const seen = NOW_PLAYING_FIELDS.map((f) => f.id);
    expect(new Set(seen).size).toBe(seen.length);
  });

  // Every catalogue entry must survive the fully-populated song, or a field
  // would be selectable in settings yet never able to render.
  it("renders every field for a song that has all the metadata", () => {
    const all = NOW_PLAYING_FIELDS.map((f) => f.id);
    expect(ids(full, all)).toEqual(all);
  });
});
