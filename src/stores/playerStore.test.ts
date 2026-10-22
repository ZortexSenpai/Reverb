import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Song } from "../lib/subsonic/types";
import { saveQueueSnapshotNow, usePlayerStore, type QueueItem } from "./playerStore";
import { usePrefsStore } from "./prefsStore";

// Queue actions push the engine commands out over IPC; there is no Tauri host
// in the test run, so swallow them (and keep them, for the tests that care).
const { invoke } = vi.hoisted(() => ({
  invoke: vi.fn<(name: string, args?: unknown) => Promise<unknown>>(() =>
    Promise.resolve(undefined),
  ),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

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

const playedTrackIds = () =>
  invoke.mock.calls
    .filter(([name]) => name === "player_play_track")
    .map(([, args]) => (args as { track: { trackId: string } }).track.trackId);

describe("playNext", () => {
  beforeEach(() => {
    invoke.mockClear();
  });

  it("slots the Songs in after the current item, in order, and leaves it playing", () => {
    usePlayerStore.setState({
      queue: [item("q1", song("a")), item("q2", song("b")), item("q3", song("c"))],
      queueIndex: 1,
    });

    usePlayerStore.getState().playNext([song("x"), song("y")]);

    const { queue, queueIndex } = usePlayerStore.getState();
    expect(queue.map((i) => i.song.id)).toEqual(["a", "b", "x", "y", "c"]);
    expect(queueIndex).toBe(1);
    expect(playedTrackIds()).toEqual([]);
  });

  it("starts an empty Queue with the first of the Songs", () => {
    usePlayerStore.setState({ queue: [], queueIndex: -1 });

    usePlayerStore.getState().playNext([song("x"), song("y")]);

    const { queue, queueIndex } = usePlayerStore.getState();
    expect(queue.map((i) => i.song.id)).toEqual(["x", "y"]);
    expect(queueIndex).toBe(0);
    expect(playedTrackIds()).toEqual(["x"]);
  });
});

describe("playNow", () => {
  beforeEach(() => {
    invoke.mockClear();
  });

  it("inserts the Song after the current item and plays it", () => {
    usePlayerStore.setState({
      queue: [item("q1", song("a")), item("q2", song("b")), item("q3", song("c"))],
      queueIndex: 1,
    });

    usePlayerStore.getState().playNow(song("x"));

    const { queue, queueIndex } = usePlayerStore.getState();
    expect(queue.map((i) => i.song.id)).toEqual(["a", "b", "x", "c"]);
    expect(queueIndex).toBe(2);
    expect(usePlayerStore.getState().currentItem()?.song.id).toBe("x");
    expect(playedTrackIds()).toEqual(["x"]);
  });

  it("keeps every other item in its place, both before and after", () => {
    usePlayerStore.setState({
      queue: [
        item("q1", song("a")),
        item("q2", song("b")),
        item("q3", song("c")),
        item("q4", song("d")),
      ],
      queueIndex: 1,
    });

    usePlayerStore.getState().playNow(song("a"));

    const { queue } = usePlayerStore.getState();
    expect(queue).toHaveLength(5);
    expect(queue[0].queueId).toBe("q1");
    expect(queue[1].queueId).toBe("q2");
    expect(queue[3].queueId).toBe("q3");
    expect(queue[4].queueId).toBe("q4");
    // A Song already in the Queue is played as a copy, leaving the original where it was.
    expect(queue[2].queueId).not.toBe("q1");
    expect(queue[2].song.id).toBe("a");
  });

  it("makes the Song the only item of an empty Queue and plays it", () => {
    usePlayerStore.setState({ queue: [], queueIndex: -1 });

    usePlayerStore.getState().playNow(song("x"));

    const { queue, queueIndex } = usePlayerStore.getState();
    expect(queue.map((i) => i.song.id)).toEqual(["x"]);
    expect(queueIndex).toBe(0);
    expect(playedTrackIds()).toEqual(["x"]);
  });
});

describe("saveQueueSnapshotNow", () => {
  const setSaveQueue = (saveQueue: boolean) =>
    usePrefsStore.setState({ prefs: { ...usePrefsStore.getState().prefs, saveQueue } });
  const snapshotSaves = () => invoke.mock.calls.filter(([name]) => name === "queue_save_snapshot");

  beforeEach(() => {
    vi.useFakeTimers();
    invoke.mockClear();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("saves a queue change right away instead of 2 seconds later", async () => {
    setSaveQueue(true);
    setQueue([item("q1", song("a")), item("q2", song("b"))]);
    expect(snapshotSaves()).toEqual([]);

    await saveQueueSnapshotNow();

    expect(snapshotSaves()).toEqual([
      ["queue_save_snapshot", { snapshot: { songs: [song("a"), song("b")], index: 0 } }],
    ]);
  });

  it("leaves nothing for the delayed save to repeat", async () => {
    setSaveQueue(true);
    setQueue([item("q1", song("a"))]);

    await saveQueueSnapshotNow();
    vi.advanceTimersByTime(5000);

    expect(snapshotSaves()).toHaveLength(1);
  });

  it("waits for a delayed save that is already under way", async () => {
    let finishSave!: () => void;
    invoke.mockImplementationOnce(
      () => new Promise((resolve) => (finishSave = () => resolve(undefined))),
    );
    setSaveQueue(true);
    setQueue([item("q1", song("a"))]);
    vi.advanceTimersByTime(2000);

    let saved = false;
    const saving = saveQueueSnapshotNow().then(() => (saved = true));
    await vi.advanceTimersByTimeAsync(0);
    expect(saved).toBe(false);

    finishSave();
    await saving;
    expect(snapshotSaves()).toHaveLength(1);
  });

  it("does nothing when 'save queue' is off", async () => {
    setSaveQueue(false);
    setQueue([item("q1", song("a"))]);

    await saveQueueSnapshotNow();
    vi.advanceTimersByTime(5000);

    expect(snapshotSaves()).toEqual([]);
  });
});
