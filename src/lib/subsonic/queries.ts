import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { usePlayerStore } from "../../stores/playerStore";
import { cmd } from "../ipc/commands";
import { qk } from "./keys";
import type { AlbumListType, Play } from "./types";

export const ALBUM_PAGE_SIZE = 60;
export const SONG_PAGE_SIZE = 100;
/** Albums fetched per genre to build its cover mosaic. */
const MOSAIC_ALBUMS = 4;

export interface AlbumPageParams {
  listType: AlbumListType;
  /** Text filter; the server searches the whole library and ignores listType. */
  query?: string;
  genre?: string;
  fromYear?: number;
  toYear?: number;
  page: number;
  size: number;
}

export function useAlbumPage(params: AlbumPageParams) {
  return useQuery({
    queryKey: qk.albumPage(params as unknown as Record<string, unknown>),
    queryFn: () =>
      cmd("library_get_albums", {
        listType: params.listType,
        size: params.size,
        offset: params.page * params.size,
        query: params.query,
        genre: params.genre,
        fromYear: params.fromYear,
        toYear: params.toYear,
      }),
    placeholderData: keepPreviousData,
    staleTime: 5 * 60_000,
  });
}

export interface SongPageParams {
  query?: string;
  genre?: string;
  /** Server sort field (Navidrome native naming, e.g. "playCount"). */
  sort?: string;
  descending?: boolean;
  page: number;
  size: number;
}

export function useSongPage(params: SongPageParams) {
  return useQuery({
    queryKey: qk.songPage(params as unknown as Record<string, unknown>),
    queryFn: () =>
      cmd("library_get_songs", {
        size: params.size,
        offset: params.page * params.size,
        query: params.query,
        genre: params.genre,
        sort: params.sort,
        descending: params.descending,
      }),
    placeholderData: keepPreviousData,
    staleTime: 5 * 60_000,
  });
}

/**
 * First few albums of a genre, used for its mosaic artwork. Shared key so the
 * genres grid and the genre page reuse one fetch.
 */
export function useGenreMosaicAlbums(genre: string) {
  return useAlbumPage({ listType: "byGenre", genre, page: 0, size: MOSAIC_ALBUMS });
}

export function useGenres() {
  return useQuery({
    queryKey: qk.genres(),
    queryFn: () => cmd("library_get_genres"),
    staleTime: 10 * 60_000,
  });
}

export function useAlbum(id: string) {
  return useQuery({
    queryKey: qk.album(id),
    queryFn: () => cmd("library_get_album", { id }),
    staleTime: 5 * 60_000,
  });
}

export function useArtists() {
  return useQuery({
    queryKey: qk.artists(),
    queryFn: () => cmd("library_get_artists"),
    staleTime: 5 * 60_000,
  });
}

export function useArtist(id: string) {
  return useQuery({
    queryKey: qk.artist(id),
    queryFn: () => cmd("library_get_artist", { id }),
    staleTime: 5 * 60_000,
  });
}

export function useArtistAppearances(id: string) {
  return useQuery({
    queryKey: qk.artistAppearances(id),
    queryFn: () => cmd("library_get_artist_appearances", { id }),
    staleTime: 5 * 60_000,
  });
}

export function usePlaylists(enabled = true) {
  return useQuery({
    queryKey: qk.playlists(),
    queryFn: () => cmd("library_get_playlists"),
    staleTime: 60_000,
    enabled,
  });
}

export function usePlaylist(id: string) {
  return useQuery({
    queryKey: qk.playlist(id),
    queryFn: () => cmd("library_get_playlist", { id }),
    staleTime: 60_000,
  });
}

/** The "Made for you" mixes. The backend keeps them for the day, so a
 *  refetch is cheap; only the first call of a day waits for them to be made. */
export function useMixes() {
  return useQuery({
    queryKey: qk.mixes(),
    queryFn: () => cmd("mixes_get", { refresh: false }),
    staleTime: 10 * 60_000,
  });
}

/**
 * The signed-in account's Plays, newest first. One that can't be read reads as
 * empty, as an unreadable file does in the store, so the Recently played tab
 * says "Nothing played yet" rather than looking broken.
 */
export async function readPlayHistory(): Promise<Play[]> {
  try {
    return await cmd("history_get");
  } catch (e) {
    console.warn("could not read the Play History:", e);
    return [];
  }
}

/** The Play History ("Recently played"), newest first. Recording a Play and
 *  clearing invalidate it, so an open list follows along. */
export function usePlayHistory() {
  return useQuery({
    queryKey: qk.playHistory(),
    queryFn: readPlayHistory,
  });
}

export function useStarred() {
  return useQuery({
    queryKey: qk.starred(),
    queryFn: () => cmd("library_get_starred"),
    staleTime: 60_000,
  });
}

/** Optimistic star/unstar that patches every cached view of the entity. */
export function useStarMutation() {
  const queryClient = useQueryClient();
  return async (id: string, kind: "song" | "album" | "artist", starred: boolean) => {
    const stamp = starred ? new Date().toISOString() : undefined;

    // Patch anything cached that contains this entity.
    queryClient.setQueriesData({ type: "all" }, (data: unknown) =>
      patchStarred(data, id, kind, stamp),
    );
    // The queue is zustand state, not a query cache, so it needs telling too —
    // the player bar and full-screen player read the song from there.
    if (kind === "song") usePlayerStore.getState().setSongStarred(id, stamp);
    try {
      await cmd("library_set_star", { id, kind, starred });
    } finally {
      void queryClient.invalidateQueries({ queryKey: qk.starred() });
    }
  };
}

/**
 * Optimistic 1–5 rating (0 clears). Ratings live on the same entities as
 * stars, so this patches every cached copy the same way.
 */
export function useRatingMutation() {
  const queryClient = useQueryClient();
  return async (id: string, rating: number) => {
    queryClient.setQueriesData({ type: "all" }, (data: unknown) =>
      patchField(data, id, "userRating", rating === 0 ? undefined : rating),
    );
    await cmd("library_set_rating", { id, rating });
  };
}

export function useMusicFolders() {
  return useQuery({
    queryKey: qk.musicFolders(),
    queryFn: () => cmd("library_get_music_folders"),
    staleTime: 30 * 60_000,
  });
}

/** Top of the folder tree; `folderId` "" means every music folder merged. */
export function useIndexes(folderId: string) {
  return useQuery({
    queryKey: qk.indexes(folderId),
    queryFn: () => cmd("library_get_indexes", { musicFolderId: folderId || undefined }),
    staleTime: 10 * 60_000,
  });
}

export function useDirectory(id: string) {
  return useQuery({
    queryKey: qk.directory(id),
    queryFn: () => cmd("library_get_music_directory", { id }),
    enabled: id !== "",
    staleTime: 10 * 60_000,
  });
}

/** Recursively find objects with a matching id and set a field on them. */
function patchField(data: unknown, id: string, field: string, value: unknown): unknown {
  if (Array.isArray(data)) return data.map((item) => patchField(item, id, field, value));
  if (typeof data === "object" && data !== null) {
    const obj = data as Record<string, unknown>;
    let out: Record<string, unknown> = obj.id === id ? { ...obj, [field]: value } : obj;
    for (const [key, nested] of Object.entries(obj)) {
      if (typeof nested === "object" && nested !== null) {
        const patched = patchField(nested, id, field, value);
        if (patched !== nested) {
          out = out === obj ? { ...obj } : out;
          out[key] = patched;
        }
      }
    }
    return out;
  }
  return data;
}

/** Recursively find objects with a matching id and set/unset `starred`. */
function patchStarred(data: unknown, id: string, kind: string, stamp: string | undefined): unknown {
  if (Array.isArray(data)) {
    return data.map((item) => patchStarred(item, id, kind, stamp));
  }
  if (typeof data === "object" && data !== null) {
    const obj = data as Record<string, unknown>;
    let out: Record<string, unknown> = obj;
    const looksRight =
      obj.id === id &&
      // Song objects have `title`; albums have `songCount`; artists have neither.
      ((kind === "song" && "title" in obj) ||
        (kind === "album" && "songCount" in obj && !("title" in obj)) ||
        (kind === "artist" && !("title" in obj) && !("songCount" in obj)));
    if (looksRight) {
      out = { ...obj, starred: stamp };
    }
    for (const [key, value] of Object.entries(obj)) {
      if (typeof value === "object" && value !== null) {
        const patched = patchStarred(value, id, kind, stamp);
        if (patched !== value) {
          out = out === obj ? { ...obj } : out;
          out[key] = patched;
        }
      }
    }
    return out;
  }
  return data;
}
