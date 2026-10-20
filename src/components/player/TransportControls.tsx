import { Pause, Play, Repeat, Repeat1, Shuffle, SkipBack, SkipForward } from "lucide-react";
import { useT } from "../../lib/i18n";
import { usePlayerStore } from "../../stores/playerStore";
import { IconButton } from "../ui/Button";
import { Spinner } from "../ui/Spinner";

export function TransportControls() {
  const t = useT();
  const state = usePlayerStore((s) => s.state);
  const shuffle = usePlayerStore((s) => s.shuffle);
  const repeat = usePlayerStore((s) => s.repeat);
  const hasQueue = usePlayerStore((s) => s.queue.length > 0);
  const { togglePlay, next, previous, toggleShuffle, cycleRepeat } = usePlayerStore.getState();

  const playing = state === "playing";
  const buffering = state === "buffering";

  return (
    <div className="flex items-center gap-1">
      <IconButton
        label={t("common.shuffle")}
        onClick={toggleShuffle}
        className={shuffle ? "text-accent hover:text-accent-bright" : ""}
      >
        <Shuffle className="size-4" />
      </IconButton>
      <IconButton label={t("player.previous")} onClick={previous} disabled={!hasQueue}>
        <SkipBack className="size-5 fill-current" />
      </IconButton>
      <button
        aria-label={playing ? t("player.pause") : t("common.play")}
        onClick={togglePlay}
        disabled={!hasQueue && state === "stopped"}
        className="play-button mx-1 flex size-10 cursor-pointer items-center justify-center rounded-full bg-text text-bg transition-transform hover:scale-105 disabled:cursor-not-allowed disabled:opacity-40"
      >
        {buffering ? (
          <Spinner className="size-5 border-bg/30 border-t-bg" />
        ) : playing ? (
          <Pause className="size-5 fill-current" />
        ) : (
          <Play className="ml-0.5 size-5 fill-current" />
        )}
      </button>
      <IconButton label={t("player.next")} onClick={next} disabled={!hasQueue}>
        <SkipForward className="size-5 fill-current" />
      </IconButton>
      <IconButton
        label={t("player.repeat", { mode: t(`repeat.${repeat}`) })}
        onClick={cycleRepeat}
        className={repeat !== "off" ? "text-accent hover:text-accent-bright" : ""}
      >
        {repeat === "one" ? <Repeat1 className="size-4" /> : <Repeat className="size-4" />}
      </IconButton>
    </div>
  );
}
