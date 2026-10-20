import { Play, Shuffle } from "lucide-react";
import { useMemo, useRef } from "react";
import { useSearchParams } from "react-router";
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
  serverSortFor,
  songGridTemplate,
} from "../../components/library/songColumns";
import { Button } from "../../components/ui/Button";
import { CenteredSpinner } from "../../components/ui/Spinner";
import { useT } from "../../lib/i18n";
import { SONG_PAGE_SIZE, useGenres, useSongPage } from "../../lib/subsonic/queries";
import { useDebounced } from "../../lib/useDebounced";
import { usePlayerStore } from "../../stores/playerStore";
import { usePrefsStore } from "../../stores/prefsStore";

const selectClass =
  "cursor-pointer rounded-md border border-edge bg-surface px-2.5 py-1.5 text-sm text-text focus:border-accent focus:outline-none";

export function SongsView() {
  const t = useT();
  const [params, setParams] = useSearchParams();
  const scrollRef = useRef<HTMLDivElement>(null);
  const playContext = usePlayerStore((s) => s.playContext);
  const songColumns = usePrefsStore((s) => s.prefs.songColumns);

  const q = params.get("q") ?? "";
  const genre = params.get("genre") ?? "";
  const sortId = params.get("sort") ?? "";
  const sortDesc = params.get("dir") === "desc";
  const page = Math.max(0, Number(params.get("page") ?? "0") || 0);
  const pageSize = Number(params.get("size") ?? "") || SONG_PAGE_SIZE;

  const debouncedQ = useDebounced(q.trim(), 300);
  const genres = useGenres();

  const columns = useMemo(() => activeColumns(songColumns), [songColumns]);
  const gridTemplate = useMemo(() => songGridTemplate(columns), [columns]);

  // Sorting is server-side across the whole library (Navidrome native API);
  // columns without a server field, or non-Navidrome servers, fall back to
  // client-side sorting within the page.
  const serverSort = sortId ? serverSortFor(sortId) : undefined;

  const query = useSongPage({
    query: debouncedQ !== "" ? debouncedQ : undefined,
    genre: debouncedQ === "" && genre !== "" ? genre : undefined,
    sort: serverSort,
    descending: serverSort ? sortDesc : undefined,
    page,
    size: pageSize,
  });

  const songs = useMemo(() => {
    const raw = query.data?.songs ?? [];
    const sortedByServer = serverSort != null && (query.data?.serverSorted ?? false);
    if (!sortId || sortedByServer) return raw;
    const compare = compareSongsBy(sortId);
    if (!compare) return raw;
    const sorted = [...raw].sort(compare);
    return sortDesc ? sorted.reverse() : sorted;
  }, [query.data, sortId, sortDesc, serverSort]);

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

  /** Header click: ascending → descending → server order. */
  function cycleSort(id: string) {
    if (sortId !== id) update({ sort: id, dir: "" }, false);
    else if (!sortDesc) update({ dir: "desc" }, false);
    else update({ sort: "", dir: "" }, false);
  }

  return (
    <div className="flex h-full flex-col">
      {/* pr matches the scrollbar gutter reserved in the scroll area below,
          keeping the column header aligned with the rows. */}
      <div className="pr-2.5">
        <div className="flex flex-wrap items-center gap-2 px-6 pt-4 pb-3">
          <h1 className="mr-2 text-2xl font-semibold">{t("nav.songs")}</h1>
          <Button
            onClick={() => playContext(songs, 0)}
            disabled={songs.length === 0}
            className="rounded-full"
          >
            <Play className="size-4 fill-current" /> {t("common.play")}
          </Button>
          <Button
            variant="outline"
            className="rounded-full"
            disabled={songs.length === 0}
            onClick={() => playContext(songs, 0, { shuffle: true })}
          >
            <Shuffle className="size-4" /> {t("common.shuffle")}
          </Button>
          <select
            value={genre}
            disabled={q !== ""}
            title={q !== "" ? t("albums.textBeatsGenre") : t("albums.genre")}
            onChange={(e) => update({ genre: e.target.value, q: "" })}
            className={`${selectClass} disabled:cursor-not-allowed disabled:opacity-40`}
          >
            <option value="">{t("albums.allGenres")}</option>
            {(genres.data ?? []).map((g) => (
              <option key={g.value} value={g.value}>
                {g.value} ({g.songCount})
              </option>
            ))}
          </select>
          <ColumnsMenu />
          <FilterInput
            value={q}
            onChange={(value) => update({ q: value, genre: "" })}
            placeholder={t("songs.filter")}
            className="ml-auto w-56"
          />
        </div>
        <SongTableHeaderRow
          columns={columns}
          gridTemplate={gridTemplate}
          sortId={sortId}
          sortDesc={sortDesc}
          onSort={cycleSort}
        />
      </div>
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]">
        {query.isPending ? (
          <CenteredSpinner />
        ) : query.isError ? (
          <div className="p-8 text-center text-sm text-dim">{t("songs.loadError")}</div>
        ) : songs.length === 0 ? (
          <div className="p-8 py-16 text-center text-sm text-dim">{t("songs.noMatch")}</div>
        ) : (
          <SongTableBody
            songs={songs}
            columns={columns}
            gridTemplate={gridTemplate}
            scrollRef={scrollRef}
            dimmed={query.isPlaceholderData}
          />
        )}
      </div>
      <PaginationBar
        page={page}
        pageSize={pageSize}
        count={songs.length}
        hasMore={
          query.data?.total != null
            ? (page + 1) * pageSize < query.data.total
            : (query.data?.songs.length ?? 0) === pageSize
        }
        total={query.data?.total ?? undefined}
        pageSizeOptions={[50, 100, 250]}
        onPageSize={(n) => update({ size: n === SONG_PAGE_SIZE ? "" : String(n) })}
        onPage={(p) => {
          update({ page: String(p) }, false);
          scrollRef.current?.scrollTo({ top: 0 });
        }}
      />
    </div>
  );
}
