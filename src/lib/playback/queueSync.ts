// Keeps the play queue on the server too (Subsonic savePlayQueue), so another
// client — Reverb on another computer, or a phone app — can pick it up, and
// picks up a queue another client saved there since.
//
// Saving happens on queue changes (debounced), on pause and stop, and every
// so often while playing, for the position. Loading happens at startup and
// when the window regains focus, and only while nothing is loaded here:
// taking over a queue never interrupts what this device is doing.

import { nanoid } from "nanoid";
import { cmd } from "../ipc/commands";
import { usePlayerStore } from "../../stores/playerStore";
import { usePrefsStore } from "../../stores/prefsStore";
import { positionClock } from "./positionClock";

const SAVE_DEBOUNCE_MS = 3000;
/** While playing, the position is saved this often. */
const POSITION_EVERY_MS = 30_000;
/** Focus changes ask the server at most this often. */
const CHECK_EVERY_MS = 30_000;
/** A server save this close to our own last one is our own (clocks drift). */
const CLOCK_SLACK_MS = 5000;
const LAST_SYNC_KEY = "reverb.queueSync.lastSync";

/** When the newest queue we saved or took over was saved. */
let lastSync = readLastSync();
let started = false;
let adopting = false;
let saveTimer: ReturnType<typeof setTimeout> | null = null;
let lastCheck = 0;

function enabled(): boolean {
  return usePrefsStore.getState().prefs.syncQueue;
}

function readLastSync(): number {
  try {
    return Number(localStorage.getItem(LAST_SYNC_KEY)) || 0;
  } catch {
    return 0;
  }
}

function setLastSync(ms: number) {
  lastSync = ms;
  try {
    localStorage.setItem(LAST_SYNC_KEY, String(ms));
  } catch {
    // Kept in memory for this session.
  }
}

async function save() {
  saveTimer = null;
  if (!enabled()) return;
  const { queue, queueIndex } = usePlayerStore.getState();
  // The queue only empties on sign-out or when everything is removed; neither
  // should wipe what another device could still resume.
  if (queue.length === 0) return;
  const current = queue[queueIndex]?.song.id ?? null;
  try {
    await cmd("library_save_play_queue", {
      ids: queue.map((it) => it.song.id),
      current,
      positionMs: current ? Math.round(positionClock.now()) : 0,
    });
    setLastSync(Date.now());
  } catch {
    // Offline or signed out: the next change tries again.
  }
}

function scheduleSave(delayMs: number) {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => void save(), delayMs);
}

/**
 * Take over the server's queue when another client saved a newer one and
 * nothing is loaded here. Resolves to whether it did.
 */
export async function adoptServerQueue(): Promise<boolean> {
  if (!enabled() || usePlayerStore.getState().state !== "stopped") return false;
  lastCheck = Date.now();
  let remote;
  try {
    remote = await cmd("library_get_play_queue");
  } catch {
    return false;
  }
  if (!remote || remote.entry.length === 0) return false;
  const changed = remote.changed ? Date.parse(remote.changed) : NaN;
  if (!Number.isFinite(changed) || changed <= lastSync + CLOCK_SLACK_MS) return false;

  const player = usePlayerStore.getState();
  // Playback may have started while the server answered.
  if (player.state !== "stopped") return false;
  const entry = remote.entry;
  const index = Math.max(0, entry.findIndex((song) => song.id === remote.current));
  setLastSync(changed);
  const same =
    player.queueIndex === index &&
    player.queue.length === entry.length &&
    player.queue.every((it, i) => it.song.id === entry[i].id);
  if (same) return false;

  // Transcoded streams can't seek, so those start from the top.
  const song = entry[index];
  const seekable =
    usePrefsStore.getState().prefs.streamingFormat === "raw" &&
    song.suffix?.toLowerCase() !== "opus";
  adopting = true;
  try {
    player.restoreQueue(
      entry.map((s) => ({ queueId: nanoid(), song: s })),
      index,
      seekable ? (remote.position ?? 0) : 0,
    );
  } finally {
    adopting = false;
  }
  return true;
}

/** Start syncing; call once the local queue has been restored. */
export async function startQueueSync() {
  if (started) return;
  started = true;
  await adoptServerQueue();

  usePlayerStore.subscribe((state, prev) => {
    // Saving a queue straight back to where it came from is pointless.
    if (adopting || !enabled()) return;
    if (state.queue !== prev.queue || state.queueIndex !== prev.queueIndex) {
      scheduleSave(SAVE_DEBOUNCE_MS);
    } else if (
      state.state !== prev.state &&
      (state.state === "paused" || state.state === "stopped")
    ) {
      // Pausing is when another device is most likely to take over.
      scheduleSave(0);
    }
  });
  setInterval(() => {
    if (enabled() && usePlayerStore.getState().state === "playing") void save();
  }, POSITION_EVERY_MS);
  window.addEventListener("focus", () => {
    if (Date.now() - lastCheck > CHECK_EVERY_MS) void adoptServerQueue();
  });
}
