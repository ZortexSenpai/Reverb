// Roon-style landing page: a rotating hero built from recent plays, a
// library stats strip, and carousel shelves. All data comes from the same
// queries the library views use, so everything shares one cache.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import {
  ArrowRight,
  ChevronLeft,
  ChevronRight,
  Disc3,
  ListMusic,
  Music,
  Play,
  RefreshCw,
  Shuffle,
  Tags,
  Users,
  type LucideIcon,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { Link, useNavigate } from "react-router";
import { AlbumCard } from "../../components/library/AlbumCard";
import { CoverImage } from "../../components/library/CoverImage";
import { MixCard } from "../../components/library/MixCard";
import { Button, IconButton } from "../../components/ui/Button";
import { artUrl } from "../../lib/art";
import { useT, type MessageKey, type T } from "../../lib/i18n";
import { cmd, errorMessage } from "../../lib/ipc/commands";
import { genrePath } from "../../lib/subsonic/genres";
import { qk } from "../../lib/subsonic/keys";
import {
  useAlbumPage,
  useArtists,
  useGenres,
  useMixes,
  usePlaylists,
  useSongPage,
} from "../../lib/subsonic/queries";
import type { AlbumListType, Playlist } from "../../lib/subsonic/types";
import { usePlayerStore } from "../../stores/playerStore";
import { GRID_CARD_WIDTH, usePrefsStore } from "../../stores/prefsStore";
import { useUiStore } from "../../stores/uiStore";

const SHELF_SIZE = 20;
const HERO_ROTATE_MS = 8000;

function greeting(t: T): string {
  const hour = new Date().getHours();
  if (hour >= 5 && hour < 12) return t("home.goodMorning");
  if (hour >= 12 && hour < 18) return t("home.goodAfternoon");
  return t("home.goodEvening");
}

function useCardWidth(): number {
  const gridSize = usePrefsStore((s) => s.prefs.gridSize);
  return GRID_CARD_WIDTH[gridSize] ?? 190;
}

// ---- hero ----

function Hero() {
  const t = useT();
  const navigate = useNavigate();
  const { data: auth } = useQuery({
    queryKey: ["auth-status"],
    queryFn: () => cmd("auth_get_status"),
  });
  const recent = useAlbumPage({ listType: "recent", page: 0, size: 8 });
  // Same params as the "Recently added" shelf, so the cache is shared.
  const newest = useAlbumPage({ listType: "newest", page: 0, size: SHELF_SIZE });

  const fromRecent = (recent.data?.length ?? 0) > 0;
  const albums = useMemo(
    () => (fromRecent ? (recent.data ?? []) : (newest.data ?? [])).slice(0, 6),
    [fromRecent, recent.data, newest.data],
  );

  const [index, setIndex] = useState(0);
  const [paused, setPaused] = useState(false);

  useEffect(() => {
    if (paused || albums.length < 2) return;
    const timer = setInterval(() => setIndex((i) => i + 1), HERO_ROTATE_MS);
    return () => clearInterval(timer);
  }, [paused, albums.length]);

  const greetLine = auth?.username
    ? t("home.greetingNamed", { greeting: greeting(t), name: auth.username })
    : greeting(t);

  if (albums.length === 0 && (recent.isPending || newest.isPending)) {
    return <div className="mx-6 mt-4 h-72 animate-pulse rounded-2xl bg-surface" />;
  }
  if (albums.length === 0) {
    return (
      <div className="px-6 pt-6">
        <h1 className="text-2xl font-semibold">{greetLine}</h1>
      </div>
    );
  }

  const current = index % albums.length;
  const album = albums[current];

  async function play(shuffle: boolean) {
    const full = await cmd("library_get_album", { id: album.id });
    usePlayerStore.getState().playContext(full.song, 0, { shuffle });
  }

  return (
    <section
      className="relative mx-6 mt-4 overflow-hidden rounded-2xl bg-surface"
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
    >
      <AnimatePresence initial={false}>
        <motion.img
          key={album.id}
          src={artUrl(album.coverArt, 600)}
          alt=""
          aria-hidden
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.9, ease: "easeInOut" }}
          className="absolute inset-0 size-full scale-125 object-cover blur-3xl brightness-[0.45] saturate-150"
        />
      </AnimatePresence>
      <div className="absolute inset-0 bg-gradient-to-t from-black/60 via-black/10 to-black/30" />

      {albums.length > 1 && (
        <div className="absolute top-4 right-5 z-10 flex gap-1.5">
          {albums.map((a, i) => (
            <button
              key={a.id}
              aria-label={t("home.showAlbum", { name: a.name })}
              onClick={() => setIndex(i)}
              className={`size-2 cursor-pointer rounded-full transition-colors ${
                i === current ? "bg-white" : "bg-white/30 hover:bg-white/60"
              }`}
            />
          ))}
        </div>
      )}

      <motion.div
        key={album.id}
        initial={{ opacity: 0, y: 12 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5 }}
        className="relative flex items-end gap-6 p-6 pt-16"
      >
        <button
          onClick={() => navigate(`/albums/${album.id}`)}
          className="w-44 shrink-0 cursor-pointer transition-transform hover:scale-[1.02]"
          aria-label={t("home.openAlbum", { name: album.name })}
        >
          <CoverImage
            coverArt={album.coverArt}
            size={600}
            alt={album.name}
            className="rounded-lg shadow-2xl"
          />
        </button>
        <div className="min-w-0 flex-1 pb-1">
          <div className="text-[11px] font-semibold tracking-[0.2em] text-white/60 uppercase">
            {greetLine} ·{" "}
            {fromRecent ? t("home.pickUpWhereYouLeftOff") : t("home.newInLibrary")}
          </div>
          <button
            onClick={() => navigate(`/albums/${album.id}`)}
            className="mt-2 block max-w-full cursor-pointer truncate text-left text-4xl font-bold tracking-tight text-white hover:underline"
          >
            {album.name}
          </button>
          <div className="mt-1.5 truncate text-sm text-white/70">
            {album.artist}
            {album.year ? ` · ${album.year}` : ""}
            {album.songCount ? ` · ${t("count.tracks", { n: album.songCount })}` : ""}
          </div>
          <div className="mt-4 flex items-center gap-2">
            <Button onClick={() => void play(false)} className="rounded-full">
              <Play className="size-4 fill-current" /> {t("common.play")}
            </Button>
            <button
              onClick={() => void play(true)}
              className="inline-flex cursor-pointer items-center gap-2 rounded-full border border-white/30 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-white/10"
            >
              <Shuffle className="size-4" /> {t("common.shuffle")}
            </button>
          </div>
        </div>
      </motion.div>
    </section>
  );
}

// ---- library stats ----

function Stat({
  value,
  labelKey,
  to,
  icon: Icon,
}: {
  value: number | undefined;
  labelKey: MessageKey;
  to: string;
  icon: LucideIcon;
}) {
  const t = useT();
  if (!value) return null;
  return (
    <Link
      to={to}
      // Cards share the row: whichever counts the server reports fill it.
      className="flex min-w-40 flex-1 items-center gap-3 rounded-xl border border-edge bg-surface px-4 py-3 transition-colors hover:border-accent/50 hover:bg-surface-2"
    >
      <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-accent/12 text-accent">
        <Icon className="size-4.5" />
      </span>
      <span className="min-w-0">
        {/* Proportional figures: tabular digits read loose at this size. */}
        <span className="block text-xl font-semibold">{t.n(value)}</span>
        <span className="block truncate text-[11px] tracking-wide text-dim uppercase">
          {t(labelKey)}
        </span>
      </span>
    </Link>
  );
}

function LibraryStats() {
  const artists = useArtists();
  const songs = useSongPage({ page: 0, size: 1 });
  const genres = useGenres();
  const playlists = usePlaylists();

  const flat = useMemo(
    () => artists.data?.index.flatMap((g) => g.artist) ?? [],
    [artists.data],
  );
  const albumCount = flat.reduce((n, a) => n + (a.albumCount ?? 0), 0);

  return (
    <div className="mx-6 mt-3 flex flex-wrap gap-3">
      <Stat value={albumCount} labelKey="nav.albums" to="/albums" icon={Disc3} />
      <Stat value={flat.length} labelKey="nav.artists" to="/artists" icon={Users} />
      <Stat value={songs.data?.total ?? undefined} labelKey="nav.tracks" to="/songs" icon={Music} />
      <Stat value={genres.data?.length} labelKey="nav.genres" to="/genres" icon={Tags} />
      <Stat value={playlists.data?.length} labelKey="nav.playlists" to="/playlists" icon={ListMusic} />
    </div>
  );
}

// ---- shelves ----

interface ShelfProps<T> {
  title: string;
  seeAllTo?: string;
  items: T[];
  itemKey: (item: T) => string;
  renderItem: (item: T) => ReactNode;
  cardWidth: number;
  pending?: boolean;
  onRefresh?: () => void;
  refreshing?: boolean;
}

function Shelf<T>({
  title,
  seeAllTo,
  items,
  itemKey,
  renderItem,
  cardWidth,
  pending,
  onRefresh,
  refreshing,
}: ShelfProps<T>) {
  const t = useT();
  const rowRef = useRef<HTMLDivElement>(null);
  const [edges, setEdges] = useState({ left: false, right: false });

  const updateEdges = useCallback(() => {
    const el = rowRef.current;
    if (!el) return;
    setEdges({
      left: el.scrollLeft > 4,
      right: el.scrollLeft + el.clientWidth < el.scrollWidth - 4,
    });
  }, []);

  useEffect(() => {
    updateEdges();
    const el = rowRef.current;
    if (!el) return;
    const ro = new ResizeObserver(updateEdges);
    ro.observe(el);
    return () => ro.disconnect();
  }, [updateEdges, items.length, pending]);

  function nudge(dir: number) {
    const el = rowRef.current;
    el?.scrollBy({ left: dir * el.clientWidth * 0.9, behavior: "smooth" });
  }

  return (
    <section>
      <div className="flex items-center gap-1 px-6 pb-1">
        <h2 className="text-lg font-semibold">{title}</h2>
        {onRefresh && (
          <IconButton label={t("home.newPicks")} onClick={onRefresh} className="-my-1">
            <RefreshCw className={`size-3.5 ${refreshing ? "animate-spin" : ""}`} />
          </IconButton>
        )}
        <span className="ml-auto" />
        {seeAllTo && (
          <Link
            to={seeAllTo}
            className="flex items-center gap-1 text-xs text-dim transition-colors hover:text-text"
          >
            {t("common.seeAll")} <ArrowRight className="size-3.5" />
          </Link>
        )}
        {(edges.left || edges.right) && (
          <div className="flex gap-0.5">
            <IconButton
              label={t("home.scrollLeft")}
              onClick={() => nudge(-1)}
              disabled={!edges.left}
              className="-my-1"
            >
              <ChevronLeft className="size-4" />
            </IconButton>
            <IconButton
              label={t("home.scrollRight")}
              onClick={() => nudge(1)}
              disabled={!edges.right}
              className="-my-1"
            >
              <ChevronRight className="size-4" />
            </IconButton>
          </div>
        )}
      </div>
      {pending ? (
        <div className="flex gap-1 overflow-x-hidden px-4 pb-1">
          {Array.from({ length: 8 }, (_, i) => (
            <div key={i} className="shrink-0 p-3" style={{ width: cardWidth }}>
              <div className="aspect-square animate-pulse rounded-md bg-surface-2" />
              <div className="mt-2 h-4 w-3/4 animate-pulse rounded bg-surface-2" />
            </div>
          ))}
        </div>
      ) : (
        <div
          ref={rowRef}
          onScroll={updateEdges}
          className="no-scrollbar flex gap-1 overflow-x-auto px-4 pb-1"
        >
          {items.map((item) => (
            <div key={itemKey(item)} className="shrink-0" style={{ width: cardWidth }}>
              {renderItem(item)}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

function AlbumShelf({
  title,
  listType,
  refreshable,
}: {
  title: string;
  listType: AlbumListType;
  refreshable?: boolean;
}) {
  const cardWidth = useCardWidth();
  const query = useAlbumPage({ listType, page: 0, size: SHELF_SIZE });
  const albums = query.data ?? [];

  if (query.isError || (!query.isPending && albums.length === 0)) return null;

  return (
    <Shelf
      title={title}
      seeAllTo={`/albums?sort=${listType}`}
      items={albums}
      itemKey={(album) => album.id}
      renderItem={(album) => <AlbumCard album={album} />}
      cardWidth={cardWidth}
      pending={query.isPending}
      onRefresh={refreshable ? () => void query.refetch() : undefined}
      refreshing={query.isFetching}
    />
  );
}

// ---- made for you ----

function MixShelf() {
  const t = useT();
  const cardWidth = useCardWidth();
  const queryClient = useQueryClient();
  const query = useMixes();
  const [refreshing, setRefreshing] = useState(false);
  const mixes = query.data ?? [];

  async function refresh() {
    setRefreshing(true);
    try {
      queryClient.setQueryData(qk.mixes(), await cmd("mixes_get", { refresh: true }));
    } catch (e) {
      useUiStore.getState().showToast(errorMessage(e), "error");
    } finally {
      setRefreshing(false);
    }
  }

  // Nothing to build mixes from yet (no plays, stars or ratings): no shelf.
  if (query.isError || (!query.isPending && mixes.length === 0)) return null;

  return (
    <Shelf
      title={t("home.madeForYou")}
      items={mixes}
      itemKey={(mix) => mix.id}
      renderItem={(mix) => <MixCard mix={mix} />}
      cardWidth={cardWidth}
      pending={query.isPending}
      onRefresh={() => void refresh()}
      refreshing={refreshing}
    />
  );
}

// ---- genres ----

function genreHue(name: string): number {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return h % 360;
}

function GenreShelf() {
  const t = useT();
  const { data: genres } = useGenres();
  const items = useMemo(
    () =>
      [...(genres ?? [])]
        .filter((g) => g.albumCount > 0)
        .sort((a, b) => b.albumCount - a.albumCount)
        .slice(0, 12),
    [genres],
  );

  if (items.length === 0) return null;

  return (
    <Shelf
      title={t("nav.genres")}
      seeAllTo="/genres"
      items={items}
      itemKey={(g) => g.value}
      renderItem={(g) => {
        const hue = genreHue(g.value);
        return (
          <Link
            to={genrePath(g.value)}
            className="m-3 flex h-24 flex-col justify-between rounded-xl p-3 transition-transform hover:scale-[1.03]"
            style={{
              background: `linear-gradient(135deg, oklch(0.45 0.1 ${hue}) 0%, oklch(0.28 0.08 ${(hue + 60) % 360}) 100%)`,
            }}
          >
            <span className="truncate text-sm font-semibold text-white">{g.value}</span>
            <span className="text-xs text-white/60">{t("count.albums", { n: g.albumCount })}</span>
          </Link>
        );
      }}
      cardWidth={200}
    />
  );
}

// ---- playlists ----

function PlaylistCard({ playlist }: { playlist: Playlist }) {
  const t = useT();
  const navigate = useNavigate();
  return (
    <div
      className="group w-full cursor-pointer rounded-lg p-3 transition-colors hover:bg-surface"
      onClick={() => navigate(`/playlists/${playlist.id}`)}
    >
      <CoverImage
        coverArt={playlist.coverArt}
        size={300}
        alt={playlist.name}
        className="rounded-md"
      />
      <div className="mt-2 truncate text-sm font-medium">{playlist.name}</div>
      <div className="truncate text-xs text-dim">
        {t("count.tracks", { n: playlist.songCount })}
      </div>
    </div>
  );
}

function PlaylistShelf() {
  const t = useT();
  const cardWidth = useCardWidth();
  const { data: playlists } = usePlaylists();

  // Most recently changed first, so active playlists surface.
  const items = useMemo(
    () =>
      [...(playlists ?? [])]
        .sort((a, b) =>
          (b.changed ?? b.created ?? "").localeCompare(a.changed ?? a.created ?? ""),
        )
        .slice(0, SHELF_SIZE),
    [playlists],
  );

  if (items.length === 0) return null;

  return (
    <Shelf
      title={t("nav.playlists")}
      seeAllTo="/playlists"
      items={items}
      itemKey={(playlist) => playlist.id}
      renderItem={(playlist) => <PlaylistCard playlist={playlist} />}
      cardWidth={cardWidth}
    />
  );
}

// ---- page ----

export function HomeView() {
  const t = useT();
  return (
    <div className="h-full overflow-y-auto">
      <Hero />
      <LibraryStats />
      <div className="flex flex-col gap-5 pt-4 pb-8">
        <MixShelf />
        <AlbumShelf title={t("home.jumpBackIn")} listType="recent" />
        <AlbumShelf title={t("home.recentlyAdded")} listType="newest" />
        <AlbumShelf title={t("home.mostPlayed")} listType="frequent" />
        <GenreShelf />
        <PlaylistShelf />
        <AlbumShelf title={t("home.surpriseMe")} listType="random" refreshable />
      </div>
    </div>
  );
}
