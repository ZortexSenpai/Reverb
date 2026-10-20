//! "Made for you" mixes: personal playlists in the spirit of Spotify's,
//! picked from the user's own library. Everything here is pure — the command
//! layer fetches the library, the scrobbles and similar artists and hands
//! them in — so the picking rules stay testable.
//!
//! Two kinds of listening data feed it. Navidrome knows each song's lifetime
//! play count and last play, keyed by song id. Imported Last.fm scrobbles know
//! every play with its time, but only as artist/title text, so they are
//! matched back to library songs by normalized name. Either source alone is
//! enough. With both, a song's plays are the larger of the two counts (the
//! same play usually reaches both) and recent listening comes from the
//! scrobbles.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

use reverb_core::subsonic::models::Child;
use serde::{Deserialize, Serialize};

use crate::stats::store::StatScrobble;

const DAY: i64 = 86_400;
/// Intros, skits and interludes never make a mix.
const MIN_DURATION_SECS: u32 = 60;
/// The "On Repeat" window.
const RECENT_DAYS: i64 = 30;
/// How long a favourite goes unplayed before it counts as forgotten.
const FORGOTTEN_AFTER_DAYS: i64 = 180;
/// A play's weight in artist affinity halves over this many days.
const AFFINITY_HALF_LIFE_DAYS: f64 = 90.0;
/// Below this affinity an artist is still new to the listener (about one
/// recent play)...
const FAMILIAR_AFFINITY: f64 = 1.0;
/// ...as long as they also have fewer lifetime plays per song than this.
const FAMILIAR_PLAYS_PER_SONG: f64 = 2.0;
/// A song played at most this often, and not for a while, is still new.
const BARELY_HEARD_PLAYS: u64 = 2;
const BARELY_HEARD_QUIET_DAYS: i64 = 90;

const MAX_DAILY_MIXES: usize = 4;
const DAILY_MIX_SIZE: usize = 40;
const MIN_DAILY_MIX_SIZE: usize = 15;
/// Favourite artists considered for daily mixes.
const DAILY_SEED_ARTISTS: usize = 30;
/// Artists a daily mix is built around; a bigger genre splits across mixes.
const ARTISTS_PER_DAILY_MIX: usize = 6;
/// A split leaving fewer artists than this joins the one before it instead.
const MIN_ARTISTS_PER_SPLIT: usize = 3;
/// Songs per guest (similar or same-genre) artist in a daily mix.
const GUEST_SONGS_PER_ARTIST: usize = 2;

const SMALL_MIX_SIZE: usize = 30;
const MIN_MIX_SIZE: usize = 8;
/// Favourite artists whose unplayed songs make "Deep cuts".
const DEEP_CUT_ARTISTS: usize = 25;
/// Favourite artists whose similar artists seed "Discover".
const DISCOVER_SEED_ARTISTS: usize = 12;
const DISCOVER_GENRES: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MixKind {
    Daily,
    Discover,
    OnRepeat,
    DeepCuts,
    Forgotten,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mix {
    /// Route id, stable for as long as the mix is: "daily-2", "discover", …
    pub id: String,
    pub kind: MixKind,
    /// Daily mixes are numbered from 1.
    #[serde(default)]
    pub number: Option<u32>,
    /// What a daily mix is built around, most played first.
    #[serde(default)]
    pub artists: Vec<String>,
    /// The genre a daily mix's artists share, when the library tags one.
    #[serde(default)]
    pub genre: Option<String>,
    pub songs: Vec<Child>,
}

/// One library song with everything the pickers need.
struct Track {
    song: Child,
    /// Folded keys of every credited artist, primary first.
    artists: Vec<String>,
    /// `artists[0]`, or empty for an uncredited song.
    primary: String,
    /// Folded genre key.
    genre: Option<String>,
    /// Artist and title without version notes. The same recording on an
    /// album and a compilation shares one, so a mix never holds both.
    dedupe_key: String,
    /// Rated one star, or too short to belong in a mix.
    eligible: bool,
    plays: u64,
    /// Matched scrobbles within [`RECENT_DAYS`].
    recent_plays: u32,
    last_played: Option<i64>,
    /// How much the listener likes it, from plays, star and rating.
    love: f64,
    /// How new it still is to the listener: 1 never played, ½ barely heard,
    /// 0 known.
    novelty: f64,
}

/// The listener's library and habits, ready for picking.
pub struct Profile {
    tracks: Vec<Track>,
    /// Folded artist key → recency-weighted plays.
    affinity: HashMap<String, f64>,
    artist_names: HashMap<String, String>,
    artist_ids: HashMap<String, String>,
    genre_names: HashMap<String, String>,
    /// Scrobbles reach into the On Repeat window, so they (rather than
    /// Navidrome's lifetime counts) decide it.
    recent_history: bool,
    /// Scrobbles that found their library song.
    pub matched_scrobbles: usize,
}

/// Lowercase alphanumerics only, so "AC/DC" and "ac dc" meet as "acdc". Text
/// made only of punctuation keeps its lowercase form rather than vanish.
fn fold(s: &str) -> String {
    let folded: String =
        s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect();
    if folded.is_empty() {
        s.trim().to_lowercase()
    } else {
        folded
    }
}

/// Words that mark a " - …" title suffix as a version note.
const VERSION_MARKERS: &[&str] = &[
    "remaster", "version", "edit", "mix", "live", "mono", "stereo", "acoustic", "demo",
    "deluxe", "bonus", "explicit", "instrumental",
];

/// Words that make a note name a different recording, not a version of the
/// same one: "(Someone Remix)" and "VIP" stay part of a title.
const DISTINCT_MARKERS: &[&str] =
    &["remix", "vip", "bootleg", "flip", "rework", "refix", "cover", "mashup"];

const FEAT_MARKERS: &[&str] = &[" feat. ", " feat ", " ft. ", " featuring "];

fn names_distinct_recording(note: &str) -> bool {
    let note = note.to_ascii_lowercase();
    DISTINCT_MARKERS.iter().any(|m| note.contains(m))
}

/// A title folded without the notes scrobblers and taggers disagree on —
/// "(Original Mix)", "[Radio Edit]", "- 2011 Remaster", "feat. X" — but with
/// the ones that make it another track, so "Song - X Remix" and
/// "Song (X Remix)" meet while "Song" stays apart from both.
fn loose_title(title: &str) -> String {
    let mut t = title.trim().to_owned();
    // ASCII lowercasing keeps byte offsets, so they index the original.
    let lower = t.to_ascii_lowercase();
    if let Some(at) = FEAT_MARKERS.iter().filter_map(|m| lower.find(m)).min() {
        // "feat. X" runs up to the next note, if there is one.
        let end = [" - ", "(", "["]
            .iter()
            .filter_map(|m| lower[at + 1..].find(m).map(|e| at + 1 + e))
            .min()
            .unwrap_or(t.len());
        t.replace_range(at..end, "");
    }

    let mut kept = String::with_capacity(t.len());
    let mut rest = t.as_str();
    while let Some(open) = rest.find(['(', '[']) {
        let close = if rest[open..].starts_with('(') { ')' } else { ']' };
        let Some(len) = rest[open..].find(close) else { break };
        let group = &rest[open..=open + len];
        kept.push_str(&rest[..open]);
        if names_distinct_recording(group) {
            kept.push_str(group);
        }
        rest = &rest[open + len + 1..];
    }
    kept.push_str(rest);

    let mut t = kept.trim_end();
    if let Some(at) = t.rfind(" - ") {
        let note = &t[at + 3..];
        let lower = note.to_ascii_lowercase();
        if VERSION_MARKERS.iter().any(|m| lower.contains(m)) && !names_distinct_recording(note) {
            t = t[..at].trim_end();
        }
    }
    if t.trim().is_empty() {
        fold(title)
    } else {
        fold(t)
    }
}

/// " • " is how Navidrome joins a credit, which is what gets scrobbled when
/// the whole credit is sent.
const CREDIT_SEPARATORS: &[&str] = &[
    ", ", " & ", " and ", " x ", " / ", "; ", " • ", " feat. ", " feat ", " ft. ",
    " featuring ", " with ", " vs. ", " vs ",
];

/// Folded keys a scrobbled artist credit can match: the whole credit, then
/// each artist in it.
fn credit_keys(credit: &str) -> Vec<String> {
    let mut keys = vec![fold(credit)];
    let lower = credit.to_ascii_lowercase();
    let mut cuts: Vec<(usize, usize)> = CREDIT_SEPARATORS
        .iter()
        .flat_map(|sep| lower.match_indices(sep).map(|(at, s)| (at, at + s.len())))
        .collect();
    cuts.sort_unstable();
    let mut start = 0;
    let mut parts = Vec::new();
    for (at, end) in cuts {
        // Where separators overlap (", " then " and " in "A, and B"), the
        // first one wins.
        if at < start {
            continue;
        }
        parts.push(&credit[start..at]);
        start = end;
    }
    if start > 0 {
        parts.push(&credit[start..]);
    }
    for part in parts {
        let key = fold(part);
        if !key.is_empty() && !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

/// Every credited artist as (name, id): the OpenSubsonic list when the
/// server sends one, else the legacy single artist.
fn credited_artists(song: &Child) -> Vec<(String, Option<String>)> {
    if !song.artists.is_empty() {
        return song.artists.iter().map(|a| (a.name.clone(), Some(a.id.clone()))).collect();
    }
    song.artist.iter().map(|name| (name.clone(), song.artist_id.clone())).collect()
}

fn name_key(artist: &str, title: &str) -> String {
    format!("{artist}\u{1}{title}")
}

fn parse_ts(iso: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(iso).ok().map(|t| t.timestamp())
}

/// Weight of something `age_secs` old that halves every `half_life_days`.
fn decay(age_secs: i64, half_life_days: f64) -> f64 {
    0.5f64.powf(age_secs.max(0) as f64 / (half_life_days * DAY as f64))
}

fn love(plays: u64, starred: bool, rating: Option<u32>) -> f64 {
    let mut love = (plays as f64).ln_1p();
    if starred {
        love += 2.0;
    }
    love + match rating {
        Some(5) => 2.0,
        Some(4) => 1.0,
        Some(2) => -1.0,
        _ => 0.0,
    }
}

/// Match a scrobble to a library song: exact titles first, then without
/// version notes, trying the whole artist credit before its parts.
fn find_song(
    exact: &HashMap<String, usize>,
    loose: &HashMap<String, usize>,
    artist_keys: &[String],
    title: &str,
) -> Option<usize> {
    let exact_title = fold(title);
    let found = artist_keys.iter().find_map(|a| exact.get(&name_key(a, &exact_title)));
    if let Some(&i) = found {
        return Some(i);
    }
    let loose_title = loose_title(title);
    artist_keys.iter().find_map(|a| loose.get(&name_key(a, &loose_title)).copied())
}

pub fn build_profile(songs: Vec<Child>, scrobbles: &[StatScrobble], now: i64) -> Profile {
    let mut artist_names = HashMap::new();
    let mut artist_ids = HashMap::new();
    let mut genre_names = HashMap::new();
    let mut seen = HashSet::new();
    let mut tracks = Vec::with_capacity(songs.len());

    for song in songs {
        // Library pages can overlap where the listing's sort order has ties.
        if song.is_dir || !seen.insert(song.id.clone()) {
            continue;
        }
        let mut artists: Vec<String> = Vec::new();
        for (name, id) in credited_artists(&song) {
            let key = fold(&name);
            if key.is_empty() || artists.contains(&key) {
                continue;
            }
            if let Some(id) = id {
                artist_ids.entry(key.clone()).or_insert(id);
            }
            artist_names.entry(key.clone()).or_insert(name);
            artists.push(key);
        }
        let genre = song
            .genre
            .clone()
            .or_else(|| song.genres.first().map(|g| g.name.clone()))
            .filter(|g| !g.trim().is_empty())
            .map(|name| {
                let key = fold(&name);
                genre_names.entry(key.clone()).or_insert(name);
                key
            });
        let primary = artists.first().cloned().unwrap_or_default();
        let dedupe_key = name_key(&primary, &loose_title(&song.title));
        let eligible = song.user_rating != Some(1)
            && song.duration.is_none_or(|d| d >= MIN_DURATION_SECS);
        let plays = song.play_count.unwrap_or(0);
        let last_played = song.played.as_deref().and_then(parse_ts);
        tracks.push(Track {
            song,
            artists,
            primary,
            genre,
            dedupe_key,
            eligible,
            plays,
            recent_plays: 0,
            last_played,
            love: 0.0,
            novelty: 0.0,
        });
    }

    // Title indexes for scrobble matching, under every way a song's artist
    // can be written. Where titles collide, the most played song wins.
    let mut exact: HashMap<String, usize> = HashMap::new();
    let mut loose: HashMap<String, usize> = HashMap::new();
    for (i, track) in tracks.iter().enumerate() {
        let song = &track.song;
        let mut keys = track.artists.clone();
        for credit in [&song.artist, &song.display_artist].into_iter().flatten() {
            let key = fold(credit);
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        let titles = [fold(&song.title), loose_title(&song.title)];
        for artist in &keys {
            for (index, title) in [(&mut exact, &titles[0]), (&mut loose, &titles[1])] {
                let slot = index.entry(name_key(artist, title)).or_insert(i);
                if tracks[i].plays > tracks[*slot].plays {
                    *slot = i;
                }
            }
        }
    }

    let known: HashSet<String> = tracks.iter().flat_map(|t| t.artists.iter().cloned()).collect();
    let mut affinity: HashMap<String, f64> = HashMap::new();
    // Per track: (plays, plays within the On Repeat window, last play).
    let mut scrobbled = vec![(0u64, 0u32, None::<i64>); tracks.len()];
    let mut matched_scrobbles = 0;
    // The same few thousand songs recur across a long history.
    let mut credit_cache: HashMap<&str, Vec<String>> = HashMap::new();
    let mut match_cache: HashMap<(&str, &str), Option<usize>> = HashMap::new();
    for s in scrobbles {
        let keys = credit_cache.entry(s.artist.as_str()).or_insert_with(|| credit_keys(&s.artist));
        // Affinity goes by artist name, so plays of songs that aren't in the
        // library (or don't match) still count toward their artist.
        let weight = decay(now - s.ts, AFFINITY_HALF_LIFE_DAYS);
        for key in keys.iter().filter(|k| known.contains(*k)) {
            *affinity.entry(key.clone()).or_default() += weight;
        }
        let hit = *match_cache
            .entry((s.artist.as_str(), s.track.as_str()))
            .or_insert_with(|| find_song(&exact, &loose, keys, &s.track));
        if let Some(i) = hit {
            matched_scrobbles += 1;
            let (plays, recent, last) = &mut scrobbled[i];
            *plays += 1;
            if now - s.ts <= RECENT_DAYS * DAY {
                *recent += 1;
            }
            *last = (*last).max(Some(s.ts));
        }
    }

    let mut played_affinity: HashMap<String, f64> = HashMap::new();
    for (track, (plays, recent, last)) in tracks.iter_mut().zip(scrobbled) {
        if track.plays > 0 {
            // Navidrome keeps only a lifetime count and the last play, so the
            // whole count ages with that last play.
            let aged = track.last_played.map_or(0.5, |ts| decay(now - ts, AFFINITY_HALF_LIFE_DAYS));
            for artist in &track.artists {
                *played_affinity.entry(artist.clone()).or_default() += track.plays as f64 * aged;
            }
        }
        track.plays = track.plays.max(plays);
        track.recent_plays = recent;
        track.last_played = track.last_played.max(last);
        track.love = love(track.plays, track.song.starred.is_some(), track.song.user_rating);
        // In a library that's been played through, "new" has to include the
        // songs that only ever got a play or two.
        track.novelty = if track.plays == 0 {
            1.0
        } else if track.plays <= BARELY_HEARD_PLAYS
            && track.last_played.is_none_or(|ts| now - ts > BARELY_HEARD_QUIET_DAYS * DAY)
        {
            0.5
        } else {
            0.0
        };
    }
    // Both sources mostly see the same plays, so take the larger rather than
    // count them twice.
    for (artist, weight) in played_affinity {
        let slot = affinity.entry(artist).or_default();
        *slot = slot.max(weight);
    }

    Profile {
        tracks,
        affinity,
        artist_names,
        artist_ids,
        genre_names,
        recent_history: scrobbles.iter().any(|s| now - s.ts <= RECENT_DAYS * DAY),
        matched_scrobbles,
    }
}

impl Profile {
    /// Artists by affinity, strongest first.
    fn ranked_artists(&self) -> Vec<(&str, f64)> {
        let mut ranked: Vec<(&str, f64)> = self
            .affinity
            .iter()
            .filter(|(_, &w)| w > 0.0)
            .map(|(artist, &w)| (artist.as_str(), w))
            .collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        ranked
    }

    /// The most listened artists with their library ids, strongest first —
    /// the seeds for similar-artist lookups.
    pub fn top_artist_ids(&self, n: usize) -> Vec<(String, String)> {
        self.ranked_artists()
            .into_iter()
            .filter_map(|(artist, _)| Some((artist.to_owned(), self.artist_ids.get(artist)?.clone())))
            .take(n)
            .collect()
    }

    /// Songs in the library, and how many of them were never played.
    pub fn coverage(&self) -> (usize, usize) {
        (self.tracks.len(), self.tracks.iter().filter(|t| t.plays == 0).count())
    }

    fn affinity(&self, artist: &str) -> f64 {
        self.affinity.get(artist).copied().unwrap_or(0.0)
    }

    fn artist_name(&self, artist: &str) -> String {
        self.artist_names.get(artist).cloned().unwrap_or_else(|| artist.to_owned())
    }

    /// Each artist's most common genre across the songs they lead.
    fn artist_genres(&self) -> HashMap<&str, &str> {
        let mut counts: HashMap<(&str, &str), usize> = HashMap::new();
        for track in &self.tracks {
            if let Some(genre) = track.genre.as_deref().filter(|_| !track.primary.is_empty()) {
                *counts.entry((&track.primary, genre)).or_default() += 1;
            }
        }
        let mut best: HashMap<&str, (&str, usize)> = HashMap::new();
        for ((artist, genre), n) in counts {
            let slot = best.entry(artist).or_insert((genre, n));
            if n > slot.1 || (n == slot.1 && genre < slot.0) {
                *slot = (genre, n);
            }
        }
        best.into_iter().map(|(artist, (genre, _))| (artist, genre)).collect()
    }

    fn mix(&self, id: &str, kind: MixKind, picked: &[usize]) -> Mix {
        Mix {
            id: id.to_owned(),
            kind,
            number: None,
            artists: Vec::new(),
            genre: None,
            songs: picked.iter().map(|&i| self.tracks[i].song.clone()).collect(),
        }
    }
}

/// SplitMix64 — tiny, and plenty for shuffling playlists.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in (0, 1], so its logarithm is always finite.
    fn unit(&mut self) -> f64 {
        ((self.next_u64() >> 11) + 1) as f64 / (1u64 << 53) as f64
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = (self.next_u64() % (i as u64 + 1)) as usize;
            items.swap(i, j);
        }
    }
}

/// What a mix under construction already holds.
struct Taken<'p> {
    songs: HashSet<usize>,
    keys: HashSet<&'p str>,
    per_artist: HashMap<&'p str, usize>,
}

impl<'p> Taken<'p> {
    /// Starts out holding `used`, so mixes built in turn don't repeat each
    /// other.
    fn excluding(used: &HashSet<usize>, profile: &'p Profile) -> Self {
        Self {
            songs: used.clone(),
            keys: used.iter().map(|&i| profile.tracks[i].dedupe_key.as_str()).collect(),
            per_artist: HashMap::new(),
        }
    }
}

/// Weighted sampling without replacement (Efraimidis–Spirakis): each
/// candidate draws `ln(u) / weight` and the largest draws win, so heavy songs
/// tend to come first without light ones ever being ruled out.
fn pick<'p>(
    profile: &'p Profile,
    candidates: &[(usize, f64)],
    n: usize,
    per_artist: usize,
    taken: &mut Taken<'p>,
    rng: &mut Rng,
) -> Vec<usize> {
    let mut drawn: Vec<(f64, usize)> = candidates
        .iter()
        .filter(|(i, w)| *w > 0.0 && !taken.songs.contains(i))
        .map(|&(i, w)| (rng.unit().ln() / w, i))
        .collect();
    drawn.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut picked = Vec::new();
    for (_, i) in drawn {
        if picked.len() == n {
            break;
        }
        let track = &profile.tracks[i];
        let count = taken.per_artist.entry(track.primary.as_str()).or_default();
        if *count >= per_artist || taken.keys.contains(track.dedupe_key.as_str()) {
            continue;
        }
        *count += 1;
        taken.songs.insert(i);
        taken.keys.insert(track.dedupe_key.as_str());
        picked.push(i);
    }
    picked
}

/// Reorder so one artist never plays twice in a row where that can be
/// avoided, otherwise keeping the order. The opener stays put.
fn space_artists(profile: &Profile, songs: &mut Vec<usize>) {
    let artist = |i: usize| profile.tracks[i].primary.as_str();
    if songs.len() < 3 {
        return;
    }
    let mut left: Vec<usize> = songs.drain(1..).collect();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for &i in &left {
        *counts.entry(artist(i)).or_default() += 1;
    }
    while !left.is_empty() {
        let previous = artist(*songs.last().expect("the opener stays"));
        // An artist holding more than half of what's left has to go now, or
        // it ends up back to back.
        let urgent = counts.iter().find(|(_, &n)| n * 2 > left.len()).map(|(&a, _)| a);
        let at = match urgent {
            Some(a) if a != previous => left.iter().position(|&i| artist(i) == a),
            _ => left.iter().position(|&i| artist(i) != previous),
        };
        let i = left.remove(at.unwrap_or(0));
        *counts.get_mut(artist(i)).expect("counted above") -= 1;
        songs.push(i);
    }
}

/// Songs per artist for a mix drawn from `artists` artists: a few each when
/// there are many, more when only a handful qualify.
fn per_artist_cap(artists: usize, floor: usize) -> usize {
    (SMALL_MIX_SIZE / artists.max(1)).max(floor)
}

fn distinct_artists(profile: &Profile, candidates: &[(usize, f64)]) -> usize {
    candidates.iter().map(|&(i, _)| profile.tracks[i].primary.as_str()).collect::<HashSet<_>>().len()
}

/// Shuffle a finished pick and space out its artists.
fn deal(profile: &Profile, songs: &mut Vec<usize>, rng: &mut Rng) {
    rng.shuffle(songs);
    space_artists(profile, songs);
}

pub fn generate(
    profile: &Profile,
    similar: &HashMap<String, Vec<String>>,
    now: i64,
    seed: u64,
) -> Vec<Mix> {
    let mut rng = Rng(seed);
    let similar: HashMap<&str, Vec<String>> = similar
        .iter()
        .map(|(artist, names)| (artist.as_str(), names.iter().map(|n| fold(n)).collect()))
        .collect();
    let genres = profile.artist_genres();
    // Songs already in a mix; the later mixes leave them out.
    let mut used = HashSet::new();

    let mut mixes = daily_mixes(profile, &similar, &genres, &mut rng, &mut used);
    mixes.extend(discover(profile, &similar, &genres, &mut rng, &mut used));
    mixes.extend(on_repeat(profile, now));
    mixes.extend(deep_cuts(profile, &mut rng, &mut used));
    mixes.extend(forgotten(profile, now, &mut rng, &mut used));
    mixes
}

/// `len` items split into runs of `size`, a short tail folded into the run
/// before it.
fn splits(len: usize, size: usize) -> Vec<Range<usize>> {
    let mut runs: Vec<Range<usize>> =
        (0..len).step_by(size).map(|start| start..(start + size).min(len)).collect();
    if runs.len() > 1 && runs.last().is_some_and(|r| r.len() < MIN_ARTISTS_PER_SPLIT) {
        let tail = runs.pop().expect("checked non-empty");
        runs.last_mut().expect("checked len > 1").end = tail.end;
    }
    runs
}

/// Favourite artists sharing a genre, with their affinities.
type Cluster<'a> = (Option<&'a str>, Vec<(&'a str, f64)>);

/// Daily mixes: the favourite artists clustered by genre, one mix per
/// cluster (big clusters split), taking turns between genres so the mixes
/// differ from one another.
fn daily_mixes(
    profile: &Profile,
    similar: &HashMap<&str, Vec<String>>,
    genres: &HashMap<&str, &str>,
    rng: &mut Rng,
    used: &mut HashSet<usize>,
) -> Vec<Mix> {
    let ranked: Vec<(&str, f64)> =
        profile.ranked_artists().into_iter().take(DAILY_SEED_ARTISTS).collect();

    let mut clusters: Vec<Cluster> = Vec::new();
    for &(artist, weight) in &ranked {
        let genre = genres.get(artist).copied();
        match clusters.iter_mut().find(|(g, _)| *g == genre) {
            Some((_, members)) => members.push((artist, weight)),
            None => clusters.push((genre, vec![(artist, weight)])),
        }
    }
    let total = |members: &[(&str, f64)]| members.iter().map(|(_, w)| w).sum::<f64>();
    clusters.sort_by(|a, b| total(&b.1).total_cmp(&total(&a.1)));

    let split: Vec<Vec<Range<usize>>> =
        clusters.iter().map(|(_, members)| splits(members.len(), ARTISTS_PER_DAILY_MIX)).collect();
    let rounds = split.iter().map(Vec::len).max().unwrap_or(0);
    let turns = (0..rounds).flat_map(|round| {
        clusters.iter().zip(&split).filter_map(move |((genre, members), runs)| {
            runs.get(round).map(|run| (*genre, &members[run.clone()]))
        })
    });

    let mut mixes = Vec::new();
    for (genre, seeds) in turns {
        if mixes.len() == MAX_DAILY_MIXES {
            break;
        }
        let Some(songs) = daily_mix(profile, seeds, genre, similar, genres, rng, used) else {
            continue;
        };
        let number = mixes.len() as u32 + 1;
        let mut mix = profile.mix(&format!("daily-{number}"), MixKind::Daily, &songs);
        mix.number = Some(number);
        mix.artists = seeds.iter().take(3).map(|(artist, _)| profile.artist_name(artist)).collect();
        mix.genre = genre.and_then(|g| profile.genre_names.get(g).cloned());
        mixes.push(mix);
    }
    mixes
}

/// One daily mix around `seeds`: mostly their songs the listener plays, some
/// of theirs still new to them, and a few from similar or same-genre artists.
fn daily_mix(
    profile: &Profile,
    seeds: &[(&str, f64)],
    genre: Option<&str>,
    similar: &HashMap<&str, Vec<String>>,
    genres: &HashMap<&str, &str>,
    rng: &mut Rng,
    used: &mut HashSet<usize>,
) -> Option<Vec<usize>> {
    let top = seeds.first()?.1;
    // The square root flattens the curve, so the cluster's last artist still
    // gets airtime next to its first.
    let seed_weight: HashMap<&str, f64> =
        seeds.iter().map(|&(artist, w)| (artist, (w / top).sqrt())).collect();

    let mut guests: HashMap<&str, f64> = HashMap::new();
    for (seed, _) in seeds {
        for name in similar.get(seed).into_iter().flatten() {
            if !seed_weight.contains_key(name.as_str()) {
                *guests.entry(name.as_str()).or_default() += 2.0;
            }
        }
    }
    if let Some(genre) = genre {
        for (&artist, &g) in genres {
            if g == genre && !seed_weight.contains_key(artist) {
                *guests.entry(artist).or_default() += 1.0;
            }
        }
    }

    let (mut favourites, mut fresh, mut guest_songs) = (Vec::new(), Vec::new(), Vec::new());
    for (i, track) in profile.tracks.iter().enumerate().filter(|(_, t)| t.eligible) {
        if let Some(&w) = seed_weight.get(track.primary.as_str()) {
            if track.novelty > 0.0 {
                fresh.push((i, w * track.novelty));
            } else {
                favourites.push((i, w * track.love.max(0.1)));
            }
        } else if let Some(&w) = guests.get(track.primary.as_str()) {
            guest_songs.push((i, w));
        }
    }

    let mut taken = Taken::excluding(used, profile);
    let per_artist = (DAILY_MIX_SIZE / seeds.len() + 2).max(4);
    let mut songs = pick(profile, &favourites, DAILY_MIX_SIZE * 3 / 5, per_artist, &mut taken, rng);
    let opener = songs.first().copied();
    songs.extend(pick(profile, &fresh, DAILY_MIX_SIZE / 5, per_artist, &mut taken, rng));
    songs.extend(pick(profile, &guest_songs, DAILY_MIX_SIZE / 5, GUEST_SONGS_PER_ARTIST, &mut taken, rng));
    // Whatever one pool can't supply, the others make up.
    for (pool, cap) in
        [(&favourites, per_artist), (&fresh, per_artist), (&guest_songs, GUEST_SONGS_PER_ARTIST)]
    {
        let missing = DAILY_MIX_SIZE - songs.len();
        songs.extend(pick(profile, pool, missing, cap, &mut taken, rng));
    }
    if songs.len() < MIN_DAILY_MIX_SIZE {
        return None;
    }
    used.extend(&songs);
    rng.shuffle(&mut songs);
    // Open with something familiar.
    if let Some(at) = opener.and_then(|first| songs.iter().position(|&i| i == first)) {
        songs[..=at].rotate_right(1);
    }
    space_artists(profile, &mut songs);
    Some(songs)
}

/// Songs still new to the listener by artists they barely know, drawn toward
/// artists similar to their favourites and toward their favourite genres.
fn discover(
    profile: &Profile,
    similar: &HashMap<&str, Vec<String>>,
    genres: &HashMap<&str, &str>,
    rng: &mut Rng,
    used: &mut HashSet<usize>,
) -> Option<Mix> {
    let ranked = profile.ranked_artists();

    let mut genre_weight: HashMap<&str, f64> = HashMap::new();
    for &(artist, w) in &ranked {
        if let Some(&genre) = genres.get(artist) {
            *genre_weight.entry(genre).or_default() += w;
        }
    }
    let mut top_genres: Vec<(&str, f64)> = genre_weight.into_iter().collect();
    top_genres.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    top_genres.truncate(DISCOVER_GENRES);

    let mut scores: HashMap<&str, f64> = HashMap::new();
    let seeds = &ranked[..ranked.len().min(DISCOVER_SEED_ARTISTS)];
    for (rank, (seed, _)) in seeds.iter().enumerate() {
        let closeness = 1.0 - rank as f64 / seeds.len() as f64;
        for name in similar.get(seed).into_iter().flatten() {
            *scores.entry(name.as_str()).or_default() += 2.0 * closeness;
        }
    }
    for (rank, (genre, _)) in top_genres.iter().enumerate() {
        let closeness = 1.0 - rank as f64 / DISCOVER_GENRES as f64;
        for (&artist, g) in genres {
            if g == genre {
                *scores.entry(artist).or_default() += closeness;
            }
        }
    }
    // Lifetime plays too: recency-weighted affinity alone would take an artist
    // played to death two years ago for a new one.
    let mut heard: HashMap<&str, (u64, u64)> = HashMap::new();
    for track in &profile.tracks {
        let (plays, songs) = heard.entry(&track.primary).or_default();
        *plays += track.plays;
        *songs += 1;
    }
    scores.retain(|artist, _| {
        let barely_played = heard
            .get(artist)
            .is_some_and(|&(plays, songs)| (plays as f64) < FAMILIAR_PLAYS_PER_SONG * songs as f64);
        barely_played && profile.affinity(artist) < FAMILIAR_AFFINITY
    });

    let candidates: Vec<(usize, f64)> = profile
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| t.eligible && t.novelty > 0.0)
        .filter_map(|(i, t)| Some((i, scores.get(t.primary.as_str())? * t.novelty)))
        .collect();
    let mut taken = Taken::excluding(used, profile);
    let cap = per_artist_cap(distinct_artists(profile, &candidates), 2);
    let mut songs = pick(profile, &candidates, SMALL_MIX_SIZE, cap, &mut taken, rng);
    if songs.len() < MIN_MIX_SIZE {
        return None;
    }
    used.extend(&songs);
    deal(profile, &mut songs, rng);
    Some(profile.mix("discover", MixKind::Discover, &songs))
}

/// What the listener has been playing most over the last month.
fn on_repeat(profile: &Profile, now: i64) -> Option<Mix> {
    let score = |t: &Track| {
        if profile.recent_history {
            u64::from(t.recent_plays)
        } else if t.last_played.is_some_and(|ts| now - ts <= RECENT_DAYS * DAY) {
            // Navidrome keeps no history: the best it can say is "played
            // lately, and played a lot overall".
            t.plays
        } else {
            0
        }
    };
    let mut ranked: Vec<usize> = (0..profile.tracks.len())
        .filter(|&i| profile.tracks[i].eligible && score(&profile.tracks[i]) >= 2)
        .collect();
    ranked.sort_by_key(|&i| {
        let t = &profile.tracks[i];
        (Reverse(score(t)), Reverse(t.last_played))
    });
    let mut keys = HashSet::new();
    ranked.retain(|&i| keys.insert(profile.tracks[i].dedupe_key.as_str()));
    ranked.truncate(SMALL_MIX_SIZE);
    (ranked.len() >= MIN_MIX_SIZE).then(|| profile.mix("on-repeat", MixKind::OnRepeat, &ranked))
}

/// Songs by the listener's favourite artists that are still new to them.
fn deep_cuts(profile: &Profile, rng: &mut Rng, used: &mut HashSet<usize>) -> Option<Mix> {
    let ranked = profile.ranked_artists();
    let &(_, top) = ranked.first()?;
    let weight: HashMap<&str, f64> = ranked
        .iter()
        .take(DEEP_CUT_ARTISTS)
        .map(|&(artist, w)| (artist, (w / top).sqrt()))
        .collect();
    let candidates: Vec<(usize, f64)> = profile
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| t.eligible && t.novelty > 0.0)
        .filter_map(|(i, t)| Some((i, weight.get(t.primary.as_str())? * t.novelty)))
        .collect();
    let mut taken = Taken::excluding(used, profile);
    let cap = per_artist_cap(distinct_artists(profile, &candidates), 3);
    let mut songs = pick(profile, &candidates, SMALL_MIX_SIZE, cap, &mut taken, rng);
    if songs.len() < MIN_MIX_SIZE {
        return None;
    }
    used.extend(&songs);
    deal(profile, &mut songs, rng);
    Some(profile.mix("deep-cuts", MixKind::DeepCuts, &songs))
}

/// Starred, highly rated or heavily played songs that haven't been played in
/// half a year.
fn forgotten(profile: &Profile, now: i64, rng: &mut Rng, used: &mut HashSet<usize>) -> Option<Mix> {
    // "Heavily played" is relative: the top fifth of the songs played at all.
    let mut counts: Vec<u64> = profile.tracks.iter().map(|t| t.plays).filter(|&p| p > 0).collect();
    counts.sort_unstable();
    let heavy = counts.get(counts.len() * 4 / 5).copied().unwrap_or(u64::MAX).max(3);
    let cutoff = now - FORGOTTEN_AFTER_DAYS * DAY;

    let candidates: Vec<(usize, f64)> = profile
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            let loved = t.song.starred.is_some()
                || t.song.user_rating.is_some_and(|r| r >= 4)
                || t.plays >= heavy;
            t.eligible && loved && t.last_played.is_none_or(|ts| ts < cutoff)
        })
        .map(|(i, t)| (i, t.love.max(0.1)))
        .collect();
    let mut taken = Taken::excluding(used, profile);
    let cap = per_artist_cap(distinct_artists(profile, &candidates), 3);
    let mut songs = pick(profile, &candidates, SMALL_MIX_SIZE, cap, &mut taken, rng);
    if songs.len() < MIN_MIX_SIZE {
        return None;
    }
    used.extend(&songs);
    deal(profile, &mut songs, rng);
    Some(profile.mix("forgotten", MixKind::Forgotten, &songs))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn iso(ts: i64) -> String {
        chrono::DateTime::from_timestamp(ts, 0).unwrap().to_rfc3339()
    }

    /// A library song; `plays`/`last` are Navidrome's view of it.
    fn song(id: &str, artist: &str, title: &str, genre: &str, plays: u64, last: Option<i64>) -> Child {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "title": title,
            "artist": artist,
            "artistId": format!("ar-{artist}"),
            "genre": genre,
            "duration": 200,
            "playCount": plays,
            "played": last.map(iso),
        }))
        .unwrap()
    }

    fn scrobble(ts: i64, artist: &str, track: &str) -> StatScrobble {
        StatScrobble { ts, artist: artist.into(), track: track.into(), album: None }
    }

    fn ids(mix: &Mix) -> Vec<&str> {
        mix.songs.iter().map(|s| s.id.as_str()).collect()
    }

    /// `n` songs by `artist`, the first `played` of them played `plays` times
    /// `days_ago` days ago.
    fn catalogue(
        artist: &str,
        genre: &str,
        n: usize,
        played: usize,
        plays: u64,
        days_ago: i64,
    ) -> Vec<Child> {
        (0..n)
            .map(|i| {
                let (plays, last) =
                    if i < played { (plays, Some(NOW - days_ago * DAY)) } else { (0, None) };
                song(&format!("{artist}-{i}"), artist, &format!("{artist} song {i}"), genre, plays, last)
            })
            .collect()
    }

    #[test]
    fn folding_ignores_case_and_punctuation() {
        assert_eq!(fold("AC/DC"), "acdc");
        assert_eq!(fold("ac dc"), "acdc");
        assert_eq!(fold("夜に駆ける"), "夜に駆ける");
        assert_eq!(fold("..."), "...");
    }

    #[test]
    fn loose_titles_drop_version_notes() {
        assert_eq!(loose_title("Song (Remastered 2011)"), "song");
        assert_eq!(loose_title("Song [Live] (Deluxe)"), "song");
        assert_eq!(loose_title("Song - 2011 Remaster"), "song");
        assert_eq!(loose_title("Song - Radio Edit"), "song");
        assert_eq!(loose_title("Song feat. Someone"), "song");
        assert_eq!(loose_title("Song (Original Mix)"), "song");
        // Part of the name, not a version note.
        assert_eq!(loose_title("Part 1 - The Beginning"), "part1thebeginning");
        assert_eq!(loose_title("(Untitled)"), "untitled");
    }

    #[test]
    fn loose_titles_keep_remixes_apart() {
        assert_eq!(loose_title("Song (Someone Remix)"), "songsomeoneremix");
        assert_eq!(loose_title("Song - Someone Remix"), "songsomeoneremix");
        assert_eq!(loose_title("Song feat. Guest - Someone Remix"), "songsomeoneremix");
        assert_eq!(loose_title("Song (Someone Remix) (Upsampled)"), "songsomeoneremix");
        assert_eq!(loose_title("Song VIP (feat. Guest)"), "songvip");
    }

    #[test]
    fn credits_split_into_artists() {
        assert_eq!(credit_keys("A, B & C"), vec!["abc", "a", "b", "c"]);
        assert_eq!(credit_keys("Artist feat. Guest"), vec!["artistfeatguest", "artist", "guest"]);
        assert_eq!(credit_keys("Solo"), vec!["solo"]);
        assert_eq!(credit_keys("Arc North • Donna Tella"), vec!["arcnorthdonnatella", "arcnorth", "donnatella"]);
    }

    #[test]
    fn scrobbles_match_library_songs_by_name() {
        let songs = vec![
            song("1", "Artist", "Song (Remastered 2011)", "Rock", 2, None),
            song("2", "Other", "Tune", "Rock", 0, None),
        ];
        let scrobbles = vec![
            scrobble(NOW - 3 * DAY, "Artist", "Song"),
            scrobble(NOW - 2 * DAY, "artist", "SONG"),
            scrobble(NOW - DAY, "Other feat. Guest", "Tune"),
            scrobble(NOW - DAY, "Nobody", "Unknown"),
        ];
        let profile = build_profile(songs, &scrobbles, NOW);
        assert_eq!(profile.matched_scrobbles, 3);
        let first = &profile.tracks[0];
        // Navidrome said 2, the scrobbles say 2: the same plays, not 4.
        assert_eq!(first.plays, 2);
        assert_eq!(first.recent_plays, 2);
        assert_eq!(first.last_played, Some(NOW - 2 * DAY));
        assert_eq!(profile.tracks[1].plays, 1);
        assert!(profile.affinity("artist") > 0.0);
        assert!(profile.affinity("other") > 0.0);
    }

    #[test]
    fn on_repeat_follows_recent_scrobbles() {
        let songs: Vec<Child> = (0..10)
            .map(|i| song(&i.to_string(), "A", &format!("Track {i}"), "Rock", 50, Some(NOW - 400 * DAY)))
            .collect();
        let mut scrobbles = Vec::new();
        for i in 0..10 {
            // Track 0 played most lately, then 1, ...
            for k in 0..(12 - i) {
                scrobbles.push(scrobble(NOW - (k + 1) * 3600, "A", &format!("Track {i}")));
            }
        }
        scrobbles.sort_by_key(|s| s.ts);
        let profile = build_profile(songs, &scrobbles, NOW);
        let mix = on_repeat(&profile, NOW).expect("enough recent plays");
        assert_eq!(&ids(&mix)[..3], ["0", "1", "2"]);
    }

    #[test]
    fn on_repeat_falls_back_to_navidrome() {
        let mut songs: Vec<Child> = (0..10)
            .map(|i| song(&i.to_string(), "A", &format!("T{i}"), "Rock", 10 - i, Some(NOW - DAY)))
            .collect();
        songs.push(song("old", "A", "Old", "Rock", 99, Some(NOW - 90 * DAY)));
        let profile = build_profile(songs, &[], NOW);
        let mix = on_repeat(&profile, NOW).expect("enough recent plays");
        assert_eq!(ids(&mix)[0], "0");
        assert!(!ids(&mix).contains(&"old"), "not played this month");
    }

    #[test]
    fn daily_mixes_cluster_by_genre_without_repeats() {
        let mut songs = Vec::new();
        for artist in ["R1", "R2", "R3"] {
            songs.extend(catalogue(artist, "Rock", 20, 10, 10, 5));
        }
        for artist in ["J1", "J2", "J3"] {
            songs.extend(catalogue(artist, "Jazz", 20, 10, 5, 5));
        }
        let profile = build_profile(songs, &[], NOW);
        let mixes = generate(&profile, &HashMap::new(), NOW, 7);
        let daily: Vec<&Mix> = mixes.iter().filter(|m| m.kind == MixKind::Daily).collect();
        assert_eq!(daily.len(), 2);
        assert_eq!(daily[0].genre.as_deref(), Some("Rock"));
        assert_eq!(daily[1].genre.as_deref(), Some("Jazz"));
        assert!(daily[0].songs.iter().all(|s| s.genre.as_deref() == Some("Rock")));

        let mut seen = HashSet::new();
        for mix in &daily {
            assert_eq!(mix.songs.len(), DAILY_MIX_SIZE);
            // Opens with a favourite.
            assert!(mix.songs[0].play_count.unwrap_or(0) > 0);
            for s in &mix.songs {
                assert!(seen.insert(&s.id), "{} is in two daily mixes", s.id);
            }
            for pair in mix.songs.windows(2) {
                assert_ne!(pair[0].artist, pair[1].artist, "artists should be spaced out");
            }
        }
    }

    #[test]
    fn deep_cuts_are_unplayed_songs_by_favourites() {
        let mut songs = catalogue("Fav", "Pop", 30, 5, 20, 3);
        songs.extend(catalogue("Never", "Pop", 30, 0, 0, 0));
        let profile = build_profile(songs, &[], NOW);
        let mix = deep_cuts(&profile, &mut Rng(1), &mut HashSet::new()).expect("enough songs");
        assert_eq!(mix.songs.len(), 25, "every unplayed song by the only favourite");
        assert!(mix.songs.iter().all(|s| s.artist.as_deref() == Some("Fav")));
        assert!(mix.songs.iter().all(|s| s.play_count == Some(0)));
    }

    #[test]
    fn deep_cuts_include_barely_heard_songs() {
        // Everything played; half of it only once, long ago.
        let mut songs = catalogue("Fav", "Pop", 10, 10, 30, 3);
        songs.extend(
            catalogue("Fav", "Pop", 20, 20, 1, 200).into_iter().skip(10).map(|mut s| {
                s.id = format!("once-{}", s.id);
                s
            }),
        );
        let profile = build_profile(songs, &[], NOW);
        let mix = deep_cuts(&profile, &mut Rng(2), &mut HashSet::new()).expect("enough songs");
        assert_eq!(mix.songs.len(), 10);
        assert!(mix.songs.iter().all(|s| s.play_count == Some(1)));
    }

    #[test]
    fn forgotten_favourites_are_loved_but_not_lately() {
        let mut songs = catalogue("Old", "Pop", 12, 12, 40, 400);
        songs.extend(catalogue("Now", "Pop", 12, 12, 40, 2));
        // A handful of rarely played songs, so 40 plays counts as heavy.
        songs.extend(catalogue("Light", "Pop", 40, 40, 1, 400));
        let profile = build_profile(songs, &[], NOW);
        let mix = forgotten(&profile, NOW, &mut Rng(3), &mut HashSet::new()).expect("enough songs");
        assert!(mix.songs.iter().all(|s| s.artist.as_deref() == Some("Old")));
    }

    #[test]
    fn discover_draws_from_unfamiliar_similar_artists() {
        let mut songs = catalogue("Fav", "Pop", 20, 20, 30, 2);
        songs.extend(catalogue("Similar", "Metal", 10, 0, 0, 0));
        songs.extend(catalogue("SameGenre", "Pop", 10, 0, 0, 0));
        songs.extend(catalogue("Unrelated", "Polka", 10, 0, 0, 0));
        // Played to death years ago: faded affinity, but not new.
        songs.extend(catalogue("OldLove", "Pop", 20, 10, 50, 1000));
        let profile = build_profile(songs, &[], NOW);
        let similar = HashMap::from([("fav".to_owned(), vec!["Similar".to_owned()])]);
        let mixes = generate(&profile, &similar, NOW, 11);
        let mix = mixes.iter().find(|m| m.kind == MixKind::Discover).expect("discover mix");
        let artists: HashSet<&str> = mix.songs.iter().filter_map(|s| s.artist.as_deref()).collect();
        assert!(artists.contains("Similar"));
        assert!(artists.contains("SameGenre"));
        assert!(!artists.contains("Unrelated"));
        assert!(!artists.contains("Fav"));
        assert!(!artists.contains("OldLove"));
    }

    #[test]
    fn one_star_and_short_songs_never_appear() {
        let mut songs = catalogue("A", "Pop", 20, 20, 10, 3);
        songs[0].user_rating = Some(1);
        songs[1].duration = Some(30);
        let profile = build_profile(songs, &[], NOW);
        for mix in generate(&profile, &HashMap::new(), NOW, 5) {
            assert!(!ids(&mix).contains(&"A-0"));
            assert!(!ids(&mix).contains(&"A-1"));
        }
    }

    #[test]
    fn duplicate_recordings_appear_once() {
        let mut songs = catalogue("A", "Pop", 20, 20, 10, 3);
        songs.push(song("dupe", "A", "A song 0 (Remastered)", "Pop", 10, Some(NOW - 3 * DAY)));
        let profile = build_profile(songs, &[], NOW);
        for mix in generate(&profile, &HashMap::new(), NOW, 9) {
            let both = ids(&mix).contains(&"A-0") && ids(&mix).contains(&"dupe");
            assert!(!both, "{} holds both copies", mix.id);
        }
    }

    #[test]
    fn empty_library_makes_no_mixes() {
        let profile = build_profile(Vec::new(), &[], NOW);
        assert!(generate(&profile, &HashMap::new(), NOW, 1).is_empty());
    }
}
