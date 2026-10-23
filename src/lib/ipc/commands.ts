// Typed wrapper around Tauri invoke — the single place where command names
// and payload shapes are written down. Everything else calls cmd().

import { invoke } from "@tauri-apps/api/core";
import type {
  Album,
  AlbumListType,
  AlbumWithSongs,
  ArtistInfo,
  ArtistsIndexes,
  ArtistWithAlbums,
  AuthStatus,
  Directory,
  EqBand,
  EqPresetMeta,
  EqState,
  FrontendSettings,
  Genre,
  Indexes,
  LyricsList,
  Mix,
  MusicFolder,
  OutputDevice,
  Play,
  PlayableTrack,
  PlayerSnapshot,
  PlayQueue,
  Playlist,
  PlaylistWithSongs,
  PreampMode,
  Preferences,
  ScanStatus,
  SearchResult,
  ServerInfo,
  Song,
  SongPage,
  SourcesStatus,
  StarKind,
  Starred,
  StatsRange,
  StatsSummary,
} from "../subsonic/types";

type NoArgs = Record<string, never>;

export interface CommandMap {
  // auth
  auth_get_status: { args: NoArgs; result: AuthStatus };
  auth_test: {
    args: { serverUrl: string; username: string; password: string };
    result: ServerInfo;
  };
  auth_save: {
    args: { serverUrl: string; username: string; password: string };
    result: ServerInfo;
  };
  auth_sign_out: { args: NoArgs; result: void };

  // library
  library_get_albums: {
    args: {
      listType: AlbumListType;
      size: number;
      offset: number;
      /** Text filter; overrides listType/genre/year (server-side search). */
      query?: string;
      genre?: string;
      fromYear?: number;
      toYear?: number;
    };
    result: Album[];
  };
  library_get_genres: { args: NoArgs; result: Genre[] };
  library_get_album: { args: { id: string }; result: AlbumWithSongs };
  library_get_song: { args: { id: string }; result: Song };
  library_get_artists: { args: NoArgs; result: ArtistsIndexes };
  library_get_artist: { args: { id: string }; result: ArtistWithAlbums };
  /** Albums the artist is credited on without being the album artist. */
  library_get_artist_appearances: { args: { id: string }; result: Album[] };
  library_get_artist_info: { args: { id: string; name: string }; result: ArtistInfo };
  library_get_top_songs: { args: { artistName: string; count: number }; result: Song[] };
  library_get_playlists: { args: NoArgs; result: Playlist[] };
  library_get_playlist: { args: { id: string }; result: PlaylistWithSongs };
  library_get_starred: { args: NoArgs; result: Starred };
  library_get_songs: {
    args: {
      size: number;
      offset: number;
      query?: string;
      genre?: string;
      sort?: string;
      descending?: boolean;
    };
    result: SongPage;
  };
  library_set_star: { args: { id: string; kind: StarKind; starred: boolean }; result: void };
  /** 1–5; 0 clears the rating. Works for songs, albums and artists. */
  library_set_rating: { args: { id: string; rating: number }; result: void };
  library_get_artist_songs: { args: { id: string }; result: Song[] };
  library_get_music_folders: { args: NoArgs; result: MusicFolder[] };
  library_get_indexes: { args: { musicFolderId?: string }; result: Indexes };
  library_get_music_directory: { args: { id: string }; result: Directory };
  library_search: {
    args: { query: string; artistCount: number; albumCount: number; songCount: number };
    result: SearchResult;
  };
  library_start_scan: { args: NoArgs; result: ScanStatus };
  library_get_scan_status: { args: NoArgs; result: ScanStatus };
  library_create_playlist: {
    args: { name: string; songIds: string[] };
    result: PlaylistWithSongs;
  };
  library_update_playlist: {
    args: {
      playlistId: string;
      name?: string;
      comment?: string;
      addSongIds?: string[];
      removeIndices?: number[];
    };
    result: void;
  };
  library_delete_playlist: { args: { id: string }; result: void };
  library_get_lyrics: { args: { id: string }; result: LyricsList };
  library_get_similar_songs: { args: { id: string; count: number }; result: Song[] };
  /** Songs to follow `id` in a radio: similar ones, else random from `genre`. */
  library_get_radio: {
    args: { id: string; genre?: string | null; count: number };
    result: Song[];
  };
  library_get_play_queue: { args: NoArgs; result: PlayQueue | null };
  /** Empty `ids` clears the server's queue. */
  library_save_play_queue: {
    args: { ids: string[]; current: string | null; positionMs: number };
    result: void;
  };

  // mixes
  /** Today's mixes; `refresh` throws them away and picks new ones. */
  mixes_get: { args: { refresh: boolean }; result: Mix[] };

  // play history
  /** The signed-in account's Plays, newest first; none when signed out. */
  history_get: { args: NoArgs; result: Play[] };
  /** `playedAt` is Unix ms of when the Song started. */
  history_record: { args: { song: Song; playedAt: number }; result: void };
  history_clear: { args: NoArgs; result: void };

  // player
  player_get_state: { args: NoArgs; result: PlayerSnapshot };
  player_play_track: { args: { track: PlayableTrack }; result: void };
  player_set_next: { args: { track: PlayableTrack | null }; result: void };
  player_pause: { args: NoArgs; result: void };
  player_resume: { args: NoArgs; result: void };
  player_toggle: { args: NoArgs; result: void };
  player_stop: { args: NoArgs; result: void };
  player_seek: { args: { positionMs: number }; result: void };
  player_set_volume: { args: { volume: number }; result: void };

  // EQ
  eq_get_state: { args: NoArgs; result: EqState };
  eq_set_all: { args: { eq: EqState }; result: void };
  eq_set_band: { args: { index: number; band: EqBand }; result: void };
  eq_set_enabled: { args: { enabled: boolean }; result: void };
  eq_set_preamp: { args: { preamp: PreampMode }; result: void };
  eq_list_presets: { args: NoArgs; result: EqPresetMeta[] };
  eq_save_preset: { args: { name: string }; result: EqPresetMeta };
  eq_load_preset: { args: { id: string }; result: EqState };
  eq_delete_preset: { args: { id: string }; result: void };
  eq_rename_preset: { args: { id: string; name: string }; result: void };

  // stats
  stats_get_sources: { args: NoArgs; result: SourcesStatus };
  /** `sharedSecret` + `password` are only needed to enable scrobbling; the
   *  password is traded for a session key and never stored. */
  stats_configure_lastfm: {
    args: {
      username: string;
      apiKey: string;
      sharedSecret?: string | null;
      password?: string | null;
    };
    result: void;
  };
  /** `apiKey` replaces the stored one — it must come from the same Last.fm
   *  application as `sharedSecret` or the signature is rejected. */
  stats_enable_lastfm_scrobbling: {
    args: { sharedSecret: string; password: string; apiKey?: string | null };
    result: void;
  };
  stats_disable_lastfm_scrobbling: { args: NoArgs; result: void };
  stats_disconnect: { args: { source: string }; result: void };
  stats_import: { args: { source: string }; result: void };
  /** `from`/`to` are unix seconds, custom range only; `to` is exclusive. */
  stats_get: {
    args: { range: StatsRange; from?: number | null; to?: number | null };
    result: StatsSummary;
  };

  // settings / queue snapshot
  settings_get: { args: NoArgs; result: FrontendSettings };
  settings_set_prefs: { args: { prefs: Preferences }; result: void };
  audio_list_devices: { args: NoArgs; result: OutputDevice[] };
  queue_save_snapshot: { args: { snapshot: unknown }; result: void };
  queue_load_snapshot: { args: NoArgs; result: unknown };
}

export function cmd<K extends keyof CommandMap>(
  name: K,
  ...args: CommandMap[K]["args"] extends NoArgs ? [] : [CommandMap[K]["args"]]
): Promise<CommandMap[K]["result"]> {
  return invoke(name, args[0] ?? {}) as Promise<CommandMap[K]["result"]>;
}

export function isReverbError(e: unknown): e is { code: string; message: string } {
  return (
    typeof e === "object" &&
    e !== null &&
    "code" in e &&
    "message" in e &&
    typeof (e as { code: unknown }).code === "string"
  );
}

export function errorMessage(e: unknown): string {
  if (isReverbError(e)) return e.message;
  if (e instanceof Error) return e.message;
  return String(e);
}
