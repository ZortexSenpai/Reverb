// Smooth playback position between the engine's 500 ms ticks, without any
// React re-renders: the SeekBar reads this clock from a rAF loop.

interface ClockState {
  basisMs: number;
  basisAt: number; // performance.now() when basisMs was true
  durationMs: number;
  playing: boolean;
  /** Bumped on seek; engine ticks from before the seek are ignored. */
  seekGeneration: number;
  pendingSeekUntil: number; // timestamp; ignore engine ticks until then
}

const clock: ClockState = {
  basisMs: 0,
  basisAt: performance.now(),
  durationMs: 0,
  playing: false,
  seekGeneration: 0,
  pendingSeekUntil: 0,
};

export const positionClock = {
  /** Engine tick arrived. */
  onEngineTick(positionMs: number, durationMs: number) {
    // After an optimistic seek, ignore stale ticks briefly so the bar
    // doesn't snap back before the engine catches up.
    if (performance.now() < clock.pendingSeekUntil) {
      clock.durationMs = durationMs;
      return;
    }
    clock.basisMs = positionMs;
    clock.basisAt = performance.now();
    clock.durationMs = durationMs;
  },

  setPlaying(playing: boolean) {
    // Re-anchor so pauses freeze the display.
    clock.basisMs = positionClock.now();
    clock.basisAt = performance.now();
    clock.playing = playing;
  },

  /** Optimistically jump the display; engine ticks resync shortly after. */
  seekTo(positionMs: number) {
    clock.basisMs = positionMs;
    clock.basisAt = performance.now();
    clock.seekGeneration++;
    clock.pendingSeekUntil = performance.now() + 700;
  },

  reset(durationMs = 0) {
    clock.basisMs = 0;
    clock.basisAt = performance.now();
    clock.durationMs = durationMs;
    clock.pendingSeekUntil = 0;
  },

  now(): number {
    const elapsed = clock.playing ? performance.now() - clock.basisAt : 0;
    const pos = clock.basisMs + elapsed;
    return clock.durationMs > 0 ? Math.min(pos, clock.durationMs) : pos;
  },

  duration(): number {
    return clock.durationMs;
  },

  /**
   * Whether playback is running. Exposed so the seek bar's rAF loop can freeze
   * its animation on pause by reading the clock directly — subscribing to the
   * player store would re-render the component instead.
   */
  playing(): boolean {
    return clock.playing;
  },
};
