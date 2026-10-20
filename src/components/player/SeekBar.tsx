// Custom seek bar driven by requestAnimationFrame reading the position clock
// — zero React re-renders at 60fps; only refs are touched.
//
// The wave and bars styles animate from that same loop, mutating one SVG path
// and one clip rect per frame. Both draw into a fixed 1000-unit-wide viewBox
// stretched with preserveAspectRatio="none", so nothing has to measure the
// element's width; `vector-effect="non-scaling-stroke"` keeps the stroke an
// honest thickness once that stretch is applied.

import { useEffect, useId, useRef, useState } from "react";
import { positionClock } from "../../lib/playback/positionClock";
import { usePlayerStore } from "../../stores/playerStore";
import { usePrefsStore } from "../../stores/prefsStore";
import { formatMs } from "../../lib/format";

/** Horizontal units the wave and bar paths are drawn in, before stretching. */
const VIEW_W = 1000;
/** Wave length in those units — 10 crests across the bar. */
const WAVELENGTH = 100;
/** Units of phase per millisecond; one wavelength every ~1.7s. */
const WAVE_SPEED = 0.06;

export type SeekBarStyle = "line" | "wave" | "bars";
export type SeekBarThickness = "thin" | "medium" | "thick";

interface Metrics {
  /** Height of the interactive strip, in px (also the SVG's vertical units). */
  height: number;
  /** Peak wave deviation from the centre line, in px. */
  amplitude: number;
  /** Stroke width for the wave and the bars, in px. */
  stroke: number;
  /** Track thickness for the plain line style, in px. */
  line: number;
  /** How many bars to draw across the full width. */
  bars: number;
}

/** `medium` reproduces the pre-customisation bar exactly, so the default is a no-op. */
export const SEEK_METRICS: Record<SeekBarThickness, Metrics> = {
  thin: { height: 14, amplitude: 2.5, stroke: 1.5, line: 2, bars: 56 },
  medium: { height: 16, amplitude: 3.5, stroke: 2, line: 4, bars: 64 },
  thick: { height: 22, amplitude: 5.5, stroke: 3, line: 6, bars: 72 },
};

/**
 * A sine-ish wave built from one quadratic curve per half period. A quadratic
 * reaches only half its control offset at the midpoint, hence the doubled
 * control deviation. Drawn from one wavelength before the left edge to one
 * past the right so that scrolling `phase` never exposes an end; the SVG
 * viewport clips the overhang.
 */
function wavePath(phase: number, mid: number, amplitude: number): string {
  const half = WAVELENGTH / 2;
  // Sliding the start left by the phase makes the crests travel rightwards.
  let x = -WAVELENGTH + (phase % WAVELENGTH);
  let up = true;
  let d = `M${x.toFixed(1)} ${mid}`;
  while (x < VIEW_W + WAVELENGTH) {
    const control = mid + (up ? -amplitude : amplitude) * 2;
    d += `Q${(x + half / 2).toFixed(1)} ${control.toFixed(1)} ${(x + half).toFixed(1)} ${mid}`;
    x += half;
    up = !up;
  }
  return d;
}

/**
 * Stable per-bar height factor in 0..1. Deterministic so the silhouette stays
 * put instead of reshuffling on every frame or re-render — it reads as a
 * waveform, and a waveform that churns looks broken.
 */
function barSeed(i: number): number {
  const x = Math.sin(i * 12.9898) * 43758.5453;
  return x - Math.floor(x);
}

/** Vertical segments, drawn as one path with round caps. */
function barsPath(phase: number, mid: number, amplitude: number, count: number): string {
  const step = VIEW_W / count;
  let d = "";
  for (let i = 0; i < count; i++) {
    const seed = barSeed(i);
    // Idle silhouette; `phase` only ever lifts bars, so the shape stays
    // recognisable while it moves.
    const pulse = phase === 0 ? 0 : Math.abs(Math.sin(phase * 0.012 + i * 0.7)) * 0.45;
    const h = amplitude * (0.35 + seed * 0.65) * (1 + pulse);
    const x = (i + 0.5) * step;
    d += `M${x.toFixed(1)} ${(mid - h).toFixed(1)}V${(mid + h).toFixed(1)}`;
  }
  return d;
}

export function SeekBar() {
  const seekMs = usePlayerStore((s) => s.seekMs);
  const hasTrack = usePlayerStore((s) => s.currentTrackId !== null);
  const style = usePrefsStore((s) => s.prefs.seekBarStyle);
  const animated = usePrefsStore((s) => s.prefs.seekBarAnimated);
  const thickness = usePrefsStore((s) => s.prefs.seekBarThickness);
  const metrics = SEEK_METRICS[thickness] ?? SEEK_METRICS.medium;

  const barRef = useRef<HTMLDivElement>(null);
  const fillRef = useRef<HTMLDivElement>(null);
  const trackPathRef = useRef<SVGPathElement>(null);
  const playedPathRef = useRef<SVGPathElement>(null);
  const clipRef = useRef<SVGRectElement>(null);
  const posLabelRef = useRef<HTMLSpanElement>(null);
  const durLabelRef = useRef<HTMLSpanElement>(null);
  const [dragging, setDragging] = useState(false);
  const dragFraction = useRef(0);
  // Unique per instance: the player bar and the full-screen player both mount
  // a SeekBar, and a duplicated clipPath id would let one clip the other.
  const clipId = `seek-${useId()}`;

  useEffect(() => {
    // Someone who has asked the OS to reduce motion gets the shape without the
    // movement, whatever the preference says.
    const still =
      !animated ||
      (typeof window !== "undefined" &&
        window.matchMedia("(prefers-reduced-motion: reduce)").matches);

    let raf: number;
    let phase = 0;
    let last = performance.now();

    const loop = (now: number) => {
      const elapsed = now - last;
      last = now;

      const duration = positionClock.duration();
      const pos = dragging ? dragFraction.current * duration : positionClock.now();
      const fraction = duration > 0 ? Math.min(1, pos / duration) : 0;

      if (fillRef.current) {
        fillRef.current.style.transform = `scaleX(${fraction})`;
      }
      if (clipRef.current) {
        clipRef.current.setAttribute("width", (fraction * VIEW_W).toFixed(1));
      }
      if (trackPathRef.current) {
        // Advance only while actually playing, so a pause freezes the artwork
        // the same way it freezes the position.
        if (!still && positionClock.playing()) phase += elapsed * WAVE_SPEED;
        const mid = metrics.height / 2;
        const d =
          style === "wave"
            ? wavePath(still ? 0 : phase, mid, metrics.amplitude)
            : barsPath(still ? 0 : phase, mid, metrics.amplitude, metrics.bars);
        trackPathRef.current.setAttribute("d", d);
        playedPathRef.current?.setAttribute("d", d);
      }
      if (posLabelRef.current) {
        posLabelRef.current.textContent = formatMs(pos);
      }
      if (durLabelRef.current) {
        durLabelRef.current.textContent = formatMs(duration);
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [dragging, style, animated, metrics.height, metrics.amplitude, metrics.bars]);

  function fractionFromEvent(e: { clientX: number }): number {
    const bar = barRef.current;
    if (!bar) return 0;
    const rect = bar.getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
  }

  function onPointerDown(e: React.PointerEvent) {
    if (!hasTrack) return;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    dragFraction.current = fractionFromEvent(e);
    setDragging(true);
  }

  function onPointerMove(e: React.PointerEvent) {
    if (!dragging) return;
    dragFraction.current = fractionFromEvent(e);
  }

  function onPointerUp(e: React.PointerEvent) {
    if (!dragging) return;
    setDragging(false);
    const duration = positionClock.duration();
    if (duration > 0) {
      seekMs(fractionFromEvent(e) * duration);
    }
  }

  return (
    <div className="flex w-full items-center gap-2 text-[11px] tabular-nums text-dim">
      <span ref={posLabelRef} className="w-10 text-right">
        0:00
      </span>
      <div
        ref={barRef}
        className={`group relative flex-1 ${hasTrack ? "cursor-pointer" : ""}`}
        style={{ height: metrics.height }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
      >
        {style === "line" ? (
          <div
            className="absolute top-1/2 w-full -translate-y-1/2 overflow-hidden rounded-full bg-surface-2"
            style={{ height: metrics.line }}
          >
            <div
              ref={fillRef}
              className="h-full w-full origin-left rounded-full bg-text transition-colors group-hover:bg-accent"
              style={{ transform: "scaleX(0)" }}
            />
          </div>
        ) : (
          <svg
            className="absolute inset-0 h-full w-full overflow-hidden"
            viewBox={`0 0 ${VIEW_W} ${metrics.height}`}
            preserveAspectRatio="none"
            aria-hidden="true"
          >
            <defs>
              <clipPath id={clipId}>
                <rect ref={clipRef} x="0" y="0" width="0" height={metrics.height} />
              </clipPath>
            </defs>
            <path
              ref={trackPathRef}
              className="text-surface-2"
              fill="none"
              stroke="currentColor"
              strokeWidth={metrics.stroke}
              strokeLinecap="round"
              vectorEffect="non-scaling-stroke"
            />
            <path
              ref={playedPathRef}
              className="text-text transition-colors group-hover:text-accent"
              clipPath={`url(#${clipId})`}
              fill="none"
              stroke="currentColor"
              strokeWidth={metrics.stroke}
              strokeLinecap="round"
              vectorEffect="non-scaling-stroke"
            />
          </svg>
        )}
      </div>
      <span ref={durLabelRef} className="w-10">
        0:00
      </span>
    </div>
  );
}
