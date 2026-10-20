import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createUpdateController,
  type FoundUpdate,
  type UpdateControllerDeps,
} from "./updateController";

const SIX_HOURS = 6 * 60 * 60 * 1000;

/** The "check for updates automatically" setting, flippable from the test. */
function fakeSetting(initial: boolean) {
  let value = initial;
  const listeners = new Set<(on: boolean) => void>();
  return {
    get: () => value,
    subscribe: (listener: (on: boolean) => void) => {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
    set: (next: boolean) => {
      value = next;
      for (const listener of listeners) listener(next);
    },
  };
}

function fakeUpdate(version = "1.0.1"): FoundUpdate {
  return {
    version,
    notes: `## ${version}\n\n- Fixes`,
    date: "2026-10-21T12:00:00Z",
    download: vi.fn(() => Promise.resolve()),
    install: vi.fn(() => Promise.resolve()),
  };
}

function setup(overrides: Partial<UpdateControllerDeps> = {}, autoCheck = true) {
  const setting = fakeSetting(autoCheck);
  const deps: UpdateControllerDeps = {
    enabled: true,
    check: vi.fn(() => Promise.resolve(null)),
    relaunch: vi.fn(() => Promise.resolve()),
    saveQueue: vi.fn(() => Promise.resolve()),
    autoCheck: setting,
    ...overrides,
  };
  const controller = createUpdateController(deps);
  const statuses: string[] = [];
  controller.subscribe((state) => statuses.push(state.status));
  return { controller, deps, setting, statuses };
}

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

describe("automatic checks", () => {
  it("checks at start when automatic checks are on", async () => {
    const { controller, deps } = setup();

    controller.start();
    await vi.advanceTimersByTimeAsync(0);

    expect(deps.check).toHaveBeenCalledTimes(1);
  });

  it("checks again every 6 hours while nothing new is out", async () => {
    const { controller, deps } = setup();

    controller.start();
    await vi.advanceTimersByTimeAsync(SIX_HOURS - 1);
    expect(deps.check).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(deps.check).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(SIX_HOURS);
    expect(deps.check).toHaveBeenCalledTimes(3);
  });

  it("treats a failed check as nothing found, and tries again at the next interval", async () => {
    const check = vi
      .fn<() => Promise<FoundUpdate | null>>()
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValue(fakeUpdate());
    const { controller, statuses } = setup({ check });

    controller.start();
    await vi.advanceTimersByTimeAsync(0);
    expect(statuses).toEqual(["checking", "idle"]);

    await vi.advanceTimersByTimeAsync(SIX_HOURS);
    expect(controller.getState().status).toBe("ready");
  });

  it("drops a failed background download silently, and tries again at the next interval", async () => {
    const flaky = fakeUpdate();
    vi.mocked(flaky.download).mockRejectedValueOnce(new Error("connection reset"));
    const { controller, statuses } = setup({ check: vi.fn(() => Promise.resolve(flaky)) });

    controller.start();
    await vi.advanceTimersByTimeAsync(0);
    expect(statuses).toEqual(["checking", "downloading", "idle"]);

    await vi.advanceTimersByTimeAsync(SIX_HOURS);
    expect(controller.getState().status).toBe("ready");
  });

  it("never checks when automatic checks are off", async () => {
    const { controller, deps } = setup({}, false);

    controller.start();
    await vi.advanceTimersByTimeAsync(4 * SIX_HOURS);

    expect(deps.check).not.toHaveBeenCalled();
  });
});

describe("the automatic-check setting", () => {
  it("stops checking when turned off", async () => {
    const { controller, deps, setting } = setup();

    controller.start();
    await vi.advanceTimersByTimeAsync(0);
    setting.set(false);
    await vi.advanceTimersByTimeAsync(4 * SIX_HOURS);

    expect(deps.check).toHaveBeenCalledTimes(1);
  });

  it("checks right away when turned on, then every 6 hours", async () => {
    const { controller, deps, setting } = setup({}, false);

    controller.start();
    await vi.advanceTimersByTimeAsync(SIX_HOURS);
    setting.set(true);
    await vi.advanceTimersByTimeAsync(0);
    expect(deps.check).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(SIX_HOURS);
    expect(deps.check).toHaveBeenCalledTimes(2);
  });
});

describe("a found update", () => {
  it("downloads straight away, then is ready with its version and notes", async () => {
    const update = fakeUpdate("1.0.1");
    const { controller, statuses } = setup({ check: vi.fn(() => Promise.resolve(update)) });

    controller.start();
    await vi.advanceTimersByTimeAsync(0);

    expect(update.download).toHaveBeenCalledTimes(1);
    expect(update.install).not.toHaveBeenCalled();
    expect(statuses).toEqual(["checking", "downloading", "ready"]);
    expect(controller.getState()).toEqual({
      status: "ready",
      update: { version: "1.0.1", notes: "## 1.0.1\n\n- Fixes", date: "2026-10-21T12:00:00Z" },
    });
  });

  it("stops the periodic checks for the session once it is ready", async () => {
    const check = vi
      .fn<() => Promise<FoundUpdate | null>>()
      .mockResolvedValueOnce(null)
      .mockResolvedValue(fakeUpdate("1.0.1"));
    const { controller } = setup({ check });

    controller.start();
    await vi.advanceTimersByTimeAsync(SIX_HOURS);
    expect(controller.getState().status).toBe("ready");
    await vi.advanceTimersByTimeAsync(4 * SIX_HOURS);

    expect(check).toHaveBeenCalledTimes(2);
  });
});

describe("checkNow", () => {
  it("checks even with automatic checks off, and says when this is the newest", async () => {
    const { controller, deps, statuses } = setup({}, false);

    await controller.checkNow();

    expect(deps.check).toHaveBeenCalledTimes(1);
    expect(statuses).toEqual(["checking", "upToDate"]);
  });

  it("downloads what it finds and ends ready", async () => {
    const update = fakeUpdate("1.1.0");
    const { controller, statuses } = setup({ check: vi.fn(() => Promise.resolve(update)) }, false);

    await controller.checkNow();

    expect(update.download).toHaveBeenCalledTimes(1);
    expect(statuses).toEqual(["checking", "downloading", "ready"]);
  });

  it("keeps an update it already downloaded rather than fetching it again", async () => {
    const update = fakeUpdate("1.1.0");
    const check = vi.fn(() => Promise.resolve(update));
    const { controller } = setup({ check });
    await controller.checkNow();

    await controller.checkNow();

    expect(check).toHaveBeenCalledTimes(1);
    expect(update.download).toHaveBeenCalledTimes(1);
    expect(controller.getState().status).toBe("ready");
  });

  it("joins a background check already under way, and reports how it went", async () => {
    let finish!: (found: FoundUpdate | null) => void;
    const check = vi.fn(() => new Promise<FoundUpdate | null>((resolve) => (finish = resolve)));
    const { controller, statuses } = setup({ check });
    controller.start();

    const manual = controller.checkNow();
    finish(null);
    await manual;

    expect(check).toHaveBeenCalledTimes(1);
    expect(statuses).toEqual(["checking", "upToDate"]);
  });

  it("says the check failed, rather than passing offline off as up to date", async () => {
    const { controller } = setup({ check: vi.fn(() => Promise.reject(new Error("offline"))) });

    await controller.checkNow();

    expect(controller.getState()).toEqual({ status: "error", during: "check", message: "offline" });
  });

  it("says a failed download failed", async () => {
    const update = fakeUpdate();
    vi.mocked(update.download).mockRejectedValueOnce(new Error("connection reset"));
    const { controller } = setup({ check: vi.fn(() => Promise.resolve(update)) });

    await controller.checkNow();

    expect(controller.getState()).toEqual({
      status: "error",
      during: "check",
      message: "connection reset",
    });
  });
});

describe("restartToUpdate", () => {
  it("saves the queue, then installs, then relaunches", async () => {
    const steps: string[] = [];
    const update = fakeUpdate();
    vi.mocked(update.install).mockImplementation(async () => void steps.push("install"));
    const { controller } = setup({
      check: vi.fn(() => Promise.resolve(update)),
      saveQueue: vi.fn(async () => void steps.push("saveQueue")),
      relaunch: vi.fn(async () => void steps.push("relaunch")),
    });
    await controller.checkNow();

    await controller.restartToUpdate();

    expect(steps).toEqual(["saveQueue", "install", "relaunch"]);
  });

  it("on a failed install, says so, never relaunches, and lets the listener try again", async () => {
    const update = fakeUpdate("1.0.1");
    vi.mocked(update.install).mockRejectedValueOnce(new Error("installer exited with 2"));
    const { controller, deps } = setup({ check: vi.fn(() => Promise.resolve(update)) });
    await controller.checkNow();

    await controller.restartToUpdate();

    expect(deps.relaunch).not.toHaveBeenCalled();
    expect(controller.getState()).toEqual({
      status: "error",
      during: "install",
      message: "installer exited with 2",
      update: { version: "1.0.1", notes: "## 1.0.1\n\n- Fixes", date: "2026-10-21T12:00:00Z" },
    });

    await controller.restartToUpdate();
    expect(update.install).toHaveBeenCalledTimes(2);
    expect(deps.relaunch).toHaveBeenCalledTimes(1);
  });

  it("ignores a second click, and checks, while it is installing", async () => {
    let finishInstall!: () => void;
    const update = fakeUpdate();
    vi.mocked(update.install).mockImplementation(
      () => new Promise<void>((resolve) => (finishInstall = resolve)),
    );
    const { controller, deps } = setup({ check: vi.fn(() => Promise.resolve(update)) });
    await controller.checkNow();

    const restarting = controller.restartToUpdate();
    await vi.advanceTimersByTimeAsync(0);
    await controller.restartToUpdate();
    await controller.checkNow();
    expect(controller.getState().status).toBe("installing");

    finishInstall();
    await restarting;
    expect(update.install).toHaveBeenCalledTimes(1);
    expect(deps.relaunch).toHaveBeenCalledTimes(1);
  });

  it("does nothing before an update is ready", async () => {
    const { controller, deps } = setup();

    await controller.restartToUpdate();

    expect(deps.saveQueue).not.toHaveBeenCalled();
    expect(deps.relaunch).not.toHaveBeenCalled();
    expect(controller.getState().status).toBe("idle");
  });
});

describe("dev builds", () => {
  it("never check, automatically or on request", async () => {
    const { controller, deps, setting, statuses } = setup({ enabled: false });

    controller.start();
    setting.set(false);
    setting.set(true);
    await controller.checkNow();
    await vi.advanceTimersByTimeAsync(4 * SIX_HOURS);

    expect(deps.check).not.toHaveBeenCalled();
    expect(statuses).toEqual([]);
  });
});
