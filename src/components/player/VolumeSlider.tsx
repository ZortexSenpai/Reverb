import { Popover, Slider } from "radix-ui";
import { Volume1, Volume2, VolumeX } from "lucide-react";
import { useCallback, useRef } from "react";
import { useT } from "../../lib/i18n";
import { usePlayerStore } from "../../stores/playerStore";

// One mouse-wheel notch (~100px) moves the volume 5%. Trackpads send many
// small deltas, so the change is proportional rather than per event, and
// capped so an accelerated flick can't jump the volume in one go.
const WHEEL_VOLUME_PER_PX = 0.0005;
const WHEEL_MAX_STEP = 0.05;

/**
 * Callback ref that makes an element change the volume on scroll. Attached
 * natively because React's onWheel is passive and can't stop the page behind
 * the popover from scrolling too.
 */
function useWheelVolume() {
  return useCallback((el: HTMLElement | null) => {
    if (!el) return;
    function onWheel(e: WheelEvent) {
      e.preventDefault();
      const px = e.deltaMode === WheelEvent.DOM_DELTA_LINE ? e.deltaY * 16 : e.deltaY;
      const step = Math.max(-WHEEL_MAX_STEP, Math.min(WHEEL_MAX_STEP, -px * WHEEL_VOLUME_PER_PX));
      const { volume, setVolume } = usePlayerStore.getState();
      setVolume(volume + step);
    }
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);
}

export function VolumeSlider() {
  const t = useT();
  const volume = usePlayerStore((s) => s.volume);
  const setVolume = usePlayerStore((s) => s.setVolume);
  const lastNonZero = useRef(volume > 0 ? volume : 0.7);
  const wheelRef = useWheelVolume();

  const muted = volume === 0;
  const Icon = muted ? VolumeX : volume < 0.5 ? Volume1 : Volume2;
  const muteLabel = muted ? t("player.unmute") : t("player.mute");

  // Clicking the icon opens the slider, so mute lives inside the panel rather
  // than on the trigger. (The `m` shortcut still toggles it from anywhere.)
  function toggleMute() {
    if (volume > 0) {
      lastNonZero.current = volume;
      setVolume(0);
    } else {
      setVolume(lastNonZero.current);
    }
  }

  return (
    <Popover.Root>
      <Popover.Trigger asChild>
        <button
          ref={wheelRef}
          aria-label={t("player.volume")}
          title={t("player.volume")}
          className="flex cursor-pointer items-center gap-1 rounded-full px-2 py-1.5 text-dim transition-colors hover:bg-surface-2 hover:text-text"
        >
          <Icon className="size-4 shrink-0" />
          {/* Fixed width and tabular figures: the reading must not shift the
              controls beside it as it goes 9% → 10% → 100%. Ranged left, so
              that slack sits after the number instead of opening a gap
              between it and the icon. */}
          <span className="w-9 text-xs tabular-nums">
            {t.n(volume, { style: "percent", maximumFractionDigits: 0 })}
          </span>
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          ref={wheelRef}
          side="top"
          sideOffset={8}
          collisionPadding={8}
          // Above the full-screen player (z-20), which renders this too.
          className="z-50 flex flex-col items-center gap-2 rounded-full border border-edge bg-surface px-2 py-3 shadow-xl focus:outline-none"
        >
          <Slider.Root
            orientation="vertical"
            value={[volume]}
            max={1}
            step={0.01}
            onValueChange={([v]) => setVolume(v)}
            aria-label={t("player.volume")}
            className="relative flex h-28 w-5 cursor-pointer touch-none flex-col items-center select-none"
          >
            <Slider.Track className="relative w-1 grow rounded-full bg-surface-2">
              <Slider.Range className="absolute w-full rounded-full bg-text" />
            </Slider.Track>
            <Slider.Thumb className="block size-3 rounded-full bg-text focus:outline-none" />
          </Slider.Root>
          <button
            aria-label={muteLabel}
            title={muteLabel}
            onClick={toggleMute}
            className={`cursor-pointer rounded-full p-1 transition-colors hover:bg-surface-2 ${
              muted ? "text-accent" : "text-dim hover:text-text"
            }`}
          >
            <Icon className="size-3.5" />
          </button>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
