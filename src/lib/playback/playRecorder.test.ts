import { beforeEach, describe, expect, it, vi } from "vitest";
import { queryClient } from "../queryClient";
import { qk } from "../subsonic/keys";
import type { Play, Song } from "../subsonic/types";
import { usePlayerStore, type QueueItem } from "../../stores/playerStore";
import { usePrefsStore } from "../../stores/prefsStore";
import { recordPlay } from "./playRecorder";

// No Tauri host in the test run: keep every IPC call, so the tests can see
// what would have been recorded.
const { invoke } = vi.hoisted(() => ({
  invoke: vi.fn<(name: string, args?: unknown) => Promise<unknown>>(() =>
    Promise.resolve(undefined),
  ),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const song = (id: string): Song => ({ id, title: `Song ${id}`, coverArt: `cover-${id}` });
const item = (queueId: string, s: Song): QueueItem => ({ queueId, song: s });

const recordedPlays = () => invoke.mock.calls.filter(([name]) => name === "history_record");

beforeEach(() => {
  // The queue-persistence subscriber would save a snapshot on every queue change.
  usePrefsStore.setState({ prefs: { ...usePrefsStore.getState().prefs, saveQueue: false } });
  usePlayerStore.setState({ queue: [], queueIndex: -1 });
  invoke.mockClear();
});

describe("recordPlay", () => {
  it("records the current Song with when it started", async () => {
    usePlayerStore.setState({
      queue: [item("q1", song("a")), item("q2", song("b")), item("q3", song("c"))],
      queueIndex: 1,
    });

    await recordPlay({ trackId: "b", startedAtMs: 1_761_134_400_000 });

    expect(recordedPlays()).toEqual([
      ["history_record", { song: song("b"), playedAt: 1_761_134_400_000 }],
    ]);
  });

  it("records nothing when the current item is a different Song", async () => {
    usePlayerStore.setState({
      queue: [item("q1", song("a")), item("q2", song("b"))],
      queueIndex: 0,
    });

    await recordPlay({ trackId: "b", startedAtMs: 1_761_134_400_000 });

    expect(recordedPlays()).toEqual([]);
  });

  it("records nothing when the Queue has no current item", async () => {
    usePlayerStore.setState({ queue: [], queueIndex: -1 });

    await recordPlay({ trackId: "b", startedAtMs: 1_761_134_400_000 });

    expect(recordedPlays()).toEqual([]);
  });

  it("refreshes the Play History once the Play is recorded, so an open tab shows it", async () => {
    usePlayerStore.setState({ queue: [item("q1", song("a"))], queueIndex: 0 });
    queryClient.setQueryData<Play[]>(qk.playHistory(), []);

    await recordPlay({ trackId: "a", startedAtMs: 1_761_134_400_000 });

    expect(queryClient.getQueryState(qk.playHistory())?.isInvalidated).toBe(true);
  });

  it("leaves the Play History alone when nothing was recorded", async () => {
    usePlayerStore.setState({ queue: [item("q1", song("a"))], queueIndex: 0 });
    queryClient.setQueryData<Play[]>(qk.playHistory(), []);

    await recordPlay({ trackId: "b", startedAtMs: 1_761_134_400_000 });

    expect(queryClient.getQueryState(qk.playHistory())?.isInvalidated).toBe(false);
  });
});
