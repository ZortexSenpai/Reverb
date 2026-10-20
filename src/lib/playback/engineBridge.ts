// Registered once at boot (before React renders): translates Tauri events
// from the Rust engine into store mutations. Living outside React sidesteps
// StrictMode double-mount listener races.

import { cmd } from "../ipc/commands";
import { getT } from "../i18n";
import { on } from "../ipc/events";
import { queryClient } from "../queryClient";
import { qk } from "../subsonic/keys";
import { useEqStore } from "../../stores/eqStore";
import { advanceToPendingNext, usePlayerStore } from "../../stores/playerStore";
import { usePrefsStore } from "../../stores/prefsStore";
import { useUiStore } from "../../stores/uiStore";
import { positionClock } from "./positionClock";

let initialized = false;

export function initEngineBridge() {
  if (initialized) return;
  initialized = true;

  void on("player:state", ({ state }) => {
    usePlayerStore.setState({ state });
    positionClock.setPlaying(state === "playing");
    if (state === "stopped") {
      usePlayerStore.setState({ currentTrackId: null, signalPath: null });
      positionClock.reset();
    }
  });

  void on("player:position", ({ positionMs, durationMs }) => {
    positionClock.onEngineTick(positionMs, durationMs);
    usePlayerStore.setState({ durationMs });
  });

  void on("player:track-changed", ({ trackId, reason }) => {
    if (reason === "natural") {
      positionClock.reset();
      advanceToPendingNext();
    }
    usePlayerStore.setState({ currentTrackId: trackId });
  });

  void on("player:track-ended", () => {
    positionClock.reset();
  });

  void on("player:error", ({ message, recoverable }) => {
    useUiStore.getState().showToast(message, "error");
    if (!recoverable) {
      usePlayerStore.setState({ state: "stopped", currentTrackId: null });
    }
  });

  void on("player:format", ({ sampleRate }) => {
    usePlayerStore.setState({ sampleRate });
  });

  // A disconnected output device: the engine reopened on the system default and
  // the backend already cleared the saved preference, so this mirrors it into
  // the store *without* persisting — going through `set` would send the engine a
  // redundant device change and rebuild the stream a second time.
  void on("player:device-lost", () => {
    usePrefsStore.setState((s) => ({ prefs: { ...s.prefs, audioDevice: null } }));
    void queryClient.invalidateQueries({ queryKey: ["audio-devices"] });
    useUiStore.getState().showToast(getT()("toast.outputDeviceLost"));
  });

  void on("player:signal-path", (signalPath) => {
    usePlayerStore.setState({ signalPath });
  });

  void on("eq:state", (state) => {
    useEqStore.getState().applyRemote(state);
  });

  // Hardware media keys / MPRIS route Next/Previous through the queue brain.
  void on("player:media-key", (action) => {
    const player = usePlayerStore.getState();
    if (action === "next") player.next();
    else if (action === "previous") player.previous();
  });

  // Mixes are rebuilt at startup for a new day, or once a first Last.fm
  // import gives them a listening history to work from.
  void on("mixes:updated", () => {
    void queryClient.invalidateQueries({ queryKey: qk.mixes() });
  });

  // Stats imports run in the backend regardless of which view is mounted:
  // mirror their progress into the UI store and refresh cached data (and
  // announce the outcome) when they finish.
  void on("stats:import", (progress) => {
    if (progress.state === "running") {
      useUiStore.setState({
        statsImport: { imported: progress.imported, total: progress.total ?? null },
      });
    } else {
      useUiStore.setState({ statsImport: null });
      void queryClient.invalidateQueries({ queryKey: ["stats-sources"] });
      void queryClient.invalidateQueries({ queryKey: ["stats"] });
      const toast = useUiStore.getState().showToast;
      const t = getT();
      if (progress.state === "done") {
        toast(
          progress.imported > 0
            ? t("toast.imported", { n: progress.imported })
            : t("toast.statsUpToDate"),
        );
      } else if (progress.message) {
        toast(progress.message, "error");
      }
    }
  });
}

/** Optimistic marker so the UI reacts before the first progress event. */
export function markStatsImportStarted() {
  useUiStore.setState({ statsImport: { imported: 0, total: null } });
}

/** Pull the engine's snapshot after a (re)load so the UI matches reality. */
export async function hydrateFromEngine() {
  try {
    const snapshot = await cmd("player_get_state");
    usePlayerStore.setState({
      state: snapshot.state,
      currentTrackId: snapshot.trackId ?? null,
      durationMs: snapshot.durationMs,
      volume: snapshot.volume,
      sampleRate: snapshot.sampleRate,
    });
    positionClock.reset(snapshot.durationMs);
    positionClock.onEngineTick(snapshot.positionMs, snapshot.durationMs);
    positionClock.setPlaying(snapshot.state === "playing");
  } catch {
    // Engine not up yet — defaults are fine.
  }
}
