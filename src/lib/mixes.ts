// Wording and artwork for "Made for you" mixes. The backend says what a mix
// is; the UI names it in the current language.

import { MOSAIC_SIZE } from "../components/library/GenreMosaic";
import type { T } from "./i18n";
import type { Mix } from "./subsonic/types";

export function mixTitle(mix: Mix, t: T): string {
  switch (mix.kind) {
    case "daily":
      return t("mix.daily", { n: mix.number ?? 1 });
    case "discover":
      return t("mix.discover");
    case "onRepeat":
      return t("mix.onRepeat");
    case "deepCuts":
      return t("mix.deepCuts");
    case "forgotten":
      return t("mix.forgotten");
  }
}

export function mixDescription(mix: Mix, t: T): string {
  switch (mix.kind) {
    case "daily":
      return t("mix.withArtists", { artists: mix.artists.join(", ") });
    case "discover":
      return t("mix.discoverAbout");
    case "onRepeat":
      return t("mix.onRepeatAbout");
    case "deepCuts":
      return t("mix.deepCutsAbout");
    case "forgotten":
      return t("mix.forgottenAbout");
  }
}

/** One cover per album, in mix order, enough to fill a mosaic. */
export function mixCovers(mix: Mix): string[] {
  const seen = new Set<string>();
  const covers: string[] = [];
  for (const song of mix.songs) {
    const album = song.albumId ?? song.coverArt;
    if (!song.coverArt || !album || seen.has(album)) continue;
    seen.add(album);
    covers.push(song.coverArt);
    if (covers.length === MOSAIC_SIZE) break;
  }
  return covers;
}
