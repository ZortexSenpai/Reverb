import { FilterX } from "lucide-react";
import { useRef } from "react";
import { useSearchParams } from "react-router";
import { AlbumGrid } from "../../components/library/AlbumGrid";
import { FilterInput } from "../../components/library/FilterInput";
import { PaginationBar } from "../../components/library/PaginationBar";
import { IconButton } from "../../components/ui/Button";
import { CenteredSpinner } from "../../components/ui/Spinner";
import { useT, type MessageKey } from "../../lib/i18n";
import { ALBUM_PAGE_SIZE, useAlbumPage, useGenres } from "../../lib/subsonic/queries";
import type { AlbumListType } from "../../lib/subsonic/types";
import { useDebounced } from "../../lib/useDebounced";

const SORTS: { value: AlbumListType; labelKey: MessageKey }[] = [
  { value: "newest", labelKey: "sort.recentlyAdded" },
  { value: "recent", labelKey: "sort.recentlyPlayed" },
  { value: "frequent", labelKey: "sort.mostPlayed" },
  { value: "alphabeticalByName", labelKey: "sort.name" },
  { value: "alphabeticalByArtist", labelKey: "sort.artist" },
  { value: "starred", labelKey: "sort.favourites" },
  { value: "random", labelKey: "sort.random" },
];

const selectClass =
  "cursor-pointer rounded-md border border-edge bg-surface px-2.5 py-1.5 text-sm text-text focus:border-accent focus:outline-none disabled:cursor-not-allowed disabled:opacity-40";

export function AlbumsView() {
  const t = useT();
  const [params, setParams] = useSearchParams();
  const scrollRef = useRef<HTMLDivElement>(null);

  const q = params.get("q") ?? "";
  const sort = (params.get("sort") as AlbumListType) ?? "newest";
  const genre = params.get("genre") ?? "";
  const page = Math.max(0, Number(params.get("page") ?? "0") || 0);
  const pageSize = Number(params.get("size") ?? "") || ALBUM_PAGE_SIZE;

  const debouncedQ = useDebounced(q.trim(), 300);
  const genres = useGenres();

  // The Subsonic API makes genre its own list type, so an active genre
  // filter overrides the sort mode (server decides ordering within it).
  const listType: AlbumListType = genre ? "byGenre" : sort;
  const filtersActive = genre !== "" || q !== "";

  const query = useAlbumPage({
    listType,
    // A text filter is a whole-library search: it replaces genre and ordering.
    query: debouncedQ !== "" ? debouncedQ : undefined,
    genre: debouncedQ === "" && genre !== "" ? genre : undefined,
    page,
    size: pageSize,
  });
  const albums = query.data ?? [];

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

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-wrap items-center gap-2 px-6 py-4">
        <h1 className="mr-2 text-2xl font-semibold">{t("nav.albums")}</h1>
        <select
          value={sort}
          disabled={filtersActive}
          title={filtersActive ? t("albums.sortLocked") : t("albums.sort")}
          onChange={(e) => update({ sort: e.target.value })}
          className={selectClass}
        >
          {SORTS.map((s) => (
            <option key={s.value} value={s.value}>
              {t(s.labelKey)}
            </option>
          ))}
        </select>
        <select
          value={genre}
          disabled={q !== ""}
          title={q !== "" ? t("albums.textBeatsGenre") : t("albums.genre")}
          onChange={(e) => update({ genre: e.target.value, q: "" })}
          className={selectClass}
        >
          <option value="">{t("albums.allGenres")}</option>
          {(genres.data ?? []).map((g) => (
            <option key={g.value} value={g.value}>
              {g.value} ({g.albumCount})
            </option>
          ))}
        </select>
        {filtersActive && (
          <IconButton label={t("albums.clearFilters")} onClick={() => update({ genre: "", q: "" })}>
            <FilterX className="size-4" />
          </IconButton>
        )}
        <FilterInput
          value={q}
          onChange={(value) => update({ q: value, genre: "" })}
          placeholder={t("albums.filter")}
          className="ml-auto w-56"
        />
      </div>
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
        <div className="px-4 pb-4">
          {query.isPending ? (
            <CenteredSpinner />
          ) : query.isError ? (
            <div className="px-2 py-16 text-center text-sm text-dim">{t("albums.loadError")}</div>
          ) : albums.length === 0 ? (
            <div className="px-2 py-16 text-center text-sm text-dim">{t("albums.noMatch")}</div>
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
        pageSizeOptions={[30, 60, 120]}
        onPageSize={(n) => update({ size: n === ALBUM_PAGE_SIZE ? "" : String(n) })}
        onPage={(p) => {
          update({ page: String(p) }, false);
          scrollRef.current?.scrollTo({ top: 0 });
        }}
      />
    </div>
  );
}
