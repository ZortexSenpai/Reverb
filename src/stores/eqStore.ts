import { create } from "zustand";
import { cmd } from "../lib/ipc/commands";
import type { EqBand, EqPresetMeta, EqState, PreampMode } from "../lib/subsonic/types";

const DEFAULT_BAND: EqBand = {
  enabled: true,
  type: "peaking",
  frequency: 1000,
  gainDb: 0,
  q: 1,
};

export const MAX_BANDS = 10;

interface EqStore {
  hydrated: boolean;
  hydrateError: string | null;
  enabled: boolean;
  bands: EqBand[];
  preamp: PreampMode;
  presets: EqPresetMeta[];
  activePresetId: string | null;
  dirty: boolean;
  /** True while a curve handle is being dragged (echo suppression). */
  dragging: boolean;

  hydrate: () => Promise<void>;
  applyRemote: (state: EqState) => void;
  setEnabled: (on: boolean) => void;
  updateBand: (index: number, patch: Partial<EqBand>) => void;
  addBand: (frequency?: number, gainDb?: number) => void;
  removeBand: (index: number) => void;
  setPreamp: (preamp: PreampMode) => void;
  setDragging: (dragging: boolean) => void;
  loadPreset: (id: string) => Promise<void>;
  saveAsPreset: (name: string) => Promise<void>;
  deletePreset: (id: string) => Promise<void>;
}

// Trailing-throttled per-band pushes: the store updates instantly (live
// curve); the engine gets at most one update per ~30ms per band.
const pendingPush = new Map<number, ReturnType<typeof setTimeout>>();

function pushBandThrottled(index: number, getBand: () => EqBand | undefined) {
  if (pendingPush.has(index)) return;
  pendingPush.set(
    index,
    setTimeout(() => {
      pendingPush.delete(index);
      const band = getBand();
      if (band) void cmd("eq_set_band", { index, band });
    }, 30),
  );
}

export const useEqStore = create<EqStore>((set, get) => ({
  hydrated: false,
  hydrateError: null,
  enabled: false,
  bands: [],
  preamp: { mode: "auto" },
  presets: [],
  activePresetId: null,
  dirty: false,
  dragging: false,

  hydrate: async () => {
    if (get().hydrated) return;
    try {
      const [state, presets] = await Promise.all([cmd("eq_get_state"), cmd("eq_list_presets")]);
      set({
        hydrated: true,
        hydrateError: null,
        enabled: state.enabled,
        bands: state.bands,
        preamp: state.preamp,
        presets,
      });
    } catch (e) {
      set({ hydrateError: e instanceof Error ? e.message : JSON.stringify(e) });
    }
  },

  applyRemote: (state) => {
    // Ignore echoes of our own writes while interacting.
    if (get().dragging || pendingPush.size > 0) return;
    const current = { enabled: get().enabled, bands: get().bands, preamp: get().preamp };
    if (JSON.stringify(current) === JSON.stringify(state)) return;
    set({ enabled: state.enabled, bands: state.bands, preamp: state.preamp });
  },

  setEnabled: (on) => {
    set({ enabled: on });
    void cmd("eq_set_enabled", { enabled: on });
  },

  updateBand: (index, patch) => {
    const bands = get().bands.map((b, i) => (i === index ? { ...b, ...patch } : b));
    set({ bands, dirty: true });
    pushBandThrottled(index, () => get().bands[index]);
  },

  addBand: (frequency = 1000, gainDb = 0) => {
    const { bands, enabled, preamp } = get();
    if (bands.length >= MAX_BANDS) return;
    const next = [...bands, { ...DEFAULT_BAND, frequency, gainDb }];
    set({ bands: next, dirty: true, enabled: bands.length === 0 ? true : enabled });
    void cmd("eq_set_all", {
      eq: { enabled: bands.length === 0 ? true : enabled, bands: next, preamp },
    });
  },

  removeBand: (index) => {
    const { bands, enabled, preamp } = get();
    const next = bands.filter((_, i) => i !== index);
    set({ bands: next, dirty: true });
    void cmd("eq_set_all", { eq: { enabled, bands: next, preamp } });
  },

  setPreamp: (preamp) => {
    set({ preamp, dirty: true });
    void cmd("eq_set_preamp", { preamp });
  },

  setDragging: (dragging) => set({ dragging }),

  loadPreset: async (id) => {
    const state = await cmd("eq_load_preset", { id });
    set({
      enabled: state.enabled,
      bands: state.bands,
      preamp: state.preamp,
      activePresetId: id,
      dirty: false,
    });
  },

  saveAsPreset: async (name) => {
    const meta = await cmd("eq_save_preset", { name });
    set({ presets: [...get().presets, meta], activePresetId: meta.id, dirty: false });
  },

  deletePreset: async (id) => {
    await cmd("eq_delete_preset", { id });
    set({
      presets: get().presets.filter((p) => p.id !== id),
      activePresetId: get().activePresetId === id ? null : get().activePresetId,
    });
  },
}));
