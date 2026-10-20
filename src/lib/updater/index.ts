// The app's update controller, wired to the Tauri updater and process
// plugins, the "check for updates automatically" preference and the queue.
// The policy itself lives in ./updateController, which imports none of this.

import { useSyncExternalStore } from "react";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { saveQueueSnapshotNow } from "../../stores/playerStore";
import { usePrefsStore } from "../../stores/prefsStore";
import { createUpdateController, type FoundUpdate, type UpdateState } from "./updateController";

export { downloadedUpdate, type UpdateState } from "./updateController";

/** A version as listeners see it, and as its release is tagged: "v1.0.1". */
export function versionLabel(version: string): string {
  return `v${version}`;
}

/** The GitHub release page for a version ("1.0.1"). */
export function releaseUrl(version: string): string {
  return `https://github.com/ZortexSenpai/Reverb/releases/tag/${versionLabel(version)}`;
}

/**
 * Ask the update endpoint (GitHub's "latest" release, which never points at a
 * draft or a pre-release) whether there is something newer. The plugin checks
 * the download's signature against the public key in tauri.conf.json.
 */
async function checkForRelease(): Promise<FoundUpdate | null> {
  const update = await check();
  if (!update) return null;
  return {
    version: update.version,
    notes: update.body ?? "",
    date: update.date ?? null,
    download: () => update.download(),
    install: () => update.install(),
  };
}

/**
 * Dev builds never look for updates: they would offer to replace themselves
 * with the last public release. The UI says so rather than offering a check.
 */
export const updatesEnabled = !import.meta.env.DEV;

export const updateController = createUpdateController({
  enabled: updatesEnabled,
  check: checkForRelease,
  relaunch,
  saveQueue: saveQueueSnapshotNow,
  autoCheck: {
    get: () => usePrefsStore.getState().prefs.checkForUpdates,
    subscribe: (listener) =>
      usePrefsStore.subscribe((state, prev) => {
        const on = state.prefs.checkForUpdates;
        if (on !== prev.prefs.checkForUpdates) listener(on);
      }),
  },
});

export function useUpdateState(): UpdateState {
  return useSyncExternalStore(updateController.subscribe, updateController.getState);
}
