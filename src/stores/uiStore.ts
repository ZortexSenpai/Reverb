import { create } from "zustand";

export type Session = "loading" | "setup" | "ready";

interface UiStore {
  session: Session;
  nowPlayingOpen: boolean;
  queuePanelOpen: boolean;
  toast: { message: string; kind: "error" | "info" } | null;
  /** Non-null opens the "new playlist" dialog, seeded with these song ids. */
  newPlaylistSongIds: string[] | null;
  /** The dialog's suggested name. */
  newPlaylistName: string;
  /** Non-null while a stats import runs (fed by stats:import events). */
  statsImport: { imported: number; total: number | null } | null;

  setSession: (s: Session) => void;
  setNowPlayingOpen: (open: boolean) => void;
  setQueuePanelOpen: (open: boolean) => void;
  showToast: (message: string, kind?: "error" | "info") => void;
  dismissToast: () => void;
  openNewPlaylist: (songIds?: string[], name?: string) => void;
  closeNewPlaylist: () => void;
}

export const useUiStore = create<UiStore>((set) => ({
  session: "loading",
  nowPlayingOpen: false,
  queuePanelOpen: false,
  toast: null,
  newPlaylistSongIds: null,
  newPlaylistName: "",
  statsImport: null,

  setSession: (session) => set({ session }),
  openNewPlaylist: (songIds = [], name = "") =>
    set({ newPlaylistSongIds: songIds, newPlaylistName: name }),
  closeNewPlaylist: () => set({ newPlaylistSongIds: null }),
  setNowPlayingOpen: (nowPlayingOpen) => set({ nowPlayingOpen }),
  setQueuePanelOpen: (queuePanelOpen) => set({ queuePanelOpen }),
  showToast: (message, kind = "info") => {
    set({ toast: { message, kind } });
    window.setTimeout(() => set({ toast: null }), 5000);
  },
  dismissToast: () => set({ toast: null }),
}));
