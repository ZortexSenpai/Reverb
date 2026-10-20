// The queue brain. The Rust engine only ever knows the current track and one
// preloaded next track; everything else (ordering, shuffle, repeat, context
// semantics) lives here as plain array operations.

import { create } from "zustand";
import { nanoid } from "nanoid";
import { cmd } from "../lib/ipc/commands";
import { positionClock } from "../lib/playback/positionClock";
import type { PlaybackState, PlayableTrack, SignalPath, Song } from "../lib/subsonic/types";
import { usePrefsStore } from "./prefsStore";

export interface QueueItem {
  queueId: string;
  song: Song;
}

export type RepeatMode = "off" | "all" | "one";

interface PlayerStore {
  // Engine-mirrored state (written by the engine bridge).
  state: PlaybackState;
  currentTrackId: string | null;
  durationMs: number;
  volume: number;
  sampleRate: number;
  signalPath: SignalPath | null;

  // Frontend-owned queue.
  queue: QueueItem[];
  queueIndex: number;
  shuffle: boolean;
  repeat: RepeatMode;
  /** queueId order before shuffle was enabled, to restore on disable. */
  unshuffledOrder: string[] | null;
  /** queueId last handed to the engine as "next" (gapless target). */
  pendingNextQueueId: string | null;
  /** The queue is a song radio, and tops itself up as it runs out. */
  radio: boolean;
  /** Where to pick a restored song up the first time it plays. */
  resumeAt: { queueId: string; positionMs: number } | null;

  // Derived helper.
  currentItem: () => QueueItem | null;

  // Actions.
  playContext: (songs: Song[], startIndex: number, opts?: { shuffle?: boolean }) => void;
  /**
   * Play `song` now and fill the queue with songs like it. Resolves to how
   * many were added, or null if something else started playing meanwhile.
   */
  startRadio: (song: Song) => Promise<number | null>;
  playNext: (songs: Song[]) => void;
  addToQueue: (songs: Song[]) => void;
  removeFromQueue: (queueId: string) => void;
  moveInQueue: (from: number, to: number) => void;
  /** Replace everything after the current track (drag-reorder in the queue panel). */
  setUpcoming: (items: QueueItem[]) => void;
  jumpTo: (queueId: string) => void;
  /** Drop everything except the playing track (if any). */
  clearQueue: () => void;
  /** Empty the queue entirely and stop playback (sign-out). */
  resetQueue: () => void;
  next: () => void;
  previous: () => void;
  togglePlay: () => void;
  seekMs: (ms: number) => void;
  setVolume: (v: number) => void;
  toggleMute: () => void;
  toggleShuffle: () => void;
  cycleRepeat: () => void;
  /** Load a queue stopped; `positionMs` resumes its current song there. */
  restoreQueue: (items: QueueItem[], index: number, positionMs?: number) => void;
  /** Recompute and push the gapless "next" (also kicks radio mode). */
  resyncNext: () => void;
  /**
   * Mirror a star onto every queued copy of a song.
   *
   * Queue items hold a snapshot of the song taken when it was queued, so they
   * do not see the react-query cache patch the star mutation does — without
   * this the player bar and full-screen player keep showing the old heart
   * even though the server was updated.
   */
  setSongStarred: (songId: string, starred: string | undefined) => void;
}

function toPlayable(song: Song): PlayableTrack {
  return {
    trackId: song.id,
    suffix: song.suffix,
    durationMs: song.duration != null ? song.duration * 1000 : undefined,
    title: song.title,
    artist: song.artist,
    // `artists` is the OpenSubsonic enumeration of the credit; `artist` is the
    // formatted string it was joined into. Servers without the extension send
    // only the string, and the first artist stays unknown.
    firstArtist: song.artists?.[0]?.name,
    album: song.album,
    replayGain: song.replayGain,
  };
}

function shuffled<T>(items: T[]): T[] {
  const out = [...items];
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

export const usePlayerStore = create<PlayerStore>((set, get) => {
  /** Compute the item after `index` under current repeat rules. */
  function nextItemAfter(index: number): QueueItem | null {
    const { queue, repeat } = get();
    if (queue.length === 0) return null;
    if (repeat === "one") return queue[index] ?? null;
    if (index + 1 < queue.length) return queue[index + 1];
    return repeat === "all" ? queue[0] : null;
  }

  /** Tell the engine what plays after the current track. */
  function syncNext() {
    const { queueIndex } = get();
    const next = nextItemAfter(queueIndex);
    set({ pendingNextQueueId: next?.queueId ?? null });
    void cmd("player_set_next", { track: next ? toPlayable(next.song) : null });
    if (!next) void maybeExtendRadio();
  }

  /** Radio: when the queue is about to run dry, append songs like the last. */
  async function maybeExtendRadio() {
    if (radioFetchInFlight) return;
    if (!usePrefsStore.getState().prefs.radioMode && !get().radio) return;
    const { queue, queueIndex, repeat } = get();
    const current = queue[queueIndex];
    if (!current || repeat !== "off") return;
    radioFetchInFlight = true;
    try {
      const similar = await cmd("library_get_radio", {
        id: current.song.id,
        genre: current.song.genre,
        count: 20,
      });
      const seen = new Set(get().queue.map((it) => it.song.id));
      const fresh = similar.filter((s) => !seen.has(s.id));
      if (fresh.length === 0) return;
      const items = fresh.map((song) => ({ queueId: nanoid(), song }));
      set({ queue: [...get().queue, ...items] });
      syncNext();
    } catch {
      // No Last.fm agent or network hiccup — radio just doesn't extend.
    } finally {
      radioFetchInFlight = false;
    }
  }

  function playItem(item: QueueItem) {
    const resumeAt = get().resumeAt;
    const startMs = resumeAt?.queueId === item.queueId ? resumeAt.positionMs : 0;
    set({ currentTrackId: item.song.id, resumeAt: null });
    positionClock.reset((item.song.duration ?? 0) * 1000);
    void cmd("player_play_track", { track: toPlayable(item.song) }).then(() => {
      // The engine takes commands in order, so this lands on the new track.
      if (startMs > 0) get().seekMs(startMs);
      syncNext();
    });
  }

  return {
    state: "stopped",
    currentTrackId: null,
    durationMs: 0,
    volume: 1,
    sampleRate: 48000,
    signalPath: null,

    queue: [],
    queueIndex: -1,
    shuffle: false,
    repeat: "off",
    unshuffledOrder: null,
    pendingNextQueueId: null,
    radio: false,
    resumeAt: null,

    currentItem: () => {
      const { queue, queueIndex } = get();
      return queue[queueIndex] ?? null;
    },

    playContext: (songs, startIndex, opts) => {
      if (songs.length === 0) return;
      let items: QueueItem[] = songs.map((song) => ({ queueId: nanoid(), song }));
      // Captured before the shuffle below reorders `items`, so disabling
      // shuffle can put the queue back into the order it was handed to us in.
      const originalOrder = items.map((it) => it.queueId);
      let index = startIndex;
      const useShuffle = opts?.shuffle ?? get().shuffle;
      if (useShuffle) {
        // The chosen track leads; the rest are shuffled behind it.
        const chosen = items[startIndex];
        const rest = shuffled(items.filter((it) => it !== chosen));
        items = [chosen, ...rest];
        index = 0;
      }
      set({
        queue: items,
        queueIndex: index,
        shuffle: useShuffle,
        unshuffledOrder: useShuffle ? originalOrder : null,
        radio: false,
      });
      playItem(items[index]);
    },

    startRadio: async (song) => {
      get().playContext([song], 0, { shuffle: false });
      const seed = get().queue[0];
      set({ radio: true });
      let songs: Song[] = [];
      try {
        songs = await cmd("library_get_radio", { id: song.id, genre: song.genre, count: RADIO_SIZE });
      } catch {
        // Treated like finding nothing.
      }
      const { queue, radio } = get();
      if (!radio || queue[0]?.queueId !== seed.queueId) return null;
      const seen = new Set(queue.map((it) => it.song.id));
      const items = songs
        .filter((s) => !seen.has(s.id))
        .map((s) => ({ queueId: nanoid(), song: s }));
      if (items.length > 0) {
        set({ queue: [...queue, ...items] });
        syncNext();
      }
      return items.length;
    },

    playNext: (songs) => {
      const items = songs.map((song) => ({ queueId: nanoid(), song }));
      const { queue, queueIndex } = get();
      const at = queueIndex < 0 ? queue.length : queueIndex + 1;
      const nextQueue = [...queue.slice(0, at), ...items, ...queue.slice(at)];
      set({ queue: nextQueue });
      if (queueIndex < 0 && nextQueue.length > 0) {
        set({ queueIndex: 0 });
        playItem(nextQueue[0]);
      } else {
        syncNext();
      }
    },

    addToQueue: (songs) => {
      const items = songs.map((song) => ({ queueId: nanoid(), song }));
      const { queue, queueIndex } = get();
      const nextQueue = [...queue, ...items];
      set({ queue: nextQueue });
      if (queueIndex < 0 && nextQueue.length > 0) {
        set({ queueIndex: 0 });
        playItem(nextQueue[0]);
      } else {
        syncNext();
      }
    },

    removeFromQueue: (queueId) => {
      const { queue, queueIndex } = get();
      const idx = queue.findIndex((it) => it.queueId === queueId);
      if (idx === -1) return;
      if (idx === queueIndex) {
        // Removing the playing track: advance to what would come next.
        const wasLast = idx === queue.length - 1;
        const nextQueue = queue.filter((it) => it.queueId !== queueId);
        if (nextQueue.length === 0 || wasLast) {
          set({ queue: nextQueue, queueIndex: -1, currentTrackId: null });
          void cmd("player_stop");
        } else {
          set({ queue: nextQueue, queueIndex: idx });
          playItem(nextQueue[idx]);
        }
        return;
      }
      const nextQueue = queue.filter((it) => it.queueId !== queueId);
      set({ queue: nextQueue, queueIndex: idx < queueIndex ? queueIndex - 1 : queueIndex });
      syncNext();
    },

    setUpcoming: (items) => {
      const { queue, queueIndex } = get();
      set({ queue: [...queue.slice(0, queueIndex + 1), ...items] });
      syncNext();
    },

    moveInQueue: (from, to) => {
      const { queue, queueIndex } = get();
      if (from === to || from < 0 || from >= queue.length || to < 0 || to >= queue.length) return;
      const nextQueue = [...queue];
      const [moved] = nextQueue.splice(from, 1);
      nextQueue.splice(to, 0, moved);
      const currentId = queue[queueIndex]?.queueId;
      const newIndex = currentId ? nextQueue.findIndex((it) => it.queueId === currentId) : -1;
      set({ queue: nextQueue, queueIndex: newIndex });
      syncNext();
    },

    jumpTo: (queueId) => {
      const { queue } = get();
      const idx = queue.findIndex((it) => it.queueId === queueId);
      if (idx === -1) return;
      set({ queueIndex: idx });
      playItem(queue[idx]);
    },

    clearQueue: () => {
      const current = get().currentItem();
      if (!current) {
        get().resetQueue();
        return;
      }
      // Clearing a radio ends it, rather than letting it refill at once.
      set({ queue: [current], queueIndex: 0, unshuffledOrder: null, radio: false });
      syncNext();
    },

    resetQueue: () => {
      set({
        queue: [],
        queueIndex: -1,
        currentTrackId: null,
        pendingNextQueueId: null,
        radio: false,
        resumeAt: null,
      });
      void cmd("player_stop");
    },

    next: () => {
      const { queueIndex } = get();
      const next = nextItemAfter(queueIndex === -1 ? 0 : queueIndex);
      if (!next) return;
      const idx = get().queue.findIndex((it) => it.queueId === next.queueId);
      set({ queueIndex: idx });
      playItem(next);
    },

    previous: () => {
      const { queue, queueIndex, repeat } = get();
      if (queueIndex < 0) return;
      if (positionClock.now() > 3000) {
        get().seekMs(0);
        return;
      }
      let idx = queueIndex - 1;
      if (idx < 0) idx = repeat === "all" ? queue.length - 1 : 0;
      set({ queueIndex: idx });
      playItem(queue[idx]);
    },

    togglePlay: () => {
      const { state, queue, queueIndex } = get();
      if (state === "stopped") {
        if (queue.length > 0) {
          const idx = queueIndex >= 0 ? queueIndex : 0;
          set({ queueIndex: idx });
          playItem(queue[idx]);
        }
        return;
      }
      void cmd("player_toggle");
    },

    seekMs: (ms) => {
      positionClock.seekTo(ms);
      void cmd("player_seek", { positionMs: Math.max(0, Math.round(ms)) });
    },

    setVolume: (v) => {
      const volume = Math.min(1, Math.max(0, v));
      set({ volume });
      void cmd("player_set_volume", { volume });
    },

    toggleMute: () => {
      const { volume } = get();
      if (volume > 0) {
        lastAudibleVolume = volume;
        get().setVolume(0);
      } else {
        get().setVolume(lastAudibleVolume);
      }
    },

    toggleShuffle: () => {
      const { shuffle, queue, queueIndex, unshuffledOrder } = get();
      if (queue.length === 0) {
        set({ shuffle: !shuffle });
        return;
      }
      if (!shuffle) {
        // Shuffle only what's upcoming; history and current stay in place.
        const played = queue.slice(0, queueIndex + 1);
        const upcoming = shuffled(queue.slice(queueIndex + 1));
        set({
          shuffle: true,
          unshuffledOrder: queue.map((it) => it.queueId),
          queue: [...played, ...upcoming],
        });
      } else {
        const byId = new Map(queue.map((it) => [it.queueId, it]));
        const restored = (unshuffledOrder ?? [])
          .map((id) => byId.get(id))
          .filter((it): it is QueueItem => Boolean(it));
        const currentId = queue[queueIndex]?.queueId;
        const newIndex = currentId ? restored.findIndex((it) => it.queueId === currentId) : -1;
        set({
          shuffle: false,
          unshuffledOrder: null,
          queue: restored.length === queue.length ? restored : queue,
          queueIndex: restored.length === queue.length ? newIndex : queueIndex,
        });
      }
      syncNext();
    },

    cycleRepeat: () => {
      const order: RepeatMode[] = ["off", "all", "one"];
      const next = order[(order.indexOf(get().repeat) + 1) % order.length];
      set({ repeat: next });
      syncNext();
    },

    restoreQueue: (items, index, positionMs = 0) => {
      if (items.length === 0) return;
      const queueIndex = Math.min(Math.max(index, 0), items.length - 1);
      set({
        queue: items,
        queueIndex,
        radio: false,
        resumeAt: positionMs > 0 ? { queueId: items[queueIndex].queueId, positionMs } : null,
      });
      // Restored stopped: pressing play starts the remembered track.
    },

    resyncNext: () => syncNext(),

    setSongStarred: (songId, starred) => {
      const { queue } = get();
      if (!queue.some((item) => item.song.id === songId)) return;
      set({
        queue: queue.map((item) =>
          item.song.id === songId ? { ...item, song: { ...item.song, starred } } : item,
        ),
      });
    },
  };
});

let lastAudibleVolume = 0.7;
let radioFetchInFlight = false;
/** Songs a started radio queues up front. */
const RADIO_SIZE = 50;

// ---- queue persistence (debounced snapshot into settings.json) ----

interface QueueSnapshot {
  songs: Song[];
  index: number;
}

let saveTimer: ReturnType<typeof setTimeout> | null = null;

function saveSnapshot(): Promise<void> {
  saveTimer = null;
  if (!usePrefsStore.getState().prefs.saveQueue) return Promise.resolve();
  const { queue, queueIndex } = usePlayerStore.getState();
  const snapshot: QueueSnapshot | null =
    queue.length > 0 ? { songs: queue.map((it) => it.song), index: queueIndex } : null;
  return cmd("queue_save_snapshot", { snapshot });
}

usePlayerStore.subscribe((state, prev) => {
  if (state.queue === prev.queue && state.queueIndex === prev.queueIndex) return;
  if (!usePrefsStore.getState().prefs.saveQueue) return;
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => void saveSnapshot(), 2000);
});

/**
 * Save a queue change still waiting out the delay above, now. Nothing saves on
 * exit, so this is what keeps the last few seconds of queue edits when the app
 * is about to go away (installing an update).
 */
export async function saveQueueSnapshotNow(): Promise<void> {
  if (!saveTimer) return;
  clearTimeout(saveTimer);
  await saveSnapshot();
}

export async function restoreQueueFromSnapshot() {
  if (!usePrefsStore.getState().prefs.saveQueue) return;
  try {
    const raw = (await cmd("queue_load_snapshot")) as QueueSnapshot | null;
    if (raw && Array.isArray(raw.songs) && raw.songs.length > 0) {
      const items = raw.songs.map((song) => ({ queueId: nanoid(), song }));
      usePlayerStore.getState().restoreQueue(items, raw.index ?? 0);
    }
  } catch {
    // No snapshot or unreadable — start empty.
  }
}

/** Called by the engine bridge on a natural (gapless) track change. */
export function advanceToPendingNext() {
  const store = usePlayerStore.getState();
  const { pendingNextQueueId, queue } = store;
  if (!pendingNextQueueId) return;
  const idx = queue.findIndex((it) => it.queueId === pendingNextQueueId);
  if (idx === -1) return;
  usePlayerStore.setState({
    queueIndex: idx,
    currentTrackId: queue[idx].song.id,
  });
  // Re-sync what comes after the newly-current track (also feeds radio).
  store.resyncNext();
}
