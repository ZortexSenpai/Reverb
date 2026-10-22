import { beforeEach, describe, expect, it, vi } from "vitest";
import { readPlayHistory } from "./queries";
import type { Play } from "./types";

// No Tauri host in the test run: answer every IPC call from the mock.
const { invoke } = vi.hoisted(() => ({
  invoke: vi.fn<(name: string, args?: unknown) => Promise<unknown>>(() =>
    Promise.resolve(undefined),
  ),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const play = (id: string, playedAt: number): Play => ({
  playedAt,
  song: { id, title: `Song ${id}` },
});

beforeEach(() => {
  invoke.mockClear();
});

describe("readPlayHistory", () => {
  it("reads the signed-in account's Plays, newest first as stored", async () => {
    const plays = [play("b", 1_761_134_500_000), play("a", 1_761_134_400_000)];
    invoke.mockResolvedValueOnce(plays);

    expect(await readPlayHistory()).toEqual(plays);
    expect(invoke).toHaveBeenCalledWith("history_get", {});
  });

  // So the Recently played tab says "Nothing played yet" rather than staying blank.
  it("reads a Play History it can't get as empty, and logs why", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const failure = { code: "internal", message: "history_get is not a command" };
    invoke.mockRejectedValueOnce(failure);

    expect(await readPlayHistory()).toEqual([]);
    expect(warn).toHaveBeenCalledWith("could not read the Play History:", failure);
    warn.mockRestore();
  });
});
