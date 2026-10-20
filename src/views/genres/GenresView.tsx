import { useMemo, useRef } from "react";
import { useSearchParams } from "react-router";
import { CardGrid } from "../../components/library/CardGrid";
import { FilterInput } from "../../components/library/FilterInput";
import { GenreCard } from "../../components/library/GenreCard";
import { PaginationBar } from "../../components/library/PaginationBar";
import { CenteredSpinner } from "../../components/ui/Spinner";
import { useT, type MessageKey } from "../../lib/i18n";
import { useGenres } from "../../lib/subsonic/queries";
import type { Genre } from "../../lib/subsonic/types";
import { GRID_CARD_WIDTH, usePrefsStore } from "../../stores/prefsStore";

const DEFAULT_PAGE_SIZE = 96;

type SortMode = "name" | "name-desc" | "albums" | "songs";

const SORTS: { value: SortMode; labelKey: MessageKey }[] = [
  { value: "name", labelKey: "sort.nameAsc" },
  { value: "name-desc", labelKey: "sort.nameDesc" },
  { value: "albums", labelKey: "sort.mostAlbums" },
  { value: "songs", labelKey: "sort.mostSongs" },
];

function compareGenres(a: Genre, b: Genre, mode: SortMode): number {
  switch (mode) {
    case "name":
      return a.value.localeCompare(b.value);
    case "name-desc":
      return b.value.localeCompare(a.value);
    case "albums":
      return b.albumCount - a.albumCount || a.value.localeCompare(b.value);
    case "songs":
      return b.songCount - a.songCount || a.value.localeCompare(b.value);
  }
}

export function GenresView() {
  const t = useT();
  const { data, isPending, isError } = useGenres();
  const gridSize = usePrefsStore((s) => s.prefs.gridSize);
  const [params, setParams] = useSearchParams();
  const scrollRef = useRef<HTMLDivElement>(null);

  const q = params.get("q") ?? "";
  const sort = (params.get("sort") as SortMode) ?? "name";
  const page = Math.max(0, Number(params.get("page") ?? "0") || 0);
  const pageSize = Number(params.get("size") ?? "") || DEFAULT_PAGE_SIZE;

  // getGenres returns the whole list at once, so filter/sort/pagination are
  // all local — same shape as the artists view.
  const filtered = useMemo(() => {
    const all = (data ?? []).filter((g) => g.albumCount > 0 || g.songCount > 0);
    const needle = q.trim().toLowerCase();
    const matched = needle
      ? all.filter((genre) => genre.value.toLowerCase().includes(needle))
      : all;
    return [...matched].sort((a, b) => compareGenres(a, b, sort));
  }, [data, q, sort]);

  const pageCount = Math.max(1, Math.ceil(filtered.length / pageSize));
  const safePage = Math.min(page, pageCount - 1);
  const pageItems = filtered.slice(safePage * pageSize, (safePage + 1) * pageSize);

  function update(patch: Record<string, string>, resetPage = true) {
    const next = new URLSearchParams(params);
    for (const [key, value] of Object.entries(patch)) {
      if (value === "") next.delete(key);
      else next.set(key, value);
    }
    if (resetPage) next.delete("page");
    setParams(next, { replace: true });
    scrollRef.current?.scrollTo({ top: 0 });
  }

  if (isPending) return <CenteredSpinner />;
  if (isError) return <div className="p-8 text-sm text-dim">{t("genres.loadError")}</div>;

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-wrap items-center gap-2 px-6 py-4">
        <h1 className="mr-1 text-2xl font-semibold">{t("nav.genres")}</h1>
        <span className="mr-2 text-sm text-faint">{t.n(filtered.length)}</span>
        <select
          value={sort}
          onChange={(e) => update({ sort: e.target.value })}
          className="cursor-pointer rounded-md border border-edge bg-surface px-2.5 py-1.5 text-sm text-text focus:border-accent focus:outline-none"
        >
          {SORTS.map((s) => (
            <option key={s.value} value={s.value}>
              {t(s.labelKey)}
            </option>
          ))}
        </select>
        <FilterInput
          value={q}
          onChange={(value) => update({ q: value })}
          placeholder={t("genres.filter")}
          className="ml-auto w-56"
        />
      </div>
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
        <div className="px-4 pb-4">
          {pageItems.length === 0 ? (
            <div className="px-2 py-16 text-center text-sm text-dim">
              {q.trim() === "" ? t("genres.none") : t("genres.noMatch", { q: q.trim() })}
            </div>
          ) : (
            <CardGrid
              items={pageItems}
              itemKey={(genre) => genre.value}
              renderItem={(genre) => <GenreCard genre={genre} />}
              scrollRef={scrollRef}
              // The label sits inside the tile, so rows only need the padding.
              rowExtra={24}
              cardMinWidth={GRID_CARD_WIDTH[gridSize] ?? 190}
            />
          )}
        </div>
      </div>
      <PaginationBar
        page={safePage}
        pageSize={pageSize}
        count={pageItems.length}
        hasMore={safePage < pageCount - 1}
        total={filtered.length}
        pageSizeOptions={[48, 96, 192]}
        onPageSize={(n) => update({ size: n === DEFAULT_PAGE_SIZE ? "" : String(n) })}
        onPage={(p) => update({ page: String(p) }, false)}
      />
    </div>
  );
}
