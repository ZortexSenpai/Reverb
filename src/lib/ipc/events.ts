// Typed wrapper around Tauri event listening.

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { PlaybackState } from "../subsonic/types";

export interface EventMap {
  "player:state": { state: PlaybackState };
  "player:position": { positionMs: number; durationMs: number };
  "player:track-changed": {
    trackId: string;
    reason: "natural" | "command";
    title?: string | null;
    artist?: string | null;
    album?: string | null;
    durationMs: number;
  };
  "player:media-key": "next" | "previous";
  "player:signal-path": import("../subsonic/types").SignalPath;
  "stats:import": {
    source: string;
    state: "running" | "done" | "error";
    imported: number;
    total?: number | null;
    message?: string | null;
  };
  "player:track-ended": { trackId: string };
  /**
   * The Song passed "Mark as played after" and counts as a Play, whether or
   * not it gets scrobbled. `startedAtMs` is Unix ms of when it started.
   */
  "player:play-counted": { trackId: string; startedAtMs: number };
  "player:error": {
    code: string;
    message: string;
    trackId?: string | null;
    recoverable: boolean;
  };
  "player:format": { sampleRate: number };
  /** The configured output device vanished; the backend is on the system default. */
  "player:device-lost": { device: string };
  "eq:state": import("../subsonic/types").EqState;
  /** A new set of mixes replaced the cached one in the background. */
  "mixes:updated": null;
  /**
   * A new Library index is ready: built for a new day, after a scan started
   * from Reverb, or for another account.
   */
  "library:index-ready": null;
}

export function on<K extends keyof EventMap>(
  event: K,
  handler: (payload: EventMap[K]) => void,
): Promise<UnlistenFn> {
  return listen<EventMap[K]>(event, (e) => handler(e.payload));
}
