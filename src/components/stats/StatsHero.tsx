// The top of the stats page: the Period's #1 artist shown large, with the
// #1 album and Song beside it. A slot the summary leaves out collapses, so the
// hero never shows an empty frame.

import { useT } from "../../lib/i18n";
import type { StatsHero as Hero } from "../../lib/subsonic/types";
import { StatsEntry } from "./StatsEntry";

/** A soft accent glow behind the artist: the page's one hue, in any theme. */
const GLOW =
  "radial-gradient(ellipse at 0% 0%, color-mix(in oklab, var(--color-accent) 16%, transparent), transparent 70%)";

export function StatsHero({ hero }: { hero: Hero }) {
  const t = useT();
  const { artist, album, song } = hero;
  const share = t.n(hero.share, {
    style: "percent",
    // "0%" would undersell a #1 in a very varied Period.
    maximumFractionDigits: hero.share < 0.1 ? 1 : 0,
  });

  return (
    <section
      className="flex flex-col gap-2 rounded-xl border border-edge bg-surface p-2 sm:p-3 lg:flex-row lg:items-center"
      style={{ backgroundImage: GLOW }}
    >
      <div className="min-w-0 flex-1">
        <StatsEntry
          kind="artist"
          item={artist}
          layout="hero"
          label={t("stats.topArtist")}
          count={artist.count}
          detail={t("stats.shareOfPlays", { share })}
        />
      </div>
      {(album || song) && (
        <div className="flex gap-1">
          {album && (
            <div className="w-36 sm:w-40">
              <StatsEntry
                kind="album"
                item={album}
                layout="tile"
                label={t("stats.topAlbum")}
                count={album.count}
              />
            </div>
          )}
          {song && (
            <div className="w-36 sm:w-40">
              <StatsEntry
                kind="song"
                item={song}
                layout="tile"
                label={t("stats.topSong")}
                count={song.count}
              />
            </div>
          )}
        </div>
      )}
    </section>
  );
}
