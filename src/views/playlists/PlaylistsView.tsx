import { LayoutGrid, List, ListMusic, Pin, Plus } from "lucide-react";
import { ToggleGroup } from "radix-ui";
import { useMemo, useRef } from "react";
import { useNavigate, useSearchParams } from "react-router";
import { CardGrid } from "../../components/library/CardGrid";
import { CoverImage } from "../../components/library/CoverImage";
import { FilterInput } from "../../components/library/FilterInput";
import { PlaylistCard } from "../../components/library/PlaylistCard";
import { Button } from "../../components/ui/Button";
import { CenteredSpinner } from "../../components/ui/Spinner";
import { formatLongDuration } from "../../lib/format";
import { useT } from "../../lib/i18n";
import { usePlaylists } from "../../lib/subsonic/queries";
import type { PlaylistLayout } from "../../lib/subsonic/types";
import { GRID_CARD_WIDTH, usePrefsStore } from "../../stores/prefsStore";
import { useUiStore } from "../../stores/uiStore";

const layoutToggleClass =
  "cursor-pointer rounded-full px-3 py-1.5 text-dim transition-colors hover:text-text data-[state=on]:bg-surface-2 data-[state=on]:text-text";

export function PlaylistsView() {
  const t = useT();
  const { data: playlists, isPending, isError } = usePlaylists();
  const [params, setParams] = useSearchParams();
  const scrollRef = useRef<HTMLDivElement>(null);
  const pinned = usePrefsStore((s) => s.prefs.pinnedPlaylists);
  const togglePinned = usePrefsStore((s) => s.togglePinnedPlaylist);
  const layout = usePrefsStore((s) => s.prefs.playlistLayout);
  const gridSize = usePrefsStore((s) => s.prefs.gridSize);
  const setPrefs = usePrefsStore((s) => s.set);
  const openNewPlaylist = useUiStore((s) => s.openNewPlaylist);
  const navigate = useNavigate();

  const q = params.get("q") ?? "";

  function setFilter(value: string) {
    const next = new URLSearchParams(params);
    if (value === "") next.delete("q");
    else next.set("q", value);
    setParams(next, { replace: true });
  }

  // The whole playlist list is already in hand, so filtering stays local.
  const visible = useMemo(() => {
    const needle = q.trim().toLowerCase();
    if (needle === "" || !playlists) return playlists ?? [];
    return playlists.filter((playlist) =>
      [playlist.name, playlist.comment, playlist.owner].some((field) =>
        field?.toLowerCase().includes(needle),
      ),
    );
  }, [playlists, q]);

  if (isPending) return <CenteredSpinner />;
  if (isError) return <div className="p-8 text-sm text-dim">{t("playlists.loadError")}</div>;

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-wrap items-center gap-3 px-6 py-4">
        <h1 className="text-2xl font-semibold">{t("nav.playlists")}</h1>
        <Button variant="outline" className="rounded-full" onClick={() => openNewPlaylist()}>
          <Plus className="size-4" /> {t("playlists.new")}
        </Button>
        {playlists.length > 0 && (
          <>
            <ToggleGroup.Root
              type="single"
              value={layout}
              // Radix reports "" when the active item is pressed again; keep
              // the current layout rather than ending up with neither.
              onValueChange={(value) =>
                value && setPrefs({ playlistLayout: value as PlaylistLayout })
              }
              aria-label={t("playlists.layout")}
              className="ml-auto flex items-center gap-0.5 rounded-full border border-edge p-0.5"
            >
              <ToggleGroup.Item
                value="grid"
                aria-label={t("playlists.gridView")}
                title={t("playlists.gridView")}
                className={layoutToggleClass}
              >
                <LayoutGrid className="size-4" />
              </ToggleGroup.Item>
              <ToggleGroup.Item
                value="list"
                aria-label={t("playlists.listView")}
                title={t("playlists.listView")}
                className={layoutToggleClass}
              >
                <List className="size-4" />
              </ToggleGroup.Item>
            </ToggleGroup.Root>
            <FilterInput
              value={q}
              onChange={setFilter}
              placeholder={t("playlists.filter")}
              className="w-56"
            />
          </>
        )}
      </div>
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
        {playlists.length === 0 ? (
          <div className="flex flex-col items-center gap-3 py-24 text-dim">
            <ListMusic className="size-10 text-faint" />
            <p className="text-sm">{t("playlists.none")}</p>
          </div>
        ) : visible.length === 0 ? (
          <div className="py-24 text-center text-sm text-dim">
            {t("playlists.noMatch", { q: q.trim() })}
          </div>
        ) : layout === "grid" ? (
          <div className="px-4 pb-8">
            <CardGrid
              items={visible}
              itemKey={(playlist) => playlist.id}
              renderItem={(playlist) => <PlaylistCard playlist={playlist} />}
              scrollRef={scrollRef}
              cardMinWidth={GRID_CARD_WIDTH[gridSize] ?? 190}
            />
          </div>
        ) : (
          <div className="flex flex-col gap-0.5 px-4 pb-8">
            {visible.map((playlist) => {
              const isPinned = pinned.includes(playlist.id);
              return (
                <button
                  key={playlist.id}
                  onClick={() => navigate(`/playlists/${playlist.id}`)}
                  className="group flex cursor-pointer items-center gap-4 rounded-md px-3 py-2.5 text-left transition-colors hover:bg-surface"
                >
                  <CoverImage
                    coverArt={playlist.coverArt}
                    size={96}
                    alt=""
                    className="size-14 rounded-md"
                  />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium">{playlist.name}</div>
                    <div className="text-xs text-dim">
                      {t("count.tracks", { n: playlist.songCount })} ·{" "}
                      {formatLongDuration(playlist.duration, t)}
                      {playlist.owner ? ` · ${t("playlist.by", { owner: playlist.owner })}` : ""}
                    </div>
                  </div>
                  <span
                    role="button"
                    aria-label={isPinned ? t("playlist.unpin") : t("playlist.pin")}
                    title={
                      isPinned ? t("playlist.unpinFromSidebar") : t("playlist.pinToSidebar")
                    }
                    onClick={(e) => {
                      e.stopPropagation();
                      togglePinned(playlist.id);
                    }}
                    className={`cursor-pointer rounded-full p-2 transition-all ${
                      isPinned
                        ? "text-accent hover:text-accent-bright"
                        : "text-dim opacity-0 group-hover:opacity-100 hover:text-text"
                    }`}
                  >
                    <Pin className={`size-4 ${isPinned ? "fill-current" : ""}`} />
                  </span>
                </button>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
