// Floating summary of a multi-selection. Fixed to the viewport so it never
// disturbs the virtualized list it belongs to, and only one track list is
// ever mounted at a time.

import { ListEnd, ListMusic, ListPlus, Play, X } from "lucide-react";
import { useT } from "../../lib/i18n";
import type { Song } from "../../lib/subsonic/types";
import { usePlayerStore } from "../../stores/playerStore";
import { useUiStore } from "../../stores/uiStore";

const actionClass =
  "flex cursor-pointer items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-medium text-dim transition-colors hover:bg-surface-2 hover:text-text";

export function SelectionBar({ songs, onClear }: { songs: Song[]; onClear: () => void }) {
  const t = useT();
  const openNewPlaylist = useUiStore((s) => s.openNewPlaylist);
  const { playContext, playNext, addToQueue } = usePlayerStore.getState();
  if (songs.length < 2) return null;

  return (
    <div className="fixed bottom-32 left-1/2 z-40 flex -translate-x-1/2 items-center gap-1 rounded-full border border-edge bg-surface/95 py-1.5 pr-1.5 pl-4 shadow-xl backdrop-blur">
      <span className="mr-1 text-xs font-medium tabular-nums">
        {t("selection.count", { n: songs.length })}
      </span>
      <button className={actionClass} onClick={() => playContext(songs, 0)}>
        <Play className="size-3.5" /> {t("common.play")}
      </button>
      <button className={actionClass} onClick={() => playNext(songs)}>
        <ListEnd className="size-3.5" /> {t("menu.playNext")}
      </button>
      <button className={actionClass} onClick={() => addToQueue(songs)}>
        <ListPlus className="size-3.5" /> {t("queue.title")}
      </button>
      <button
        className={actionClass}
        onClick={() => openNewPlaylist(songs.map((song) => song.id))}
      >
        <ListMusic className="size-3.5" /> {t("playlists.new")}
      </button>
      <button
        aria-label={t("selection.clear")}
        title={t("selection.clearHint")}
        onClick={onClear}
        className="cursor-pointer rounded-full p-1.5 text-dim transition-colors hover:bg-surface-2 hover:text-text"
      >
        <X className="size-3.5" />
      </button>
    </div>
  );
}
