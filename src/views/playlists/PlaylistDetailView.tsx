import { DropdownMenu } from "radix-ui";
import {
  ArrowLeft,
  MoreVertical,
  Pencil,
  Pin,
  Play,
  RefreshCw,
  Shuffle,
  Trash2,
} from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router";
import { CoverImage } from "../../components/library/CoverImage";
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
import { Button, IconButton } from "../../components/ui/Button";
import { ConfirmDialog, TextPromptDialog } from "../../components/ui/Dialogs";
import { CenteredSpinner } from "../../components/ui/Spinner";
import { cmd, errorMessage } from "../../lib/ipc/commands";
import { formatLongDuration } from "../../lib/format";
import { useT } from "../../lib/i18n";
import { qk } from "../../lib/subsonic/keys";
import { usePlaylist } from "../../lib/subsonic/queries";
import type { Song } from "../../lib/subsonic/types";
import { usePlayerStore } from "../../stores/playerStore";
import { usePrefsStore } from "../../stores/prefsStore";
import { useUiStore } from "../../stores/uiStore";

const menuItemClass =
  "flex cursor-pointer items-center gap-2.5 rounded px-2.5 py-1.5 text-sm text-text outline-none select-none data-highlighted:bg-surface-2";

export function PlaylistDetailView() {
  const t = useT();
  const { playlistId = "" } = useParams();
  const navigate = useNavigate();
  const scrollRef = useRef<HTMLDivElement>(null);
  const { data: playlist, isPending, isError, refetch, isRefetching } = usePlaylist(playlistId);
  const playContext = usePlayerStore((s) => s.playContext);
  const songColumns = usePrefsStore((s) => s.prefs.songColumns);
  const pinned = usePrefsStore((s) => s.prefs.pinnedPlaylists);
  const togglePinned = usePrefsStore((s) => s.togglePinnedPlaylist);
  const showToast = useUiStore((s) => s.showToast);
  const queryClient = useQueryClient();

  const [sortId, setSortId] = useState("");
  const [sortDesc, setSortDesc] = useState(false);
  const [renameOpen, setRenameOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);

  const columns = useMemo(() => activeColumns(songColumns), [songColumns]);
  const gridTemplate = useMemo(() => songGridTemplate(columns), [columns]);

  // Sorting is a client-side display order; default is playlist order.
  const songs = useMemo(() => {
    const raw = playlist?.entry ?? [];
    const compare = sortId ? compareSongsBy(sortId) : null;
    if (!compare) return raw;
    const sorted = [...raw].sort(compare);
    return sortDesc ? sorted.reverse() : sorted;
  }, [playlist?.entry, sortId, sortDesc]);

  // updatePlaylist removes by *original* playlist position; map by object
  // identity so display sorting (and duplicate songs) stay correct.
  const originalIndex = useMemo(
    () => new Map((playlist?.entry ?? []).map((song, i) => [song, i])),
    [playlist?.entry],
  );

  async function removeFromPlaylist(songsToRemove: Song[]) {
    const indices = songsToRemove
      .map((song) => originalIndex.get(song))
      .filter((index): index is number => index != null);
    if (indices.length === 0) return;
    try {
      await cmd("library_update_playlist", { playlistId, removeIndices: indices });
      void queryClient.invalidateQueries({ queryKey: qk.playlist(playlistId) });
      void queryClient.invalidateQueries({ queryKey: qk.playlists() });
    } catch (e) {
      showToast(errorMessage(e), "error");
    }
  }

  async function renamePlaylist(name: string) {
    try {
      await cmd("library_update_playlist", { playlistId, name });
      void queryClient.invalidateQueries({ queryKey: qk.playlist(playlistId) });
      void queryClient.invalidateQueries({ queryKey: qk.playlists() });
    } catch (e) {
      showToast(errorMessage(e), "error");
    }
  }

  async function deletePlaylist() {
    try {
      await cmd("library_delete_playlist", { id: playlistId });
      void queryClient.invalidateQueries({ queryKey: qk.playlists() });
      showToast(t("toast.playlistDeleted"));
      navigate("/playlists");
    } catch (e) {
      showToast(errorMessage(e), "error");
    }
  }

  function cycleSort(id: string) {
    if (sortId !== id) {
      setSortId(id);
      setSortDesc(false);
    } else if (!sortDesc) {
      setSortDesc(true);
    } else {
      setSortId("");
      setSortDesc(false);
    }
  }

  if (isPending) return <CenteredSpinner />;
  if (isError || !playlist) {
    return <div className="p-8 text-sm text-dim">{t("playlist.notFound")}</div>;
  }

  return (
    <div ref={scrollRef} className="h-full overflow-y-auto">
      <div className="flex items-end gap-6 bg-gradient-to-b from-surface to-bg px-6 pt-6 pb-6">
        <div className="relative">
          <IconButton
            label={t("common.back")}
            onClick={() => navigate(-1)}
            className="absolute -top-1 -left-1 z-10 bg-bg/60 backdrop-blur"
          >
            <ArrowLeft className="size-4" />
          </IconButton>
          <CoverImage
            coverArt={playlist.coverArt}
            size={600}
            alt={playlist.name}
            className="w-44 rounded-lg shadow-2xl"
          />
        </div>
        <div className="min-w-0 flex-1 pb-1">
          <div className="text-xs font-medium tracking-wide text-dim uppercase">
            {t("playlist.label")}
          </div>
          <h1 className="mt-1 mb-2 text-4xl leading-tight font-bold">{playlist.name}</h1>
          <div className="text-sm text-dim">
            {t("count.tracks", { n: playlist.songCount })} ·{" "}
            {formatLongDuration(playlist.duration, t)}
            {playlist.owner ? ` · ${t("playlist.by", { owner: playlist.owner })}` : ""}
          </div>
          {playlist.comment && <p className="mt-1 text-sm text-dim">{playlist.comment}</p>}
          <div className="mt-4 flex items-center gap-2">
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
            <IconButton
              label={
                pinned.includes(playlist.id)
                  ? t("playlist.unpinFromSidebar")
                  : t("playlist.pinToSidebar")
              }
              onClick={() => togglePinned(playlist.id)}
              className={pinned.includes(playlist.id) ? "text-accent hover:text-accent-bright" : ""}
            >
              <Pin className={`size-4 ${pinned.includes(playlist.id) ? "fill-current" : ""}`} />
            </IconButton>
            <IconButton
              label={t("playlist.reload")}
              onClick={() => void refetch()}
              disabled={isRefetching}
            >
              <RefreshCw className={`size-4 ${isRefetching ? "animate-spin" : ""}`} />
            </IconButton>
            <ColumnsMenu />
            <DropdownMenu.Root>
              <DropdownMenu.Trigger asChild>
                <button
                  aria-label={t("playlist.options")}
                  className="cursor-pointer rounded-full p-2 text-dim transition-colors hover:bg-surface-2 hover:text-text"
                >
                  <MoreVertical className="size-4" />
                </button>
              </DropdownMenu.Trigger>
              <DropdownMenu.Portal>
                <DropdownMenu.Content
                  align="start"
                  className="z-50 min-w-44 rounded-lg border border-edge bg-surface p-1 shadow-xl"
                >
                  <DropdownMenu.Item
                    className={menuItemClass}
                    onSelect={() => setRenameOpen(true)}
                  >
                    <Pencil className="size-4 text-dim" /> {t("playlist.renameItem")}
                  </DropdownMenu.Item>
                  <DropdownMenu.Item
                    className={menuItemClass}
                    onSelect={() => setDeleteOpen(true)}
                  >
                    <Trash2 className="size-4 text-danger" /> {t("playlist.deleteItem")}
                  </DropdownMenu.Item>
                </DropdownMenu.Content>
              </DropdownMenu.Portal>
            </DropdownMenu.Root>
          </div>
        </div>
      </div>
      <div className="pt-2">
        <SongTableHeaderRow
          columns={columns}
          gridTemplate={gridTemplate}
          sortId={sortId}
          sortDesc={sortDesc}
          onSort={cycleSort}
        />
      </div>
      <div className="pb-8">
        <SongTableBody
          songs={songs}
          columns={columns}
          gridTemplate={gridTemplate}
          scrollRef={scrollRef}
          dimmed={isRefetching}
          onRemoveFromPlaylist={(removed) => void removeFromPlaylist(removed)}
        />
      </div>
      <TextPromptDialog
        open={renameOpen}
        title={t("playlist.renameTitle")}
        initialValue={playlist.name}
        confirmLabel={t("common.rename")}
        onConfirm={(name) => void renamePlaylist(name)}
        onClose={() => setRenameOpen(false)}
      />
      <ConfirmDialog
        open={deleteOpen}
        title={t("playlist.deleteTitle", { name: playlist.name })}
        body={t("playlist.deleteBody")}
        confirmLabel={t("common.delete")}
        danger
        onConfirm={() => void deletePlaylist()}
        onClose={() => setDeleteOpen(false)}
      />
    </div>
  );
}
