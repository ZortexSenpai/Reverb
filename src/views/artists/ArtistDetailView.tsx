import { useQuery } from "@tanstack/react-query";
import { ArrowLeft, ExternalLink, MoreVertical, Play, Shuffle } from "lucide-react";
import { useState } from "react";
import { useNavigate, useParams } from "react-router";
import { AlbumCard } from "../../components/library/AlbumCard";
import { ArtistCard } from "../../components/library/ArtistCard";
import { CoverImage } from "../../components/library/CoverImage";
import { EntityActionsMenu } from "../../components/library/EntityContextMenu";
import { commonGenres, GenreBubbles } from "../../components/library/GenreBubbles";
import { StarButton } from "../../components/library/StarButton";
import { TrackList } from "../../components/library/TrackList";
import { Button, IconButton } from "../../components/ui/Button";
import { CenteredSpinner } from "../../components/ui/Spinner";
import { artUrl } from "../../lib/art";
import { useT } from "../../lib/i18n";
import { cmd, errorMessage } from "../../lib/ipc/commands";
import { useArtist, useArtistAppearances } from "../../lib/subsonic/queries";
import { qk } from "../../lib/subsonic/keys";
import { usePlayerStore } from "../../stores/playerStore";
import { useUiStore } from "../../stores/uiStore";

function Biography({ text, lastFmUrl }: { text: string; lastFmUrl?: string | null }) {
  const t = useT();
  const [expanded, setExpanded] = useState(false);
  const isLong = text.length > 420;
  return (
    <section className="max-w-3xl px-6 pt-5">
      <p
        className={`text-sm leading-relaxed whitespace-pre-line text-dim ${
          expanded || !isLong ? "" : "line-clamp-3"
        }`}
      >
        {text}
      </p>
      <div className="mt-1.5 flex items-center gap-3">
        {isLong && (
          <button
            onClick={() => setExpanded(!expanded)}
            className="cursor-pointer text-xs font-medium text-accent hover:underline"
          >
            {expanded ? t("common.showLess") : t("common.showMore")}
          </button>
        )}
        {lastFmUrl && (
          <a
            href={lastFmUrl}
            target="_blank"
            rel="noreferrer"
            className="flex items-center gap-1 text-xs text-faint hover:text-dim hover:underline"
          >
            Last.fm <ExternalLink className="size-3" />
          </a>
        )}
      </div>
    </section>
  );
}

export function ArtistDetailView() {
  const t = useT();
  const { artistId = "" } = useParams();
  const navigate = useNavigate();
  const { data: artist, isPending, isError } = useArtist(artistId);
  const appearances = useArtistAppearances(artistId);
  const playContext = usePlayerStore((s) => s.playContext);
  const showToast = useUiStore((s) => s.showToast);
  const [starting, setStarting] = useState(false);

  const info = useQuery({
    queryKey: ["artist-info", artistId],
    queryFn: () => cmd("library_get_artist_info", { id: artistId, name: artist!.name }),
    enabled: Boolean(artist?.name),
    staleTime: 30 * 60_000,
  });

  const topSongs = useQuery({
    queryKey: qk.topSongs(artist?.name ?? ""),
    queryFn: () => cmd("library_get_top_songs", { artistName: artist!.name, count: 10 }),
    enabled: Boolean(artist?.name),
    staleTime: 10 * 60_000,
  });
  const [allTopSongs, setAllTopSongs] = useState(false);

  /**
   * The hero buttons cover the whole discography, not the top-songs list
   * rendered below them, so the tracks are pulled on demand (one request per
   * album). Shuffling starts somewhere random rather than always leading with
   * the first album's opener.
   */
  async function playArtist(shuffle: boolean) {
    setStarting(true);
    try {
      const all = await cmd("library_get_artist_songs", { id: artistId });
      if (all.length === 0) {
        showToast(t("toast.nothingToPlay"));
        return;
      }
      // Play leaves the shuffle toggle as the user set it (playContext honours
      // it), matching the album header.
      if (shuffle) playContext(all, Math.floor(Math.random() * all.length), { shuffle: true });
      else playContext(all, 0);
    } catch (e) {
      showToast(errorMessage(e), "error");
    } finally {
      setStarting(false);
    }
  }

  if (isPending) return <CenteredSpinner />;
  if (isError || !artist) return <div className="p-8 text-sm text-dim">{t("artist.notFound")}</div>;

  const songs = topSongs.data ?? [];
  const shownSongs = allTopSongs ? songs : songs.slice(0, 5);
  const totalSongs = artist.album.reduce((n, a) => n + a.songCount, 0);
  const appearsOn = appearances.data ?? [];
  const similar = info.data?.similarArtists ?? [];

  return (
    <div className="h-full overflow-y-auto">
      {/* Hero */}
      <div className="relative overflow-hidden">
        <div
          className="absolute inset-0 scale-125 bg-cover bg-center blur-3xl brightness-[0.30] saturate-150"
          style={{ backgroundImage: `url(${artUrl(artist.coverArt, 300)})` }}
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
          <CoverImage
            coverArt={artist.coverArt}
            size={600}
            alt={artist.name}
            className="w-44 shrink-0 rounded-full shadow-2xl ring-1 ring-white/10"
          />
          <div className="min-w-0 flex-1 pb-1">
            <div className="text-xs font-semibold tracking-widest text-dim uppercase">
              {t("artist.label")}
            </div>
            <h1 className="mt-1 mb-2 truncate text-5xl leading-tight font-bold">
              {artist.name}
            </h1>
            <div className="text-sm text-dim">
              {t("count.albums", { n: artist.album.length })}
              {totalSongs > 0 && <> · {t("count.songs", { n: totalSongs })}</>}
            </div>
            {/* Artists carry no genres of their own — these come from their
                releases, most common first. */}
            <GenreBubbles names={commonGenres(artist.album)} className="mt-2.5" />
            <div className="mt-4 flex items-center gap-2">
              {artist.album.length > 0 && (
                <>
                  <Button
                    className="rounded-full"
                    disabled={starting}
                    onClick={() => void playArtist(false)}
                  >
                    <Play className="size-4 fill-current" /> {t("common.play")}
                  </Button>
                  <Button
                    variant="outline"
                    className="rounded-full"
                    disabled={starting}
                    onClick={() => void playArtist(true)}
                  >
                    <Shuffle className="size-4" /> {t("common.shuffle")}
                  </Button>
                </>
              )}
              <StarButton id={artist.id} kind="artist" starred={artist.starred} />
              <EntityActionsMenu
                entity={{
                  kind: "artist",
                  id: artist.id,
                  name: artist.name,
                  starred: artist.starred,
                }}
                trigger={
                  <button
                    aria-label={t("artist.options")}
                    className="cursor-pointer rounded-full p-2 text-dim transition-colors hover:bg-surface-2 hover:text-text"
                  >
                    <MoreVertical className="size-4" />
                  </button>
                }
              />
            </div>
          </div>
        </div>
      </div>

      {/* Biography */}
      {info.data?.biography && (
        <Biography text={info.data.biography} lastFmUrl={info.data.lastFmUrl} />
      )}

      {/* Top songs */}
      {songs.length > 0 && (
        <section className="px-4 pt-6">
          <h2 className="px-2 pb-2 text-lg font-semibold">{t("artist.topSongs")}</h2>
          <TrackList songs={shownSongs} showArt />
          {songs.length > 5 && (
            <button
              onClick={() => setAllTopSongs(!allTopSongs)}
              className="mx-2 mt-1 cursor-pointer rounded-md px-2.5 py-1.5 text-xs font-medium text-dim transition-colors hover:bg-surface hover:text-text"
            >
              {allTopSongs ? t("artist.showFewer") : t("artist.showAll", { n: songs.length })}
            </button>
          )}
        </section>
      )}

      {/* Albums */}
      <section className="px-4 pt-6">
        <h2 className="px-2 pb-2 text-lg font-semibold">{t("nav.albums")}</h2>
        <div className="grid grid-cols-[repeat(auto-fill,minmax(190px,1fr))] gap-1">
          {artist.album.map((album) => (
            <AlbumCard key={album.id} album={album} />
          ))}
        </div>
      </section>

      {/* Albums by others with a track crediting this artist */}
      {appearsOn.length > 0 && (
        <section className="px-4 pt-6">
          <h2 className="px-2 pb-2 text-lg font-semibold">{t("artist.appearsOn")}</h2>
          <div className="grid grid-cols-[repeat(auto-fill,minmax(190px,1fr))] gap-1">
            {appearsOn.map((album) => (
              <AlbumCard key={album.id} album={album} />
            ))}
          </div>
        </section>
      )}

      {/* Similar artists (only entries that exist in the library) */}
      {similar.length > 0 && (
        <section className="px-4 pt-6 pb-8">
          <h2 className="px-2 pb-2 text-lg font-semibold">{t("artist.similar")}</h2>
          <div className="grid grid-cols-[repeat(auto-fill,minmax(160px,1fr))] gap-1">
            {similar.slice(0, 8).map((entry) => (
              <ArtistCard key={entry.id} artist={entry} />
            ))}
          </div>
        </section>
      )}
      {similar.length === 0 && <div className="pb-8" />}
    </div>
  );
}
