import { AnimatePresence, Reorder, motion } from "motion/react";
import { ListPlus, ListX, Pause, Play, X } from "lucide-react";
import { formatDuration } from "../../lib/format";
import { useT } from "../../lib/i18n";
import { usePlayerStore, type QueueItem } from "../../stores/playerStore";
import { usePrefsStore } from "../../stores/prefsStore";
import { useUiStore } from "../../stores/uiStore";
import { CoverImage } from "../../components/library/CoverImage";
import { IconButton } from "../../components/ui/Button";

function Row({ item, current }: { item: QueueItem; current: boolean }) {
  const t = useT();
  const playing = usePlayerStore((s) => s.state === "playing");
  const { jumpTo, removeFromQueue } = usePlayerStore.getState();
  return (
    <div
      className={`group flex w-full cursor-pointer items-center gap-2.5 rounded-md px-2 py-1.5 transition-colors glass-hover hover:bg-surface-2 ${
        current ? "glass-active bg-surface-2" : ""
      }`}
      onDoubleClick={() => jumpTo(item.queueId)}
    >
      <div className="relative">
        <CoverImage coverArt={item.song.coverArt} size={80} alt="" className="size-10 rounded" />
        <button
          aria-label={t("queue.playFromHere")}
          onClick={() => jumpTo(item.queueId)}
          className="absolute inset-0 flex cursor-pointer items-center justify-center rounded bg-black/50 opacity-0 transition-opacity group-hover:opacity-100"
        >
          {current && playing ? (
            <Pause className="size-4 fill-white text-white" />
          ) : (
            <Play className="size-4 fill-white text-white" />
          )}
        </button>
      </div>
      <div className="min-w-0 flex-1">
        <div className={`truncate text-sm ${current ? "font-medium text-accent" : ""}`}>
          {item.song.title}
        </div>
        <div className="truncate text-xs text-dim">{item.song.artist}</div>
      </div>
      <span className="text-xs tabular-nums text-faint">{formatDuration(item.song.duration)}</span>
      <button
        aria-label={t("queue.remove")}
        onClick={(e) => {
          e.stopPropagation();
          removeFromQueue(item.queueId);
        }}
        className="cursor-pointer rounded p-1 text-faint opacity-0 transition-opacity group-hover:opacity-100 hover:text-danger"
      >
        <X className="size-3.5" />
      </button>
    </div>
  );
}

export function QueuePanel() {
  const t = useT();
  const open = useUiStore((s) => s.queuePanelOpen);
  const setOpen = useUiStore((s) => s.setQueuePanelOpen);
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
          <div
            className={`flex items-center justify-between border-b px-4 py-3 ${
              liquid ? "border-text/10" : "border-edge"
            }`}
          >
            <h2 className="text-sm font-semibold">{t("queue.title")}</h2>
            <div className="flex items-center gap-1">
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
              <IconButton label={t("common.close")} onClick={() => setOpen(false)}>
                <X className="size-4" />
              </IconButton>
            </div>
          </div>
          <div className="flex-1 overflow-y-auto p-2">
            {queue.length === 0 ? (
              <p className="px-2 py-8 text-center text-xs text-faint">{t("queue.empty")}</p>
            ) : (
              <>
                {current && (
                  <>
                    <div className="px-2 pt-1 pb-1 text-[11px] font-medium tracking-wide text-faint uppercase">
                      {t("queue.nowPlaying")}
                    </div>
                    <Row item={current} current />
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
                          <Row item={item} current={false} />
                        </Reorder.Item>
                      ))}
                    </Reorder.Group>
                  </>
                )}
              </>
            )}
          </div>
        </motion.aside>
      )}
    </AnimatePresence>
  );
}
