import { create } from "zustand";

export type Session = "loading" | "setup" | "ready";

/** The queue panel's tabs: the Queue itself, and the Play History ("Recently played"). */
export type QueuePanelTab = "queue" | "playHistory";

interface UiStore {
  session: Session;
  nowPlayingOpen: boolean;
  queuePanelOpen: boolean;
  /** Kept while the panel is closed, so it reopens on the tab used last. */
  queuePanelTab: QueuePanelTab;
  toast: { message: string; kind: "error" | "info" } | null;
  /** Non-null opens the "new playlist" dialog, seeded with these song ids. */
  newPlaylistSongIds: string[] | null;
  /** The dialog's suggested name. */
  newPlaylistName: string;
  /** Non-null while a stats import runs (fed by stats:import events). */
  statsImport: { imported: number; total: number | null } | null;
  /** The downloaded update's release notes and "Restart to update". */
  updatePanelOpen: boolean;

  setSession: (s: Session) => void;
  setNowPlayingOpen: (open: boolean) => void;
  setQueuePanelOpen: (open: boolean) => void;
  setQueuePanelTab: (tab: QueuePanelTab) => void;
  showToast: (message: string, kind?: "error" | "info") => void;
  dismissToast: () => void;
  openNewPlaylist: (songIds?: string[], name?: string) => void;
  closeNewPlaylist: () => void;
  setUpdatePanelOpen: (open: boolean) => void;
}

export const useUiStore = create<UiStore>((set) => ({
  session: "loading",
  nowPlayingOpen: false,
  queuePanelOpen: false,
  queuePanelTab: "queue",
  toast: null,
  newPlaylistSongIds: null,
  newPlaylistName: "",
  statsImport: null,
  updatePanelOpen: false,

  setSession: (session) => set({ session }),
  openNewPlaylist: (songIds = [], name = "") =>
    set({ newPlaylistSongIds: songIds, newPlaylistName: name }),
  closeNewPlaylist: () => set({ newPlaylistSongIds: null }),
  setNowPlayingOpen: (nowPlayingOpen) => set({ nowPlayingOpen }),
  setQueuePanelOpen: (queuePanelOpen) => set({ queuePanelOpen }),
  setQueuePanelTab: (queuePanelTab) => set({ queuePanelTab }),
  showToast: (message, kind = "info") => {
    set({ toast: { message, kind } });
    window.setTimeout(() => set({ toast: null }), 5000);
  },
  dismissToast: () => set({ toast: null }),
  setUpdatePanelOpen: (updatePanelOpen) => set({ updatePanelOpen }),
}));
