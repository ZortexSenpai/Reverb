import { ArrowLeft, Play, Shuffle } from "lucide-react";
import { useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import { AlbumGrid } from "../../components/library/AlbumGrid";
import { GenreMosaic } from "../../components/library/GenreMosaic";
import { PaginationBar } from "../../components/library/PaginationBar";
import { Button, IconButton } from "../../components/ui/Button";
import { CenteredSpinner } from "../../components/ui/Spinner";
import { artUrl } from "../../lib/art";
import { useT } from "../../lib/i18n";
import { cmd } from "../../lib/ipc/commands";
import {
  ALBUM_PAGE_SIZE,
  useAlbumPage,
  useGenreMosaicAlbums,
  useGenres,
} from "../../lib/subsonic/queries";
import { usePlayerStore } from "../../stores/playerStore";

/** Upper bound on tracks pulled for whole-genre playback. */
const PLAY_LIMIT = 500;

export function GenreDetailView() {
  const t = useT();
  const { genre: rawGenre = "" } = useParams();
  const genre = decodeURIComponent(rawGenre);
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const scrollRef = useRef<HTMLDivElement>(null);
  const playContext = usePlayerStore((s) => s.playContext);
  const [starting, setStarting] = useState(false);

  const page = Math.max(0, Number(params.get("page") ?? "0") || 0);
  const pageSize = Number(params.get("size") ?? "") || ALBUM_PAGE_SIZE;

  const query = useAlbumPage({ listType: "byGenre", genre, page, size: pageSize });
  const albums = query.data ?? [];

  // Hero art comes from the same cached query the genres grid used, so it is
  // free on arrival and stays put as the user pages through.
  const mosaic = useGenreMosaicAlbums(genre);
  const covers = (mosaic.data ?? []).map((album) => album.coverArt);

  // Counts come from the (cached) genre list rather than a dedicated call.
  const genres = useGenres();
  const meta = genres.data?.find((g) => g.value === genre);

  /** Genre playback isn't a listing the server exposes, so pull tracks on demand. */
  async function playGenre(shuffle: boolean) {
    setStarting(true);
    try {
      const page = await cmd("library_get_songs", { genre, size: PLAY_LIMIT, offset: 0 });
      if (page.songs.length > 0) playContext(page.songs, 0, { shuffle });
    } finally {
      setStarting(false);
    }
  }

  function goToPage(next: number) {
    const params2 = new URLSearchParams(params);
    if (next === 0) params2.delete("page");
    else params2.set("page", String(next));
    setParams(params2, { replace: true });
    scrollRef.current?.scrollTo({ top: 0 });
  }

  return (
    <div className="flex h-full flex-col">
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
        {/* Hero: artwork from the genre's first album, blurred into a backdrop. */}
        <div className="relative overflow-hidden">
          <div
            className="absolute inset-0 scale-125 bg-cover bg-center blur-3xl brightness-[0.30] saturate-150"
            style={{ backgroundImage: `url(${artUrl(covers[0], 300)})` }}
          />
          <div className="absolute inset-0 bg-gradient-to-b from-transparent to-bg" />
          <div className="relative flex items-end gap-7 px-8 pt-14 pb-7">
            <IconButton
              label={t("common.back")}
              onClick={() => navigate(-1)}
              className="absolute top-4 left-4 bg-bg/50 backdrop-blur"
            >
              <ArrowLeft className="size-4" />
            </IconButton>
            <GenreMosaic
              covers={covers}
              size={600}
              className="w-44 shrink-0 rounded-md shadow-2xl ring-1 ring-white/10"
            />
            <div className="min-w-0 flex-1 pb-1">
              <div className="text-xs font-semibold tracking-widest text-dim uppercase">
                {t("genre.label")}
              </div>
              <h1 className="mt-1 mb-2 text-5xl leading-tight font-bold break-words">{genre}</h1>
              <div className="text-sm text-dim">
                {meta ? (
                  <>
                    {t("count.albums", { n: meta.albumCount })} ·{" "}
                    {t("count.songs", { n: meta.songCount })}
                  </>
                ) : (
                  t("count.albums", { n: albums.length })
                )}
              </div>
              <div className="mt-4 flex items-center gap-2">
                <Button
                  onClick={() => void playGenre(false)}
                  disabled={starting || albums.length === 0}
                  className="rounded-full"
                >
                  <Play className="size-4 fill-current" /> {t("common.play")}
                </Button>
                <Button
                  variant="outline"
                  className="rounded-full"
                  disabled={starting || albums.length === 0}
                  onClick={() => void playGenre(true)}
                >
                  <Shuffle className="size-4" /> {t("common.shuffle")}
                </Button>
              </div>
            </div>
          </div>
        </div>

        <div className="px-4 pb-4">
          {query.isPending ? (
            <CenteredSpinner />
          ) : query.isError ? (
            <div className="px-2 py-16 text-center text-sm text-dim">{t("genre.loadError")}</div>
          ) : albums.length === 0 ? (
            <div className="px-2 py-16 text-center text-sm text-dim">
              {t("genre.noAlbums", { genre })}
            </div>
          ) : (
            <div className={query.isPlaceholderData ? "opacity-60 transition-opacity" : ""}>
              <AlbumGrid albums={albums} scrollRef={scrollRef} />
            </div>
          )}
        </div>
      </div>
      <PaginationBar
        page={page}
        pageSize={pageSize}
        count={albums.length}
        hasMore={albums.length === pageSize}
        total={meta?.albumCount}
        pageSizeOptions={[30, 60, 120]}
        onPageSize={(n) => {
          const next = new URLSearchParams(params);
          if (n === ALBUM_PAGE_SIZE) next.delete("size");
          else next.set("size", String(n));
          next.delete("page");
          setParams(next, { replace: true });
        }}
        onPage={goToPage}
      />
    </div>
  );
}
