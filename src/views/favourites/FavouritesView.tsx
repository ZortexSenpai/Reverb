import { Tabs } from "radix-ui";
import { Heart, Play, Shuffle } from "lucide-react";
import { useMemo, useRef } from "react";
import { useSearchParams } from "react-router";
import { AlbumCard } from "../../components/library/AlbumCard";
import { ArtistCard } from "../../components/library/ArtistCard";
import { CardGrid } from "../../components/library/CardGrid";
import { FilterInput } from "../../components/library/FilterInput";
import { PaginationBar } from "../../components/library/PaginationBar";
import {
  ColumnsMenu,
  SongTableBody,
  SongTableHeaderRow,
} from "../../components/library/SongTable";
import {
  activeColumns,
  compareSongsBy,
  songGridTemplate,
} from "../../components/library/songColumns";
import { Button } from "../../components/ui/Button";
import { CenteredSpinner } from "../../components/ui/Spinner";
import { useT, type MessageKey } from "../../lib/i18n";
import { useStarred } from "../../lib/subsonic/queries";
import type { Album, Artist } from "../../lib/subsonic/types";
import { usePlayerStore } from "../../stores/playerStore";
import { GRID_CARD_WIDTH, usePrefsStore } from "../../stores/prefsStore";

const tabClass =
  "cursor-pointer rounded-full px-4 py-1.5 text-sm text-dim transition-colors hover:text-text data-[state=active]:bg-surface-2 data-[state=active]:font-medium data-[state=active]:text-text";
const selectClass =
  "cursor-pointer rounded-md border border-edge bg-surface px-2.5 py-1.5 text-sm text-text focus:border-accent focus:outline-none";

const PAGE_SIZES: Record<string, { default: number; options: number[] }> = {
  songs: { default: 100, options: [50, 100, 250] },
  albums: { default: 60, options: [30, 60, 120] },
  artists: { default: 96, options: [48, 96, 192] },
};

const isoDesc = (a?: string, b?: string) => (b ?? "").localeCompare(a ?? "");

/** `needle` is expected pre-trimmed and lower-cased. */
const matches = (needle: string, ...fields: (string | undefined)[]) =>
  fields.some((field) => field?.toLowerCase().includes(needle));

type AlbumSort = "starred" | "name" | "name-desc" | "artist" | "year-desc" | "year-asc";

const ALBUM_SORTS: { value: AlbumSort; labelKey: MessageKey }[] = [
  { value: "starred", labelKey: "sort.recentlyStarred" },
  { value: "name", labelKey: "sort.nameAsc" },
  { value: "name-desc", labelKey: "sort.nameDesc" },
  { value: "artist", labelKey: "sort.artist" },
  { value: "year-desc", labelKey: "sort.yearNewest" },
  { value: "year-asc", labelKey: "sort.yearOldest" },
];

function compareAlbums(a: Album, b: Album, sort: AlbumSort): number {
  switch (sort) {
    case "starred":
      return isoDesc(a.starred, b.starred);
    case "name":
      return a.name.localeCompare(b.name);
    case "name-desc":
      return b.name.localeCompare(a.name);
    case "artist":
      return (a.artist ?? "").localeCompare(b.artist ?? "") || a.name.localeCompare(b.name);
    case "year-desc":
      return (b.year ?? 0) - (a.year ?? 0);
    case "year-asc":
      return (a.year ?? 0) - (b.year ?? 0);
  }
}

type ArtistSort = "starred" | "name" | "name-desc" | "albums" | "albums-asc";

const ARTIST_SORTS: { value: ArtistSort; labelKey: MessageKey }[] = [
  { value: "starred", labelKey: "sort.recentlyStarred" },
  { value: "name", labelKey: "sort.nameAsc" },
  { value: "name-desc", labelKey: "sort.nameDesc" },
  { value: "albums", labelKey: "sort.mostAlbums" },
  { value: "albums-asc", labelKey: "sort.fewestAlbums" },
];

function compareArtists(a: Artist, b: Artist, sort: ArtistSort): number {
  switch (sort) {
    case "starred":
      return isoDesc(a.starred, b.starred);
    case "name":
      return a.name.localeCompare(b.name);
    case "name-desc":
      return b.name.localeCompare(a.name);
    case "albums":
      return (b.albumCount ?? 0) - (a.albumCount ?? 0) || a.name.localeCompare(b.name);
    case "albums-asc":
      return (a.albumCount ?? 0) - (b.albumCount ?? 0) || a.name.localeCompare(b.name);
  }
}

export function FavouritesView() {
  const t = useT();
  const [params, setParams] = useSearchParams();
  const scrollRef = useRef<HTMLDivElement>(null);
  const { data: starred, isPending, isError } = useStarred();
  const playContext = usePlayerStore((s) => s.playContext);
  const songColumns = usePrefsStore((s) => s.prefs.songColumns);
  const gridSize = usePrefsStore((s) => s.prefs.gridSize);

  const tab = params.get("tab") ?? "songs";
  const q = params.get("q") ?? "";
  const needle = q.trim().toLowerCase();
  const sort = params.get("sort") ?? "";
  const sortDesc = params.get("dir") === "desc";
  const page = Math.max(0, Number(params.get("page") ?? "0") || 0);
  const sizes = PAGE_SIZES[tab] ?? PAGE_SIZES.songs;
  const pageSize = Number(params.get("size") ?? "") || sizes.default;

  const columns = useMemo(() => activeColumns(songColumns), [songColumns]);
  const gridTemplate = useMemo(() => songGridTemplate(columns), [columns]);

  // getStarred2 returns the complete starred set, so filtering and sorting
  // here cover ALL favourites; pagination is a local slice.
  const sortedSongs = useMemo(() => {
    const raw = starred?.song ?? [];
    const hits = needle
      ? raw.filter((song) => matches(needle, song.title, song.album, song.artist))
      : raw;
    const compare = sort ? compareSongsBy(sort) : null;
    if (!compare) return hits;
    const sorted = [...hits].sort(compare);
    return sortDesc ? sorted.reverse() : sorted;
  }, [starred?.song, sort, sortDesc, needle]);

  const sortedAlbums = useMemo(() => {
    const raw = starred?.album ?? [];
    const hits = needle ? raw.filter((album) => matches(needle, album.name, album.artist)) : raw;
    const mode = (sort || "starred") as AlbumSort;
    return [...hits].sort((a, b) => compareAlbums(a, b, mode));
  }, [starred?.album, sort, needle]);

  const sortedArtists = useMemo(() => {
    const raw = starred?.artist ?? [];
    const hits = needle ? raw.filter((artist) => matches(needle, artist.name)) : raw;
    const mode = (sort || "name") as ArtistSort;
    return [...hits].sort((a, b) => compareArtists(a, b, mode));
  }, [starred?.artist, sort, needle]);

  function update(patch: Record<string, string>, resetPage = true) {
    const next = new URLSearchParams(params);
    for (const [key, value] of Object.entries(patch)) {
      if (value === "") next.delete(key);
      else next.set(key, value);
    }
    if (resetPage) next.delete("page");
    setParams(next, { replace: true });
    if (resetPage) scrollRef.current?.scrollTo({ top: 0 });
  }

  // The filter survives a tab switch: the tab counts show where the matches
  // are, so switching to them is the point.
  function switchTab(nextTab: string) {
    update({ tab: nextTab === "songs" ? "" : nextTab, sort: "", dir: "", size: "" });
  }

  function cycleSongSort(id: string) {
    if (sort !== id) update({ sort: id, dir: "" }, false);
    else if (!sortDesc) update({ dir: "desc" }, false);
    else update({ sort: "", dir: "" }, false);
  }

  function paginate<T>(items: T[]): { pageItems: T[]; safePage: number; total: number } {
    const total = items.length;
    const pageCount = Math.max(1, Math.ceil(total / pageSize));
    const safePage = Math.min(page, pageCount - 1);
    return { pageItems: items.slice(safePage * pageSize, (safePage + 1) * pageSize), safePage, total };
  }

  if (isPending) return <CenteredSpinner />;
  if (isError) return <div className="p-8 text-sm text-dim">{t("favourites.loadError")}</div>;

  const empty =
    starred.song.length === 0 && starred.album.length === 0 && starred.artist.length === 0;

  // Empty-state copy per tab, spelled out rather than composed from a noun:
  // "No {kind} match" does not translate as one sentence into every language.
  const emptyKeys: { none: MessageKey; noMatch: MessageKey } =
    tab === "albums"
      ? { none: "favourites.noAlbums", noMatch: "favourites.noAlbumsMatch" }
      : tab === "artists"
        ? { none: "favourites.noArtists", noMatch: "favourites.noArtistsMatch" }
        : { none: "favourites.noSongs", noMatch: "favourites.noSongsMatch" };
  const current =
    tab === "albums"
      ? paginate(sortedAlbums)
      : tab === "artists"
        ? paginate(sortedArtists)
        : paginate(sortedSongs);

  return (
    <Tabs.Root value={tab} onValueChange={switchTab} className="flex h-full flex-col">
      {/* pr matches the scrollbar gutter reserved in the scroll area below,
          keeping the column header aligned with the rows. */}
      <div className="pr-2.5">
        <div className="flex flex-wrap items-center gap-3 px-6 pt-4 pb-3">
          <h1 className="text-2xl font-semibold">{t("nav.favourites")}</h1>
          {/* Counts follow the filter, so they double as a hint of which tab
              holds the matches. */}
          <Tabs.List className="flex gap-1">
            <Tabs.Trigger value="songs" className={tabClass}>
              {t("nav.songs")} ({t.n(sortedSongs.length)})
            </Tabs.Trigger>
            <Tabs.Trigger value="albums" className={tabClass}>
              {t("nav.albums")} ({t.n(sortedAlbums.length)})
            </Tabs.Trigger>
            <Tabs.Trigger value="artists" className={tabClass}>
              {t("nav.artists")} ({t.n(sortedArtists.length)})
            </Tabs.Trigger>
          </Tabs.List>
          {tab === "songs" && starred.song.length > 0 && (
            <>
              <Button
                onClick={() => playContext(sortedSongs, 0)}
                disabled={sortedSongs.length === 0}
                className="rounded-full"
              >
                <Play className="size-4 fill-current" /> {t("common.playAll")}
              </Button>
              <Button
                variant="outline"
                className="rounded-full"
                disabled={sortedSongs.length === 0}
                onClick={() => playContext(sortedSongs, 0, { shuffle: true })}
              >
                <Shuffle className="size-4" /> {t("common.shuffle")}
              </Button>
              <ColumnsMenu />
            </>
          )}
          {tab === "albums" && (
            <select
              value={sort || "starred"}
              onChange={(e) => update({ sort: e.target.value === "starred" ? "" : e.target.value })}
              className={selectClass}
            >
              {ALBUM_SORTS.map((s) => (
                <option key={s.value} value={s.value}>
                  {t(s.labelKey)}
                </option>
              ))}
            </select>
          )}
          {tab === "artists" && (
            <select
              value={sort || "name"}
              onChange={(e) => update({ sort: e.target.value === "name" ? "" : e.target.value })}
              className={selectClass}
            >
              {ARTIST_SORTS.map((s) => (
                <option key={s.value} value={s.value}>
                  {t(s.labelKey)}
                </option>
              ))}
            </select>
          )}
          {!empty && (
            <FilterInput
              value={q}
              onChange={(value) => update({ q: value })}
              placeholder={t("favourites.search")}
              className="ml-auto w-56"
            />
          )}
        </div>
        {tab === "songs" && starred.song.length > 0 && (
          <SongTableHeaderRow
            columns={columns}
            gridTemplate={gridTemplate}
            sortId={sort}
            sortDesc={sortDesc}
            onSort={cycleSongSort}
          />
        )}
      </div>
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]">
        {empty ? (
          <div className="flex flex-col items-center gap-3 py-24 text-dim">
            <Heart className="size-10 text-faint" />
            <p className="text-sm">{t("favourites.empty")}</p>
          </div>
        ) : current.total === 0 ? (
          <div className="py-24 text-center text-sm text-dim">
            {needle === "" ? t(emptyKeys.none) : t(emptyKeys.noMatch, { q: q.trim() })}
          </div>
        ) : (
          <>
            <Tabs.Content value="songs">
              <div className="pt-1 pb-4">
                <SongTableBody
                  songs={current.pageItems as typeof sortedSongs}
                  columns={columns}
                  gridTemplate={gridTemplate}
                  scrollRef={scrollRef}
                />
              </div>
            </Tabs.Content>
            <Tabs.Content value="albums">
              <div className="px-4 pb-4">
                <CardGrid
                  items={current.pageItems as typeof sortedAlbums}
                  itemKey={(album) => album.id}
                  renderItem={(album) => <AlbumCard album={album} />}
                  scrollRef={scrollRef}
                  cardMinWidth={GRID_CARD_WIDTH[gridSize] ?? 190}
                />
              </div>
            </Tabs.Content>
            <Tabs.Content value="artists">
              <div className="px-4 pb-4">
                <CardGrid
                  items={current.pageItems as typeof sortedArtists}
                  itemKey={(artist) => artist.id}
                  renderItem={(artist) => <ArtistCard artist={artist} />}
                  scrollRef={scrollRef}
                  rowExtra={76}
                  cardMinWidth={GRID_CARD_WIDTH[gridSize] ?? 190}
                />
              </div>
            </Tabs.Content>
          </>
        )}
      </div>

      {!empty && (
        <PaginationBar
          page={current.safePage}
          pageSize={pageSize}
          count={current.pageItems.length}
          hasMore={(current.safePage + 1) * pageSize < current.total}
          total={current.total}
          pageSizeOptions={sizes.options}
          onPageSize={(n) => update({ size: n === sizes.default ? "" : String(n) })}
          onPage={(p) => update({ page: String(p) }, false)}
        />
      )}
    </Tabs.Root>
  );
}
