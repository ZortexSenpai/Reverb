// Wire types matching the Rust serde models (camelCase on the wire).

/** One credited artist. */
export interface ArtistRef {
  id: string;
  name: string;
}

/** One genre. Servers wrap genre names in objects rather than sending strings. */
export interface GenreRef {
  name: string;
}

export interface Song {
  id: string;
  title: string;
  album?: string;
  albumId?: string;
  /** Formatted credit for every artist, e.g. "A feat. B". */
  artist?: string;
  /** Only the *first* credited artist — use `artists` to link them all. */
  artistId?: string;
  /** OpenSubsonic: every credited artist, in tag order. */
  artists?: ArtistRef[];
  /** OpenSubsonic's own spelling of `artist`. */
  displayArtist?: string;
  track?: number;
  discNumber?: number;
  year?: number;
  /** Only the first genre — use `genres` for all of them. */
  genre?: string;
  /** OpenSubsonic: every genre the track is tagged with. */
  genres?: GenreRef[];
  coverArt?: string;
  size?: number;
  contentType?: string;
  suffix?: string;
  /** Seconds. */
  duration?: number;
  bitRate?: number;
  samplingRate?: number;
  bitDepth?: number;
  /** ISO date when starred; present only if starred. */
  starred?: string;
  /** This user's 1–5 rating; absent or 0 when unrated. */
  userRating?: number;
  playCount?: number;
  /** ISO date of the last play. */
  played?: string;
  /** ISO date the file was added to the library. */
  created?: string;
  /** OpenSubsonic loudness data (for ReplayGain). */
  replayGain?: ReplayGainData;
  /** Folder browsing: true when this entry is a subdirectory, not a song. */
  isDir?: boolean;
  /** Id of the containing directory, in folder listings. */
  parent?: string;
  path?: string;
}

export interface ReplayGainData {
  trackGain?: number;
  albumGain?: number;
  trackPeak?: number;
  albumPeak?: number;
}

export interface Album {
  id: string;
  name: string;
  /** Formatted credit for every album artist. */
  artist?: string;
  /** Only the *first* album artist — use `artists` to link them all. */
  artistId?: string;
  /** OpenSubsonic: every credited album artist, in tag order. */
  artists?: ArtistRef[];
  /** OpenSubsonic's own spelling of `artist`. */
  displayArtist?: string;
  coverArt?: string;
  songCount: number;
  /** Seconds. */
  duration: number;
  created?: string;
  year?: number;
  /** Only the first genre — use `genres` for all of them. */
  genre?: string;
  /** OpenSubsonic: every genre the album is tagged with. */
  genres?: GenreRef[];
  starred?: string;
  /** This user's 1–5 rating; absent or 0 when unrated. */
  userRating?: number;
  played?: string;
}

export type AlbumWithSongs = Album & { song: Song[] };

export interface Artist {
  id: string;
  name: string;
  coverArt?: string;
  albumCount?: number;
  artistImageUrl?: string;
  starred?: string;
}

export type ArtistWithAlbums = Artist & { album: Album[] };

export interface ArtistIndexGroup {
  name: string;
  artist: Artist[];
}

export interface ArtistsIndexes {
  ignoredArticles?: string;
  index: ArtistIndexGroup[];
}

/** A configured library root on the server (`getMusicFolders`). */
export interface MusicFolder {
  id: string;
  name?: string;
}

/** Top of the folder tree: artist-level dirs by initial, plus loose files. */
export interface Indexes {
  ignoredArticles?: string;
  index: ArtistIndexGroup[];
  child: Song[];
}

/** One directory listing; `child` interleaves subdirectories and songs. */
export interface Directory {
  id: string;
  name?: string;
  parent?: string;
  child: Song[];
}

export interface Playlist {
  id: string;
  name: string;
  comment?: string;
  owner?: string;
  songCount: number;
  /** Seconds. */
  duration: number;
  created?: string;
  changed?: string;
  coverArt?: string;
}

export type PlaylistWithSongs = Playlist & { entry: Song[] };

/** The queue the server keeps per user, shared between clients. */
export interface PlayQueue {
  entry: Song[];
  /** Id of the song that was playing. */
  current?: string | null;
  /** Position within it, ms. */
  position?: number | null;
  /** ISO time of the last save. */
  changed?: string | null;
  /** Client that saved it. */
  changedBy?: string | null;
}

export type MixKind = "daily" | "discover" | "onRepeat" | "deepCuts" | "forgotten";

/** A "Made for you" mix, picked by the backend from the library and the
 *  user's listening. Rebuilt daily (Discover weekly). */
export interface Mix {
  /** Route id, e.g. "daily-2". */
  id: string;
  kind: MixKind;
  /** Daily mixes are numbered from 1. */
  number?: number | null;
  /** What a daily mix is built around, most played first. */
  artists: string[];
  /** The genre a daily mix's artists share, when the library tags one. */
  genre?: string | null;
  songs: Song[];
}

export interface Starred {
  song: Song[];
  album: Album[];
  artist: Artist[];
}

export interface SearchResult {
  song: Song[];
  album: Album[];
  artist: Artist[];
}

export interface ServerInfo {
  version?: string;
  serverType?: string;
  serverVersion?: string;
  openSubsonic: boolean;
}

export type AlbumListType =
  | "newest"
  | "recent"
  | "frequent"
  | "random"
  | "alphabeticalByName"
  | "alphabeticalByArtist"
  | "starred"
  | "byGenre"
  | "byYear";

export interface Genre {
  value: string;
  songCount: number;
  albumCount: number;
}

export interface SongPage {
  songs: Song[];
  /** Total matching songs, when the server reports it (Navidrome native API). */
  total?: number | null;
  /** False when the unsorted Subsonic fallback was used. */
  serverSorted: boolean;
}

export interface LyricsLine {
  /** Start time in ms (synced lyrics only). */
  start?: number | null;
  value: string;
}

export interface StructuredLyrics {
  lang?: string | null;
  synced: boolean;
  /** Global timing offset in ms. */
  offset?: number | null;
  displayArtist?: string | null;
  displayTitle?: string | null;
  line: LyricsLine[];
}

export interface LyricsList {
  structuredLyrics: StructuredLyrics[];
}

export interface SignalPath {
  codec?: string | null;
  sourceSampleRate: number;
  bitRate?: number | null;
  losslessSource: boolean;
  replayGainDb?: number | null;
  resampleTo?: number | null;
  eqBands: number;
  outputSampleRate: number;
  /** Device the stream is open on; null for the system default. */
  outputDevice?: string | null;
  /** Rate the hardware actually runs at; null when it couldn't be confirmed. */
  deviceRate?: number | null;
  /** The OS is resampling below us — bit-perfect is impossible while true. */
  osResampling: boolean;
  quality: "lossless" | "high" | "enhanced";
}

/** A selectable audio output. `id` is the stable device id we persist. */
export interface OutputDevice {
  id: string;
  label: string;
  /** Bypasses the sound server — bit-perfect, but takes the card exclusively. */
  direct: boolean;
}

export type StarKind = "song" | "album" | "artist";

// ---- player / engine ----

export type PlaybackState = "playing" | "paused" | "stopped" | "buffering";

export interface PlayerSnapshot {
  state: PlaybackState;
  trackId?: string | null;
  positionMs: number;
  durationMs: number;
  volume: number;
  sampleRate: number;
}

export interface PlayableTrack {
  trackId: string;
  suffix?: string;
  durationMs?: number;
  title?: string;
  /** The full credit, as the OS media session and the server's scrobble see it. */
  artist?: string;
  /** First credited artist alone, for the Last.fm first-artist-only preference. */
  firstArtist?: string;
  album?: string;
  replayGain?: ReplayGainData;
}

/** One listening of a Song in Reverb on this computer, as the Play History
 *  ("Recently played") keeps it. */
export interface Play {
  /** Unix ms of when the Song started. */
  playedAt: number;
  song: Song;
}

// ---- EQ ----

export type EqFilterType = "peaking" | "lowshelf" | "highshelf" | "lowpass" | "highpass";

export interface EqBand {
  enabled: boolean;
  type: EqFilterType;
  /** Hz, 20..20000 (log scale in the UI). */
  frequency: number;
  /** dB, UI range -18..+18. Ignored for lowpass/highpass. */
  gainDb: number;
  q: number;
}

export type PreampMode = { mode: "auto" } | { mode: "manual"; db: number };

export interface EqState {
  enabled: boolean;
  bands: EqBand[];
  preamp: PreampMode;
}

export interface EqPresetMeta {
  id: string;
  name: string;
}

// ---- auth/settings ----

export interface AuthStatus {
  configured: boolean;
  serverUrl?: string | null;
  username?: string | null;
  insecureStorage: boolean;
}

export type ReplayGainMode = "off" | "track" | "album";

export interface ReplayGainSettings {
  mode: ReplayGainMode;
  preampDb: number;
  clipProtection: boolean;
  /** Gain in dB for tracks without usable ReplayGain tags. */
  fallbackDb: number;
}

export type StreamingFormat = "raw" | "mp3-320" | "mp3-192" | "mp3-128";
export type GridSize = "small" | "medium" | "large";

/** How the playlists view renders: compact rows, or cover-art cards. */
export type PlaylistLayout = "list" | "grid";

/** Shell layout: solid docked panels, or floating translucent glass ones. */
export type LayoutMode = "classic" | "liquid";

/** Full-screen player backdrop: from heavy album-art blur to none at all. */
export type NowPlayingBackdrop = "blurred" | "soft" | "clear" | "solid";

/** Cover size in the full-screen player, as a fraction of viewport height. */
export type NowPlayingArtSize = "small" | "medium" | "large";

/** Whether full-screen player text is centred under the art or ranged left. */
export type NowPlayingAlign = "center" | "left";

export type SeekBarStyle = "line" | "wave" | "bars";
export type SeekBarThickness = "thin" | "medium" | "thick";

export interface Preferences {
  theme: string;
  /**
   * UI language: a locale we ship ("en", "fr", "es", "de"), or "system" to
   * follow the OS. Anything unrecognised resolves the same way as "system",
   * so a value written by a later release degrades instead of breaking.
   */
  language: string;
  accent: string;
  layout: LayoutMode;
  /** Liquid layout glass strength, 0–100 (0 = solid panels, effect off). */
  glassIntensity: number;
  /** How see-through the panels are, 0–100 (0 = as designed). Both layouts. */
  panelTransparency: number;
  /** How album art shows behind the full-screen player. */
  nowPlayingBackdrop: NowPlayingBackdrop;
  nowPlayingArtSize: NowPlayingArtSize;
  nowPlayingAlign: NowPlayingAlign;
  /**
   * Metadata fields shown under the title in the full-screen player, by
   * canonical id (see `nowPlayingFields.ts`). An allowlist rather than a
   * denylist: the row is meant to stay short, so fields added in a later
   * release stay hidden until the user asks for them.
   */
  nowPlayingFields: string[];
  /** Show the track's genres as chips under the album line. */
  nowPlayingGenres: boolean;
  startupView: string;
  gridSize: GridSize;
  playlistLayout: PlaylistLayout;
  audioDevice: string | null;
  /** Switch the device to each track's native sample rate (bit-perfect). */
  matchSourceRate: boolean;
  /** Take the output device exclusively (macOS hog mode / WASAPI exclusive). */
  exclusiveOutput: boolean;
  streamingFormat: StreamingFormat;
  replayGain: ReplayGainSettings;
  /** Crossfade between tracks in seconds (0 = gapless). */
  crossfadeSecs: number;
  /** How the playback position is drawn: flat line, wave, or bars. */
  seekBarStyle: SeekBarStyle;
  /** Animate the wave/bars while playing (ignored by the flat line). */
  seekBarAnimated: boolean;
  seekBarThickness: SeekBarThickness;
  scrobbleEnabled: boolean;
  /** Percentage of a song to play before it counts as played (10–100). */
  scrobbleAtPercent: number;
  /**
   * Send plays straight to Last.fm and leave the Subsonic server out of it —
   * no scrobble, no now-playing entry. Requires Last.fm scrobbling to be
   * connected; the backend ignores it otherwise, so it can never silently
   * stop plays being recorded anywhere.
   */
  lastfmScrobbleOnly: boolean;
  /** Clear `lastfmScrobbleOnly` on the next launch. */
  lastfmScrobbleOnlyResetOnStart: boolean;
  /** Show a `lastfmScrobbleOnly` toggle in the sidebar. */
  lastfmScrobbleOnlyPinned: boolean;
  /**
   * Submit only the first credited artist to Last.fm instead of the whole
   * credit string. Needs the server to enumerate a song's artists; where it
   * doesn't, the credit is one opaque string and goes as-is.
   */
  lastfmScrobbleFirstArtistOnly: boolean;
  /** Visible columns in the songs table. */
  songColumns: string[];
  /** Playlist ids pinned by the user, in pin order. */
  pinnedPlaylists: string[];
  showPinnedInSidebar: boolean;
  /**
   * Sidebar navigation the user has hidden, by route path. A denylist, so
   * navigation added later is visible by default (see `visibleNavItems`).
   */
  hiddenNavItems: string[];
  /** Radio: keep playing similar songs when the queue ends. */
  radioMode: boolean;
  /** Persist the queue and restore it on the next launch. */
  saveQueue: boolean;
  /** Keep the queue on the server too, and pick up one another client saved. */
  syncQueue: boolean;
  /** Look for a new release at launch and every 6 hours. */
  checkForUpdates: boolean;
}

export type FrontendSettings = Preferences & { volume: number };

export interface ScanStatus {
  scanning: boolean;
  count?: number | null;
}

export interface ReverbErrorPayload {
  code: string;
  message: string;
}

// ---- stats ----

export type StatsRange = "day" | "week" | "month" | "year" | "all" | "custom";

/** An artist, album or Song on the stats page. */
export interface StatsItem {
  name: string;
  /** For album and Song entries. */
  artist?: string | null;
  /** The library item's id and cover-art id, for a Library match. */
  id?: string | null;
  coverArt?: string | null;
}

/** A top-list entry's change of place since the Previous period. */
export type RankMovement =
  /** Places up, or down when negative: from 40th to 3rd is 37. */
  | { kind: "moved"; places: number }
  /** No Scrobbles in the Previous period. */
  | { kind: "new" };

export interface TopEntry extends StatsItem {
  count: number;
  /** Since the Previous period, for a top-list entry. Absent for all time. */
  movement?: RankMovement | null;
}

/**
 * Each headline count's change since the Previous period, relative to what it
 * was then: 0.12 for 12% more. Absent when it was nothing then. The current
 * Streak and the Busiest day have none.
 */
export interface HeadlineChanges {
  plays?: number | null;
  /** Also absent when either Period has no library Song length to go by. */
  listeningTime?: number | null;
  artists?: number | null;
  albums?: number | null;
  songs?: number | null;
  playsPerDay?: number | null;
  longestStreak?: number | null;
}

/** The Period's #1 artist, album and Song. */
export interface StatsHero {
  artist: TopEntry;
  /** The artist's share of the Period's plays, from 0 to 1. */
  share: number;
  /** Absent with no album tags, or when the Period has a single Song. */
  album?: TopEntry | null;
  song?: TopEntry | null;
}

export interface TimelinePoint {
  label: string;
  count: number;
}

/** One of the Period's top artists, played per bucket of the activity timeline. */
export interface ArtistTimeline {
  artist: TopEntry;
  /** The same buckets, with the same labels, as `StatsSummary.timeline`. */
  timeline: TimelinePoint[];
}

/** A Scrobble, as the Song it counts toward. */
export interface RecentPlay extends StatsItem {
  ts: number;
  artist: string;
  album?: string | null;
}

/** A run of plays with no long gap between them. */
export interface ListeningSession {
  startTs: number;
  tracks: number;
  spanMinutes: number;
  topArtist: string;
}

export interface SessionStats {
  count: number;
  medianTracks: number;
  /** Biggest first. */
  longest: ListeningSession[];
}

/** The Song played most on a single day of the Period. */
export interface Obsession {
  /** Its `count` is that day's plays. */
  song: TopEntry;
  /** The local date, `YYYY-MM-DD`. */
  date: string;
}

/** An artist first heard in the Period. */
export interface FirstHeard extends StatsItem {
  ts: number;
  plays: number;
}

export interface Discovery {
  newArtists: number;
  newSongs: number;
  firstArtists: FirstHeard[];
}

export interface OnThisDay {
  year: number;
  plays: number;
  /** That day's most played Songs. */
  songs: TopEntry[];
}

/** Plays split by what only the Library match knows, like genres. */
export interface Breakdown<T> {
  entries: T[];
  /** The share of the Period's Scrobbles it's based on, from 0 to 1. */
  coverage: number;
}

export interface GenrePlays {
  name: string;
  count: number;
}

export interface DecadePlays {
  /** Its first year: 1980 for the 1980s. */
  decade: number;
  count: number;
}

/** An estimate of the Period's listening time. */
export interface ListeningTime {
  /** 0 when no matched Song has a known length: nothing to estimate from. */
  secs: number;
  /**
   * The share of the Period's Scrobbles that matched a library Song, from 0
   * to 1. Every play without a known length counts at the median of those
   * with one.
   */
  coverage: number;
}

export interface StatsSummary {
  totalPlays: number;
  uniqueArtists: number;
  uniqueSongs: number;
  uniqueAlbums: number;
  firstTs?: number | null;
  lastTs?: number | null;
  playsPerDay: number;
  /** Absent when the Period has no plays. */
  hero?: StatsHero | null;
  topArtists: TopEntry[];
  topSongs: TopEntry[];
  topAlbums: TopEntry[];
  byHour: number[];
  byWeekday: number[];
  timeline: TimelinePoint[];
  /** The five most played artists, most played first. */
  topArtistsOverTime: ArtistTimeline[];
  recent: RecentPlay[];
  busiestDay?: TimelinePoint | null;
  longestStreakDays: number;
  /**
   * Days in a row with a Scrobble up to today, or up to yesterday while today
   * has none yet. Whatever the Period.
   */
  currentStreakDays: number;
  /** One entry per day for the calendar; empty for short ranges. */
  daily: TimelinePoint[];
  sessions: SessionStats;
  /** Absent when the Period has no plays. */
  obsession?: Obsession | null;
  /** Absent for the all-time range, where everything is "new". */
  discovery?: Discovery | null;
  onThisDay: OnThisDay[];
  /**
   * Whether the Library index was there to match Scrobbles against. Until it
   * is, entries are grouped by name and have no art.
   */
  libraryAvailable: boolean;
  /**
   * Without the Library index, whether it's being built, so the numbers that
   * need it are on their way. When it isn't, they can't be had for now.
   */
  libraryBuilding: boolean;
  /** The ten most played genres. Absent until the Library index is there. */
  genres?: Breakdown<GenrePlays> | null;
  /** Plays per release decade, oldest first. Absent until the Library index is there. */
  decades?: Breakdown<DecadePlays> | null;
  /** Estimated from library Songs' lengths. Absent until the Library index is there. */
  listeningTime?: ListeningTime | null;
  /** Since the Previous period. Absent for all time, which has none. */
  changes?: HeadlineChanges | null;
}

export interface LastfmStatus {
  username: string;
  trackCount: number;
  lastSyncAt?: string | null;
  /**
   * An authenticated session for *submitting* plays exists. Importing needs
   * only an API key, so a source can be connected without this.
   */
  scrobbling: boolean;
}

export interface SourcesStatus {
  lastfm?: LastfmStatus | null;
  importing: boolean;
}

export interface ArtistInfo {
  biography?: string | null;
  lastFmUrl?: string | null;
  similarArtists: Artist[];
}
