import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Song } from "../lib/subsonic/types";
import { usePlayerStore, type QueueItem } from "./playerStore";
import { usePrefsStore } from "./prefsStore";

// Queue actions push the engine commands out over IPC; there is no Tauri host
// in the test run, so swallow them.
vi.mock("@tauri-apps/api/core", () => ({ invoke: () => Promise.resolve(undefined) }));

const song = (id: string, starred?: string): Song => ({ id, title: `Song ${id}`, starred });
const item = (queueId: string, s: Song): QueueItem => ({ queueId, song: s });

function setQueue(items: QueueItem[]) {
  usePlayerStore.setState({ queue: items, queueIndex: 0 });
}

beforeEach(() => {
  // The queue-persistence subscriber debounces a `queue_save_snapshot` IPC call
  // on every queue change; there is no Tauri host here, so keep it switched off.
  usePrefsStore.setState({ prefs: { ...usePrefsStore.getState().prefs, saveQueue: false } });
  setQueue([]);
});

describe("setSongStarred", () => {
  it("stamps the song wherever it sits in the queue", () => {
    setQueue([item("q1", song("a")), item("q2", song("b"))]);
    usePlayerStore.getState().setSongStarred("b", "2026-08-14T10:00:00Z");

    const { queue } = usePlayerStore.getState();
    expect(queue.map((i) => i.song.starred)).toEqual([undefined, "2026-08-14T10:00:00Z"]);
  });

  it("updates every copy — the same song can be queued more than once", () => {
    setQueue([item("q1", song("a")), item("q2", song("b")), item("q3", song("a"))]);
    usePlayerStore.getState().setSongStarred("a", "2026-08-14T10:00:00Z");

    const { queue } = usePlayerStore.getState();
    expect(queue.map((i) => i.song.starred)).toEqual([
      "2026-08-14T10:00:00Z",
      undefined,
      "2026-08-14T10:00:00Z",
    ]);
  });

  it("clears the stamp when unstarring", () => {
    setQueue([item("q1", song("a", "2026-08-14T10:00:00Z"))]);
    usePlayerStore.getState().setSongStarred("a", undefined);

    expect(usePlayerStore.getState().queue[0].song.starred).toBeUndefined();
  });

  it("keeps the queue identity when the song is not queued, so nothing re-renders", () => {
    const queue = [item("q1", song("a"))];
    setQueue(queue);
    usePlayerStore.getState().setSongStarred("not-here", "2026-08-14T10:00:00Z");

    expect(usePlayerStore.getState().queue).toBe(queue);
  });

  it("replaces only the items it touches, leaving the others by reference", () => {
    const untouched = item("q2", song("b"));
    setQueue([item("q1", song("a")), untouched]);
    usePlayerStore.getState().setSongStarred("a", "2026-08-14T10:00:00Z");

    expect(usePlayerStore.getState().queue[1]).toBe(untouched);
  });
});

describe("playContext", () => {
  const songs = ["a", "b", "c", "d", "e", "f"].map((id) => song(id));

  it("leads with the chosen track and shuffles the rest behind it", () => {
    usePlayerStore.getState().playContext(songs, 3, { shuffle: true });

    const { queue, queueIndex } = usePlayerStore.getState();
    expect(queueIndex).toBe(0);
    expect(queue[0].song.id).toBe("d");
    expect(queue.map((i) => i.song.id).sort()).toEqual(["a", "b", "c", "d", "e", "f"]);
  });

  it("remembers the pre-shuffle order so switching shuffle off restores it", () => {
    usePlayerStore.getState().playContext(songs, 3, { shuffle: true });
    usePlayerStore.getState().toggleShuffle();

    const { queue, queueIndex } = usePlayerStore.getState();
    expect(queue.map((i) => i.song.id)).toEqual(["a", "b", "c", "d", "e", "f"]);
    expect(queue[queueIndex].song.id).toBe("d");
  });
});
