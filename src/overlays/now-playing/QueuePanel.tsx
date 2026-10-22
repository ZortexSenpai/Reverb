import { AnimatePresence, Reorder, motion } from "motion/react";
import { ListPlus, ListX, Pause, Play as PlayIcon, X } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { Tabs } from "radix-ui";
import { formatDuration, formatRelativeDate } from "../../lib/format";
import { useT } from "../../lib/i18n";
import { cmd, errorMessage } from "../../lib/ipc/commands";
import { qk } from "../../lib/subsonic/keys";
import { usePlayHistory } from "../../lib/subsonic/queries";
import type { Play, Song } from "../../lib/subsonic/types";
import { usePlayerStore, type QueueItem } from "../../stores/playerStore";
import { usePrefsStore } from "../../stores/prefsStore";
import { useUiStore, type QueuePanelTab } from "../../stores/uiStore";
import { CoverImage } from "../../components/library/CoverImage";
import { TrackContextMenu } from "../../components/library/TrackContextMenu";
import { IconButton } from "../../components/ui/Button";

/** Same selection pill as the sidebar, so it reads as glass in liquid mode. */
const tabClass = (active: boolean) =>
  `cursor-pointer rounded-full px-2.5 py-1 text-xs transition-colors ${
    active ? "glass-active bg-surface-2 font-medium text-text" : "text-dim hover:text-text"
  }`;

const emptyClass = "px-2 py-8 text-center text-xs text-faint";

/** One Song in either tab: the Queue's items and the Play History's Plays. */
function Row({
  song,
  current = false,
  onPlay,
  playLabel,
  detail,
  onRemove,
}: {
  song: Song;
  current?: boolean;
  /** Double-click and the button over the cover. */
  onPlay: () => void;
  playLabel: string;
  /** The duration in the Queue, how long ago in the Play History. */
  detail: string;
  /** When set, offers the hover "Remove from queue" button. */
  onRemove?: () => void;
}) {
  const t = useT();
  const playing = usePlayerStore((s) => s.state === "playing");
  return (
    <div
      className={`group flex w-full cursor-pointer items-center gap-2.5 rounded-md px-2 py-1.5 transition-colors glass-hover hover:bg-surface-2 ${
        current ? "glass-active bg-surface-2" : ""
      }`}
      onDoubleClick={onPlay}
    >
      <div className="relative">
        <CoverImage coverArt={song.coverArt} size={80} alt="" className="size-10 rounded" />
        <button
          aria-label={playLabel}
          onClick={onPlay}
          className="absolute inset-0 flex cursor-pointer items-center justify-center rounded bg-black/50 opacity-0 transition-opacity group-hover:opacity-100"
        >
          {current && playing ? (
            <Pause className="size-4 fill-white text-white" />
          ) : (
            <PlayIcon className="size-4 fill-white text-white" />
          )}
        </button>
      </div>
      <div className="min-w-0 flex-1">
        <div className={`truncate text-sm ${current ? "font-medium text-accent" : ""}`}>
          {song.title}
        </div>
        <div className="truncate text-xs text-dim">{song.artist}</div>
      </div>
      <span className="text-xs tabular-nums text-faint">{detail}</span>
      {onRemove && (
        <button
          aria-label={t("queue.remove")}
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          className="cursor-pointer rounded p-1 text-faint opacity-0 transition-opacity group-hover:opacity-100 hover:text-danger"
        >
          <X className="size-3.5" />
        </button>
      )}
    </div>
  );
}

function QueueRow({ item, current }: { item: QueueItem; current: boolean }) {
  const t = useT();
  const { jumpTo, removeFromQueue } = usePlayerStore.getState();
  return (
    <Row
      song={item.song}
      current={current}
      onPlay={() => jumpTo(item.queueId)}
      playLabel={t("queue.playFromHere")}
      detail={formatDuration(item.song.duration)}
      onRemove={() => removeFromQueue(item.queueId)}
    />
  );
}

function PlayRow({ play }: { play: Play }) {
  const t = useT();
  const { playNow } = usePlayerStore.getState();
  // "Play now" slots the Song in after the current one; the Play History is
  // never handed to the player as a list to play through.
  return (
    <TrackContextMenu song={play.song} context={[play.song]} index={0} onPlayNow={playNow}>
      <div>
        <Row
          song={play.song}
          onPlay={() => playNow(play.song)}
          playLabel={t("menu.playNow")}
          detail={formatRelativeDate(new Date(play.playedAt).toISOString(), t)}
        />
      </div>
    </TrackContextMenu>
  );
}

function PlayHistoryList() {
  const t = useT();
  const { data: plays } = usePlayHistory();
  if (!plays) return null;
  if (plays.length === 0) return <p className={emptyClass}>{t("queue.nothingPlayed")}</p>;
  // A Song played twice is two Plays, so the Song id alone is no key; when it
  // started tells them apart, and stays put as new Plays arrive on top.
  return plays.map((play) => <PlayRow key={`${play.playedAt}-${play.song.id}`} play={play} />);
}

function ClearHistoryButton() {
  const t = useT();
  const queryClient = useQueryClient();
  const showToast = useUiStore((s) => s.showToast);
  const { data: plays } = usePlayHistory();

  async function clearHistory() {
    try {
      await cmd("history_clear");
      void queryClient.invalidateQueries({ queryKey: qk.playHistory() });
    } catch (e) {
      showToast(errorMessage(e), "error");
    }
  }

  return (
    <IconButton
      label={t("queue.clearHistory")}
      onClick={() => void clearHistory()}
      disabled={!plays?.length}
    >
      <ListX className="size-4" />
    </IconButton>
  );
}

export function QueuePanel() {
  const t = useT();
  const open = useUiStore((s) => s.queuePanelOpen);
  const setOpen = useUiStore((s) => s.setQueuePanelOpen);
  const tab = useUiStore((s) => s.queuePanelTab);
  const setTab = useUiStore((s) => s.setQueuePanelTab);
  const openNewPlaylist = useUiStore((s) => s.openNewPlaylist);
  const queue = usePlayerStore((s) => s.queue);
  const queueIndex = usePlayerStore((s) => s.queueIndex);
  const liquid = usePrefsStore((s) => s.prefs.layout) === "liquid";
  const { setUpcoming, clearQueue } = usePlayerStore.getState();

  const current = queueIndex >= 0 ? queue[queueIndex] : null;
  const upcoming = queue.slice(queueIndex + 1);

  // Liquid: a floating glass panel inset by the shell padding (top/right-3)
  // and clearing the player bar (12px padding + 84px bar + 12px gap = 108px).
  // Classic: a solid panel attached to the window edge above the 84px bar.
  const slideX = liquid ? 360 : 340;

  return (
    <AnimatePresence>
      {open && (
        <motion.aside
          initial={{ x: slideX }}
          animate={{ x: 0 }}
          exit={{ x: slideX }}
          transition={{ type: "tween", duration: 0.2 }}
          className={`fixed z-30 flex w-[330px] flex-col ${
            liquid
              ? "glass top-3 right-3 bottom-[108px] overflow-hidden rounded-2xl"
              : "top-0 right-0 bottom-[84px] border-l border-edge bg-surface"
          }`}
        >
          <Tabs.Root
            value={tab}
            onValueChange={(value) => setTab(value as QueuePanelTab)}
            className="flex min-h-0 flex-1 flex-col"
          >
            {/* Wraps the buttons under the tabs should a language's tab names
                not leave them room, rather than pushing Close out of the panel. */}
            <div
              className={`flex flex-wrap items-center gap-y-2 border-b px-4 py-3 ${
                liquid ? "border-text/10" : "border-edge"
              }`}
            >
              <Tabs.List className="flex gap-1">
                <Tabs.Trigger value="queue" className={tabClass(tab === "queue")}>
                  {t("queue.title")}
                </Tabs.Trigger>
                <Tabs.Trigger value="playHistory" className={tabClass(tab === "playHistory")}>
                  {t("queue.recentlyPlayed")}
                </Tabs.Trigger>
              </Tabs.List>
              <div className="ml-auto flex items-center gap-1">
                {tab === "queue" ? (
                  <>
                    <IconButton
                      label={t("queue.saveAsPlaylist")}
                      onClick={() => openNewPlaylist(queue.map((item) => item.song.id))}
                      disabled={queue.length === 0}
                    >
                      <ListPlus className="size-4" />
                    </IconButton>
                    <IconButton
                      label={t("queue.clear")}
                      onClick={clearQueue}
                      disabled={queue.length <= (current ? 1 : 0)}
                    >
                      <ListX className="size-4" />
                    </IconButton>
                  </>
                ) : (
                  <ClearHistoryButton />
                )}
                <IconButton label={t("common.close")} onClick={() => setOpen(false)}>
                  <X className="size-4" />
                </IconButton>
              </div>
            </div>
            <Tabs.Content value="queue" className="flex-1 overflow-y-auto p-2">
              {queue.length === 0 ? (
                <p className={emptyClass}>{t("queue.empty")}</p>
              ) : (
                <>
                  {current && (
                    <>
                      <div className="px-2 pt-1 pb-1 text-[11px] font-medium tracking-wide text-faint uppercase">
                        {t("queue.nowPlaying")}
                      </div>
                      <QueueRow item={current} current />
                    </>
                  )}
                  {upcoming.length > 0 && (
                    <>
                      <div className="px-2 pt-3 pb-1 text-[11px] font-medium tracking-wide text-faint uppercase">
                        {t("queue.nextUp")}
                      </div>
                      <Reorder.Group axis="y" values={upcoming} onReorder={setUpcoming}>
                        {upcoming.map((item) => (
                          <Reorder.Item key={item.queueId} value={item} className="cursor-grab">
                            <QueueRow item={item} current={false} />
                          </Reorder.Item>
                        ))}
                      </Reorder.Group>
                    </>
                  )}
                </>
              )}
            </Tabs.Content>
            <Tabs.Content value="playHistory" className="flex-1 overflow-y-auto p-2">
              <PlayHistoryList />
            </Tabs.Content>
          </Tabs.Root>
        </motion.aside>
      )}
    </AnimatePresence>
  );
}
