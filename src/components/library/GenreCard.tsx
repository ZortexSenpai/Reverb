import { useNavigate } from "react-router";
import { useT } from "../../lib/i18n";
import { genrePath } from "../../lib/subsonic/genres";
import { useGenreMosaicAlbums } from "../../lib/subsonic/queries";
import type { Genre } from "../../lib/subsonic/types";
import { GenreMosaic } from "./GenreMosaic";

export function GenreCard({ genre }: { genre: Genre }) {
  const t = useT();
  const navigate = useNavigate();

  // CardGrid virtualizes rows, so only on-screen tiles ever mount — this
  // fetch is lazy by construction, no intersection observer needed.
  const albums = useGenreMosaicAlbums(genre.value);
  const covers = (albums.data ?? []).map((album) => album.coverArt);

  return (
    <div
      onClick={() => navigate(genrePath(genre.value))}
      className="group w-full cursor-pointer p-3"
    >
      <div className="relative overflow-hidden rounded-md ring-1 ring-white/5 transition-all group-hover:ring-accent/40">
        <GenreMosaic covers={covers} size={300} />

        {/* Scrim has to hold up over pale artwork, so it stays opaque under
            the text and only fades out above the halfway mark. */}
        <div className="absolute inset-0 bg-gradient-to-t from-black/95 from-0% via-black/70 via-25% to-transparent to-60%" />
        <div className="absolute inset-x-0 bottom-0 p-3">
          <div className="truncate text-sm font-semibold text-white drop-shadow-md">
            {genre.value}
          </div>
          <div className="truncate text-xs text-white/75 drop-shadow-md">
            {t("count.albums", { n: genre.albumCount })} ·{" "}
            {t("count.songs", { n: genre.songCount })}
          </div>
        </div>
      </div>
    </div>
  );
}
