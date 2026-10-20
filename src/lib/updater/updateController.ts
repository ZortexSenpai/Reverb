// The update controller: when to look for a new release, what to do with one,
// and the state the UI shows for it. Everything that touches the outside world
// (the updater plugin, the setting, the queue) is passed in, so this file is
// pure policy and runs in tests without Tauri. Its schedule uses the global
// timers, which the tests fake.

/** What the UI shows about a found update. */
export interface UpdateInfo {
  version: string;
  /** Release notes, Markdown (the release's CHANGELOG.md section). */
  notes: string;
  /** When the release was published, as the manifest gives it. */
  date: string | null;
}

/** A newer release the updater found. */
export interface FoundUpdate extends UpdateInfo {
  download: () => Promise<void>;
  install: () => Promise<void>;
}

export type UpdateState =
  | { status: "idle" }
  | { status: "checking" }
  | { status: "downloading"; update: UpdateInfo }
  /** Downloaded and waiting for the listener to restart into it. */
  | { status: "ready"; update: UpdateInfo }
  | { status: "installing"; update: UpdateInfo }
  /** Only after a check the listener asked for. */
  | { status: "upToDate" }
  /** Only for checks the listener asked for, and for installs. */
  | { status: "error"; during: "check"; message: string }
  | { status: "error"; during: "install"; message: string; update: UpdateInfo };

export interface UpdateControllerDeps {
  /** Ask for a newer stable release; null when this is the newest. */
  check: () => Promise<FoundUpdate | null>;
  relaunch: () => Promise<void>;
  /** Save the queue snapshot now, before the app goes away. */
  saveQueue: () => Promise<void>;
  /** The "check for updates automatically" setting, and its changes. */
  autoCheck: {
    get: () => boolean;
    subscribe: (listener: (on: boolean) => void) => () => void;
  };
  /** False in dev builds, which never check. */
  enabled: boolean;
}

/** How often a long-running session looks again. */
const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

export interface UpdateController {
  getState: () => UpdateState;
  subscribe: (listener: (state: UpdateState) => void) => () => void;
  /** Begin automatic checks (if the setting allows). Called once at boot. */
  start: () => void;
  /** Check whatever the setting says; ends `upToDate`, `ready` or `error`. */
  checkNow: () => Promise<void>;
  /**
   * Save the queue, install the downloaded update and relaunch into it. The
   * only way an update is ever installed: nothing restarts on its own.
   */
  restartToUpdate: () => Promise<void>;
}

/**
 * The update a state is about, once it has finished downloading: what the
 * update panel shows, and what "Restart to update" would install.
 */
export function downloadedUpdate(state: UpdateState): UpdateInfo | null {
  switch (state.status) {
    case "ready":
    case "installing":
      return state.update;
    case "error":
      return state.during === "install" ? state.update : null;
    default:
      return null;
  }
}

function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

function infoOf(found: FoundUpdate): UpdateInfo {
  return { version: found.version, notes: found.notes, date: found.date };
}

export function createUpdateController(deps: UpdateControllerDeps): UpdateController {
  let state: UpdateState = { status: "idle" };
  const listeners = new Set<(state: UpdateState) => void>();
  let started = false;
  let inFlight: Promise<void> | null = null;
  let reportOutcome = false;
  let schedule: ReturnType<typeof setInterval> | null = null;
  /** The downloaded update, once there is one. */
  let downloaded: FoundUpdate | null = null;

  function setState(next: UpdateState) {
    state = next;
    for (const listener of listeners) listener(state);
  }

  /**
   * One check-and-download at a time. A manual check that arrives while a
   * background one is running joins it, and gets told how it went.
   */
  function run(manual: boolean): Promise<void> {
    if (!deps.enabled) return Promise.resolve();
    if (downloaded) {
      // Already holding one, so there is nothing to fetch. Offer it again
      // (after a failed install, say), unless it is being installed.
      if (state.status !== "installing") {
        setState({ status: "ready", update: infoOf(downloaded) });
      }
      return Promise.resolve();
    }
    if (manual) reportOutcome = true;
    inFlight ??= attempt().finally(() => {
      inFlight = null;
      reportOutcome = false;
    });
    return inFlight;
  }

  async function attempt() {
    setState({ status: "checking" });
    let found: FoundUpdate | null;
    try {
      found = await deps.check();
      if (!found) {
        setState(reportOutcome ? { status: "upToDate" } : { status: "idle" });
        return;
      }
      setState({ status: "downloading", update: infoOf(found) });
      await found.download();
    } catch (e) {
      // Offline, GitHub down, a dropped download. Said out loud only when the
      // listener asked; in the background the next interval just retries.
      setState(
        reportOutcome
          ? { status: "error", during: "check", message: messageOf(e) }
          : { status: "idle" },
      );
      return;
    }
    downloaded = found;
    // Stop looking: anything newer than this waits for the next launch.
    stopSchedule();
    setState({ status: "ready", update: infoOf(found) });
  }

  function startSchedule() {
    void run(false);
    schedule = setInterval(() => void run(false), CHECK_INTERVAL_MS);
  }

  function stopSchedule() {
    if (schedule == null) return;
    clearInterval(schedule);
    schedule = null;
  }

  return {
    getState: () => state,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
    start: () => {
      if (started || !deps.enabled) return;
      started = true;
      if (deps.autoCheck.get()) startSchedule();
      deps.autoCheck.subscribe((on) => {
        if (!on) stopSchedule();
        // Already running, or already holding an update: nothing to restart.
        else if (schedule == null && !downloaded) startSchedule();
      });
    },
    checkNow: () => run(true),
    restartToUpdate: async () => {
      if (!downloaded || state.status === "installing") return;
      const update = infoOf(downloaded);
      setState({ status: "installing", update });
      try {
        await deps.saveQueue();
        await downloaded.install();
        await deps.relaunch();
      } catch (e) {
        // The running version is untouched; the update stays downloaded, so
        // the listener can try again.
        setState({ status: "error", during: "install", message: messageOf(e), update });
      }
    },
  };
}
