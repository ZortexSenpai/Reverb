//! Aggregation for the stats page: top lists, clock/weekday distributions,
//! and a play-count timeline, computed over the imported scrobbles.

use std::collections::{BTreeMap, HashMap};

use chrono::{
    DateTime, Datelike, Duration, Local, Months, NaiveDate, NaiveDateTime, TimeZone, Timelike,
};
use serde::Serialize;

use super::grouping::{Group, GroupKey, Grouper, Tally};
use super::store::StatScrobble;
use crate::library_index::index::{LibraryIndex, LibrarySong};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopEntry {
    pub name: String,
    /// Artist for album and Song entries.
    pub artist: Option<String>,
    /// The library item's id and cover-art id, for a Library match.
    pub id: Option<String>,
    pub cover_art: Option<String>,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelinePoint {
    pub label: String,
    pub count: u64,
}

/// One of the Period's top artists, played per bucket of the activity
/// timeline.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistTimeline {
    pub artist: TopEntry,
    /// The same buckets, with the same labels, as [`StatsSummary::timeline`].
    pub timeline: Vec<TimelinePoint>,
}

/// A Scrobble, as the Song it counts toward.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentPlay {
    pub ts: i64,
    pub name: String,
    pub artist: String,
    pub album: Option<String>,
    /// The library Song's id and cover-art id, for a Library match.
    pub id: Option<String>,
    pub cover_art: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsSummary {
    pub total_plays: u64,
    pub unique_artists: u64,
    pub unique_songs: u64,
    pub unique_albums: u64,
    pub first_ts: Option<i64>,
    pub last_ts: Option<i64>,
    pub plays_per_day: f64,
    pub top_artists: Vec<TopEntry>,
    pub top_songs: Vec<TopEntry>,
    pub top_albums: Vec<TopEntry>,
    /// Local-time hour of day, 24 buckets.
    pub by_hour: Vec<u64>,
    /// Monday-first weekday, 7 buckets.
    pub by_weekday: Vec<u64>,
    pub timeline: Vec<TimelinePoint>,
    /// The [`OVER_TIME_ARTISTS`] most played artists, most played first.
    pub top_artists_over_time: Vec<ArtistTimeline>,
    /// Newest first.
    pub recent: Vec<RecentPlay>,
    /// Local date with the most plays in the range.
    pub busiest_day: Option<TimelinePoint>,
    /// Longest run of consecutive days with at least one play.
    pub longest_streak_days: u32,
    /// One entry per day for the calendar grid, oldest first, `%Y-%m-%d`
    /// labels. Empty for windows too short to be worth a calendar.
    pub daily: Vec<TimelinePoint>,
    pub sessions: SessionStats,
    /// Absent for the all-time range, where every artist is "new".
    pub discovery: Option<Discovery>,
    /// Same calendar day in earlier years, most recent first. Independent of
    /// the selected range.
    pub on_this_day: Vec<OnThisDay>,
    /// Whether the Library index was there to match Scrobbles against. Until
    /// it is, everything is grouped by name and nothing has art.
    pub library_available: bool,
    /// The [`TOP_N`] genres of the Period's library Songs, most played
    /// first. Absent without a Library index.
    pub genres: Option<Breakdown<GenrePlays>>,
    /// Plays per release decade of the Period's library Songs, oldest first.
    /// Absent without a Library index.
    pub decades: Option<Breakdown<DecadePlays>>,
}

/// Listening broken into sittings — runs of plays with no long gap. Scrobbles
/// carry no track lengths, so a session's "span" is first play → last play.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    pub count: u64,
    /// Typical sitting, in tracks.
    pub median_tracks: u64,
    /// Longest sittings by track count, biggest first.
    pub longest: Vec<Session>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub start_ts: i64,
    pub tracks: u64,
    pub span_minutes: u64,
    /// Whoever dominated the sitting.
    pub top_artist: String,
}

/// First-ever plays that fall inside the window — what the period introduced.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Discovery {
    pub new_artists: u64,
    pub new_songs: u64,
    /// Artists first heard in the window, most-played first.
    pub first_artists: Vec<FirstHeard>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstHeard {
    pub name: String,
    /// The library Artist's id and cover-art id, for a Library match.
    pub id: Option<String>,
    pub cover_art: Option<String>,
    pub ts: i64,
    /// Plays since, within the window.
    pub plays: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnThisDay {
    pub year: i32,
    pub plays: u64,
    /// That day's most played Songs.
    pub songs: Vec<TopEntry>,
}

/// Plays split by what only the Library match knows about their Songs, like
/// genres or release years.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Breakdown<T> {
    pub entries: Vec<T>,
    /// The share of the Period's Scrobbles it's based on, from 0 to 1.
    pub coverage: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenrePlays {
    pub name: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecadePlays {
    /// Its first year: 1980 for the 1980s.
    pub decade: i32,
    pub count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatsRange {
    Day,
    Week,
    Month,
    Year,
    All,
    /// Explicit `from`/`to` window from the date pickers.
    Custom,
}

/// A range selection from the UI. `from`/`to` are unix seconds and only read
/// for [`StatsRange::Custom`]; `to` is exclusive (local midnight after the
/// picked end date).
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct StatsQuery {
    pub range: StatsRange,
    pub from: Option<i64>,
    pub to: Option<i64>,
}

impl StatsQuery {
    /// Half-open `[from, to)` window in unix seconds; `None` is unbounded.
    fn window(self, now: i64) -> (Option<i64>, Option<i64>) {
        const DAY: i64 = 86_400;
        match self.range {
            StatsRange::Day => (Some(now - DAY), None),
            StatsRange::Week => (Some(now - 7 * DAY), None),
            StatsRange::Month => (Some(now - 30 * DAY), None),
            StatsRange::Year => (Some(now - 365 * DAY), None),
            StatsRange::All => (None, None),
            // Tolerate reversed pickers rather than reporting an empty range.
            StatsRange::Custom => match (self.from, self.to) {
                (Some(a), Some(b)) if a > b => (Some(b), Some(a)),
                bounds => bounds,
            },
        }
    }
}

/// Timeline bucket width, picked from the window's span so every range gets a
/// readable number of points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bucket {
    /// `with_day` disambiguates labels when the window spans several days.
    Hour { with_day: bool },
    Day,
    Month,
    Year,
}

impl Bucket {
    fn for_span(secs: i64) -> Self {
        const DAY: i64 = 86_400;
        match secs {
            s if s <= 26 * 3_600 => Bucket::Hour { with_day: false },
            s if s <= 3 * DAY => Bucket::Hour { with_day: true },
            s if s <= 120 * DAY => Bucket::Day,
            s if s <= 4 * 365 * DAY => Bucket::Month,
            _ => Bucket::Year,
        }
    }

    /// Unique bucket identity (labels may repeat across days).
    fn key(self, t: NaiveDateTime) -> String {
        match self {
            Bucket::Hour { .. } => t.format("%Y-%m-%dT%H").to_string(),
            Bucket::Day => t.format("%Y-%m-%d").to_string(),
            Bucket::Month => t.format("%Y-%m").to_string(),
            Bucket::Year => t.format("%Y").to_string(),
        }
    }

    fn label(self, t: NaiveDateTime) -> String {
        match self {
            Bucket::Hour { with_day: false } => t.format("%H:00").to_string(),
            Bucket::Hour { with_day: true } => t.format("%m-%d %H:00").to_string(),
            Bucket::Day => t.format("%m-%d").to_string(),
            Bucket::Month => t.format("%Y-%m").to_string(),
            Bucket::Year => t.format("%Y").to_string(),
        }
    }

    /// Start of the bucket containing `t`.
    fn floor(self, t: NaiveDateTime) -> NaiveDateTime {
        let date = t.date();
        let (date, hour) = match self {
            Bucket::Hour { .. } => (date, t.hour()),
            Bucket::Day => (date, 0),
            Bucket::Month => (NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap_or(date), 0),
            Bucket::Year => (NaiveDate::from_ymd_opt(date.year(), 1, 1).unwrap_or(date), 0),
        };
        date.and_hms_opt(hour, 0, 0).unwrap_or(t)
    }

    fn next(self, t: NaiveDateTime) -> NaiveDateTime {
        let months = match self {
            Bucket::Hour { .. } => return t + Duration::hours(1),
            Bucket::Day => return t + Duration::days(1),
            Bucket::Month => 1,
            Bucket::Year => 12,
        };
        t.checked_add_months(Months::new(months)).unwrap_or(t + Duration::days(31 * months as i64))
    }
}

const TOP_N: usize = 10;
const OVER_TIME_ARTISTS: usize = 5;
const TOP_SESSIONS: usize = 5;
const ON_THIS_DAY_YEARS: usize = 3;
const ON_THIS_DAY_SONGS: usize = 3;

/// Guard against a pathological window producing an unbounded timeline.
const MAX_TIMELINE_POINTS: usize = 600;

/// Silence longer than this ends a listening session.
const SESSION_GAP_SECS: i64 = 30 * 60;

/// Below a month, the calendar grid has too few cells to say anything the
/// timeline doesn't; above a year it stops fitting on screen.
const MIN_CALENDAR_SPAN_SECS: i64 = 28 * 86_400;
const CALENDAR_MAX_DAYS: i64 = 371;

fn local_naive(ts: i64) -> Option<NaiveDateTime> {
    Local.timestamp_opt(ts, 0).single().map(|t| t.naive_local())
}

/// The [`TOP_N`] most played entries.
fn top(tally: Tally) -> Vec<TopEntry> {
    let mut entries = tally.ranked();
    entries.truncate(TOP_N);
    entries
}

/// The stats page's summary of `scrobbles` over the Period `query` picks, as
/// of `now`. Scrobbles with a Library match in `library` are grouped by it.
pub fn compute(
    scrobbles: &[StatScrobble],
    library: Option<&LibraryIndex>,
    query: StatsQuery,
    now: DateTime<Local>,
) -> StatsSummary {
    let (from, to) = query.window(now.timestamp());
    let filtered: Vec<&StatScrobble> = scrobbles
        .iter()
        .filter(|s| from.is_none_or(|f| s.ts >= f) && to.is_none_or(|t| s.ts < t))
        .collect();

    // Timeline span: the requested window, falling back to the data's own
    // extent when a bound is open ("all time", or any preset's open end).
    let start = from.or_else(|| filtered.first().map(|s| s.ts)).unwrap_or(now.timestamp());
    let end = to.unwrap_or(now.timestamp()).min(now.timestamp()).max(start + 1);
    let bucket = Bucket::for_span(end - start);

    let mut grouper = Grouper::new(library);
    let mut artists = Tally::default();
    let mut songs = Tally::default();
    let mut albums = Tally::default();
    let mut from_library = FromLibrary::default();
    let mut by_hour = vec![0u64; 24];
    let mut by_weekday = vec![0u64; 7];
    let mut buckets: HashMap<String, u64> = HashMap::new();
    let mut day_counts: HashMap<i32, u64> = HashMap::new();

    for s in &filtered {
        artists.add(grouper.artist(s));
        songs.add(grouper.song(s));
        if let Some(album) = grouper.album(s) {
            albums.add(album);
        }
        from_library.add(grouper.library_song(s));
        if let Some(local) = Local.timestamp_opt(s.ts, 0).single() {
            by_hour[local.hour() as usize] += 1;
            by_weekday[local.weekday().num_days_from_monday() as usize] += 1;
            *day_counts.entry(local.date_naive().num_days_from_ce()).or_default() += 1;
            *buckets.entry(bucket.key(local.naive_local())).or_default() += 1;
        }
    }

    // Busiest single day + longest consecutive-days streak.
    let busiest_day = day_counts.iter().max_by_key(|(_, count)| **count).and_then(
        |(day, count)| {
            chrono::NaiveDate::from_num_days_from_ce_opt(*day).map(|date| TimelinePoint {
                label: date.format("%Y-%m-%d").to_string(),
                count: *count,
            })
        },
    );
    let mut days: Vec<i32> = day_counts.keys().copied().collect();
    days.sort_unstable();
    let mut longest_streak_days = 0u32;
    let mut run = 0u32;
    let mut prev: Option<i32> = None;
    for day in days {
        run = if prev.is_some_and(|p| day == p + 1) { run + 1 } else { 1 };
        longest_streak_days = longest_streak_days.max(run);
        prev = Some(day);
    }

    let recent: Vec<RecentPlay> = filtered
        .iter()
        .rev()
        .take(15)
        .map(|s| {
            let song = grouper.song(s);
            RecentPlay {
                ts: s.ts,
                name: song.name.to_owned(),
                artist: song.artist.unwrap_or(&s.artist).to_owned(),
                album: grouper.album(s).map(|album| album.name.to_owned()),
                id: song.id.map(str::to_owned),
                cover_art: song.cover_art.map(str::to_owned),
            }
        })
        .collect();

    // Dense timeline (include empty buckets so gaps are visible). `end` is
    // exclusive, so the last point is the bucket holding its final second.
    let mut slots = Vec::new();
    if let (Some(first), Some(last)) = (local_naive(start), local_naive(end - 1)) {
        let mut cursor = bucket.floor(first);
        while cursor <= last && slots.len() < MAX_TIMELINE_POINTS {
            slots.push(cursor);
            cursor = bucket.next(cursor);
        }
    }
    let points = |counts: &HashMap<String, u64>| -> Vec<TimelinePoint> {
        let point = |at| TimelinePoint {
            count: counts.get(&bucket.key(at)).copied().unwrap_or(0),
            label: bucket.label(at),
        };
        slots.iter().copied().map(point).collect()
    };
    let timeline = points(&buckets);

    // The leading artists are only known once everything is counted, so
    // their plays are bucketed in a second pass.
    let unique_artists = artists.len() as u64;
    let top_artists = top(artists);
    let leaders: Vec<&TopEntry> = top_artists.iter().take(OVER_TIME_ARTISTS).collect();
    let mut leader_buckets = vec![HashMap::<String, u64>::new(); leaders.len()];
    for s in &filtered {
        let key = grouper.artist(s).key();
        let Some(at) = leaders.iter().position(|leader| leader.key() == key) else { continue };
        if let Some(local) = local_naive(s.ts) {
            *leader_buckets[at].entry(bucket.key(local)).or_default() += 1;
        }
    }
    let top_artists_over_time = leaders
        .iter()
        .zip(&leader_buckets)
        .map(|(&artist, counts)| ArtistTimeline {
            artist: artist.clone(),
            timeline: points(counts),
        })
        .collect();

    // One cell per day for the calendar, capped so a long window stays a
    // readable grid rather than an endless strip.
    let mut daily = Vec::new();
    if end - start >= MIN_CALENDAR_SPAN_SECS {
        let calendar_start = start.max(end - CALENDAR_MAX_DAYS * 86_400);
        if let (Some(first), Some(last)) = (local_naive(calendar_start), local_naive(end - 1)) {
            let (mut date, last) = (first.date(), last.date());
            while date <= last {
                daily.push(TimelinePoint {
                    count: day_counts.get(&date.num_days_from_ce()).copied().unwrap_or(0),
                    label: date.format("%Y-%m-%d").to_string(),
                });
                let Some(next) = date.succ_opt() else { break };
                date = next;
            }
        }
    }

    let sessions = sessionize(&filtered);
    let discovery = from.map(|window_start| discover(scrobbles, &mut grouper, window_start, to));
    let on_this_day = on_this_day(scrobbles, &mut grouper, now.date_naive());

    let first_ts = filtered.first().map(|s| s.ts);
    let last_ts = filtered.last().map(|s| s.ts);
    let span_days = match (first_ts, last_ts) {
        (Some(a), Some(b)) if b > a => ((b - a) as f64 / 86_400.0).max(1.0),
        _ => 1.0,
    };

    StatsSummary {
        total_plays: filtered.len() as u64,
        unique_artists,
        unique_songs: songs.len() as u64,
        unique_albums: albums.len() as u64,
        first_ts,
        last_ts,
        plays_per_day: filtered.len() as f64 / span_days,
        top_artists,
        top_songs: top(songs),
        top_albums: top(albums),
        by_hour,
        by_weekday,
        timeline,
        top_artists_over_time,
        recent,
        busiest_day,
        longest_streak_days,
        daily,
        sessions,
        discovery,
        on_this_day,
        library_available: library.is_some(),
        genres: library.map(|_| from_library.genres()),
        decades: library.map(|_| from_library.decades()),
    }
}

/// Split plays into sittings: a gap longer than [`SESSION_GAP_SECS`] starts a
/// new one. Only the winning artist is kept per session, so the tally does not
/// grow with history.
fn sessionize(plays: &[&StatScrobble]) -> SessionStats {
    /// One run of plays, still being tallied.
    struct Open<'a> {
        start: i64,
        last: i64,
        tracks: u64,
        artists: HashMap<&'a str, u64>,
    }

    impl Open<'_> {
        fn finish(self) -> Session {
            let top_artist = self
                .artists
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                .map(|(name, _)| (*name).to_owned())
                .unwrap_or_default();
            Session {
                start_ts: self.start,
                tracks: self.tracks,
                span_minutes: ((self.last - self.start) / 60) as u64,
                top_artist,
            }
        }
    }

    let mut sessions: Vec<Session> = Vec::new();
    let mut open: Option<Open> = None;

    for play in plays {
        match open.as_mut() {
            Some(run) if play.ts - run.last <= SESSION_GAP_SECS => {
                run.last = play.ts;
                run.tracks += 1;
                *run.artists.entry(play.artist.as_str()).or_default() += 1;
            }
            _ => {
                if let Some(run) = open.take() {
                    sessions.push(run.finish());
                }
                open = Some(Open {
                    start: play.ts,
                    last: play.ts,
                    tracks: 1,
                    artists: HashMap::from([(play.artist.as_str(), 1)]),
                });
            }
        }
    }
    if let Some(run) = open {
        sessions.push(run.finish());
    }

    let mut sizes: Vec<u64> = sessions.iter().map(|s| s.tracks).collect();
    sizes.sort_unstable();
    let median_tracks = sizes.get(sizes.len() / 2).copied().unwrap_or(0);
    let count = sessions.len() as u64;

    sessions
        .sort_by(|a, b| b.tracks.cmp(&a.tracks).then_with(|| b.span_minutes.cmp(&a.span_minutes)));
    sessions.truncate(TOP_SESSIONS);
    SessionStats { count, median_tracks, longest: sessions }
}

/// Artists and Songs whose *first ever* play falls inside the window, grouped
/// like the top lists: "A feat. B" is nothing new once A is known.
fn discover<'a>(
    all: &'a [StatScrobble],
    grouper: &mut Grouper<'a>,
    from: i64,
    to: Option<i64>,
) -> Discovery {
    let in_window = |ts: i64| ts >= from && to.is_none_or(|t| ts < t);
    // Each artist's entry, first play and plays in the window.
    let mut first_artist: HashMap<GroupKey, (Group, i64, u64)> = HashMap::new();
    let mut first_song: HashMap<GroupKey, i64> = HashMap::new();
    // Scrobbles are chronological, so the first sighting wins.
    for s in all {
        let artist = grouper.artist(s);
        let (_, _, plays) = first_artist.entry(artist.key()).or_insert((artist, s.ts, 0));
        if in_window(s.ts) {
            *plays += 1;
        }
        first_song.entry(grouper.song(s).key()).or_insert(s.ts);
    }

    let mut first_artists: Vec<FirstHeard> = first_artist
        .into_values()
        .filter(|(_, ts, _)| in_window(*ts))
        .map(|(artist, ts, plays)| FirstHeard {
            name: artist.name.to_owned(),
            id: artist.id.map(str::to_owned),
            cover_art: artist.cover_art.map(str::to_owned),
            ts,
            plays,
        })
        .collect();
    // Discoveries that stuck first: a one-off play says less about the period
    // than an artist picked up and kept.
    first_artists.sort_by(|a, b| b.plays.cmp(&a.plays).then_with(|| b.ts.cmp(&a.ts)));
    let new_artists = first_artists.len() as u64;
    first_artists.truncate(TOP_N);

    Discovery {
        new_artists,
        new_songs: first_song.values().filter(|ts| in_window(**ts)).count() as u64,
        first_artists,
    }
}

/// One earlier year's plays on today's date, still being tallied.
#[derive(Default)]
struct DayInYear<'a> {
    plays: u64,
    songs: Tally<'a>,
}

/// Plays from the same month/day in earlier years, most recent year first.
fn on_this_day<'a>(
    all: &'a [StatScrobble],
    grouper: &mut Grouper<'a>,
    today: NaiveDate,
) -> Vec<OnThisDay> {
    let mut years: HashMap<i32, DayInYear> = HashMap::new();
    for s in all {
        let Some(local) = Local.timestamp_opt(s.ts, 0).single() else { continue };
        let date = local.date_naive();
        if date.month() != today.month() || date.day() != today.day() || date.year() >= today.year()
        {
            continue;
        }
        let entry = years.entry(date.year()).or_default();
        entry.plays += 1;
        entry.songs.add(grouper.song(s));
    }

    let mut out: Vec<OnThisDay> = years
        .into_iter()
        .map(|(year, day)| {
            let mut songs = day.songs.ranked();
            songs.truncate(ON_THIS_DAY_SONGS);
            OnThisDay { year, plays: day.plays, songs }
        })
        .collect();
    out.sort_by_key(|day| std::cmp::Reverse(day.year));
    out.truncate(ON_THIS_DAY_YEARS);
    out
}

/// Plays per genre and per release decade of the Scrobbles' library Songs,
/// still being tallied.
#[derive(Default)]
struct FromLibrary<'a> {
    plays: u64,
    genres: HashMap<&'a str, u64>,
    /// Plays of a Song with at least one genre.
    with_genre: u64,
    decades: BTreeMap<i32, u64>,
}

impl<'a> FromLibrary<'a> {
    /// Count a Scrobble with Library match `song`, if any.
    fn add(&mut self, song: Option<LibrarySong<'a>>) {
        self.plays += 1;
        let Some(song) = song else { return };
        let genres = song.genres();
        if !genres.is_empty() {
            self.with_genre += 1;
        }
        for genre in genres {
            *self.genres.entry(genre).or_default() += 1;
        }
        if let Some(year) = song.year() {
            *self.decades.entry(year - year.rem_euclid(10)).or_default() += 1;
        }
    }

    /// `plays` as a share of every play counted.
    fn share(&self, plays: u64) -> f64 {
        if self.plays == 0 {
            return 0.0;
        }
        plays as f64 / self.plays as f64
    }

    fn genres(&self) -> Breakdown<GenrePlays> {
        let mut entries: Vec<GenrePlays> = self
            .genres
            .iter()
            .map(|(&name, &count)| GenrePlays { name: name.to_owned(), count })
            .collect();
        entries.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
        entries.truncate(TOP_N);
        Breakdown { entries, coverage: self.share(self.with_genre) }
    }

    fn decades(&self) -> Breakdown<DecadePlays> {
        let entries =
            self.decades.iter().map(|(&decade, &count)| DecadePlays { decade, count }).collect();
        // A Song has one year, so every play of a dated Song is in one decade.
        Breakdown { entries, coverage: self.share(self.decades.values().sum()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library_index::testing as library;

    /// A preset range needs no explicit bounds.
    fn preset(range: StatsRange) -> StatsQuery {
        StatsQuery { range, from: None, to: None }
    }

    /// The summary as of the wall clock, with no Library index.
    fn compute_now(scrobbles: &[StatScrobble], query: StatsQuery) -> StatsSummary {
        compute(scrobbles, None, query, Local::now())
    }

    fn scrobble(ts: i64, artist: &str, track: &str) -> StatScrobble {
        StatScrobble {
            ts,
            artist: artist.into(),
            track: track.into(),
            album: Some(format!("{artist} album")),
        }
    }

    #[test]
    fn computes_tops_and_counts() {
        let now = Local::now().timestamp();
        let scrobbles = vec![
            scrobble(now - 100, "A", "one"),
            scrobble(now - 200, "A", "one"),
            scrobble(now - 300, "A", "two"),
            scrobble(now - 400, "B", "three"),
        ];
        let s = compute_now(&scrobbles, preset(StatsRange::All));
        assert_eq!(s.total_plays, 4);
        assert_eq!(s.unique_artists, 2);
        assert_eq!(s.unique_songs, 3);
        assert_eq!(s.top_artists[0].name, "A");
        assert_eq!(s.top_artists[0].count, 3);
        assert_eq!(s.top_songs[0].name, "one");
        assert_eq!(s.top_songs[0].count, 2);
        assert_eq!(s.by_hour.iter().sum::<u64>(), 4);
        assert_eq!(s.by_weekday.iter().sum::<u64>(), 4);
    }

    #[test]
    fn range_filter_excludes_old_plays() {
        let now = Local::now().timestamp();
        let scrobbles = vec![
            scrobble(now - 100, "A", "recent"),
            scrobble(now - 400 * 86_400, "B", "ancient"),
        ];
        let year = compute_now(&scrobbles, preset(StatsRange::Year));
        assert_eq!(year.total_plays, 1);
        let all = compute_now(&scrobbles, preset(StatsRange::All));
        assert_eq!(all.total_plays, 2);
    }

    #[test]
    fn short_presets_bucket_by_hour_and_day() {
        let now = Local::now().timestamp();
        let scrobbles = vec![
            scrobble(now - 600, "A", "now"),
            scrobble(now - 3 * 86_400, "B", "three days ago"),
            scrobble(now - 20 * 86_400, "C", "three weeks ago"),
        ];

        let day = compute_now(&scrobbles, preset(StatsRange::Day));
        assert_eq!(day.total_plays, 1);
        // 24h window, hourly buckets: current hour plus the 24 before it.
        assert_eq!(day.timeline.len(), 25);
        assert_eq!(day.timeline.iter().map(|p| p.count).sum::<u64>(), 1);
        // A 10-minute-old play lands in this hour's bucket, or the one before
        // it when the hour has just turned over.
        let newest: u64 = day.timeline.iter().rev().take(2).map(|p| p.count).sum();
        assert_eq!(newest, 1);

        let week = compute_now(&scrobbles, preset(StatsRange::Week));
        assert_eq!(week.total_plays, 2);
        assert_eq!(week.timeline.len(), 8);
        assert_eq!(week.timeline.iter().map(|p| p.count).sum::<u64>(), 2);
    }

    #[test]
    fn sessions_split_on_long_gaps() {
        let now = Local::now().timestamp();
        // Three back-to-back plays, a two-hour gap, then two more.
        let scrobbles = vec![
            scrobble(now - 7_500, "A", "one"),
            scrobble(now - 7_300, "A", "two"),
            scrobble(now - 7_100, "B", "three"),
            scrobble(now - 400, "C", "four"),
            scrobble(now - 200, "C", "five"),
        ];
        let s = compute_now(&scrobbles, preset(StatsRange::Day)).sessions;
        assert_eq!(s.count, 2);
        assert_eq!(s.median_tracks, 3);
        // Longest first: 3 tracks over the ~6 minutes between first and last.
        assert_eq!(s.longest[0].tracks, 3);
        assert_eq!(s.longest[0].span_minutes, 6);
        assert_eq!(s.longest[0].top_artist, "A");
        assert_eq!(s.longest[1].tracks, 2);
        assert_eq!(s.longest[1].top_artist, "C");
    }

    #[test]
    fn plays_inside_the_gap_stay_one_session() {
        let now = Local::now().timestamp();
        // Exactly the gap limit apart — still the same sitting.
        let scrobbles = vec![
            scrobble(now - 3_600, "A", "one"),
            scrobble(now - 1_800, "A", "two"),
        ];
        assert_eq!(compute_now(&scrobbles, preset(StatsRange::Day)).sessions.count, 1);
    }

    #[test]
    fn discovery_counts_only_first_ever_plays() {
        let now = Local::now().timestamp();
        let day = 86_400;
        let scrobbles = vec![
            // Known long before the week window opened.
            scrobble(now - 300 * day, "Old", "song"),
            scrobble(now - 2 * day, "Old", "song"),
            // Brand new inside it.
            scrobble(now - day, "New", "first"),
            scrobble(now - 3_600, "New", "second"),
        ];
        let week = compute_now(&scrobbles, preset(StatsRange::Week)).discovery.unwrap();
        assert_eq!(week.new_artists, 1);
        assert_eq!(week.new_songs, 2);
        assert_eq!(week.first_artists.len(), 1);
        assert_eq!(week.first_artists[0].name, "New");
        // Both of the new artist's plays fall in the window.
        assert_eq!(week.first_artists[0].plays, 2);

        // A discovery that stuck outranks a more recent one-off.
        let mut with_oneoff = scrobbles.clone();
        with_oneoff.push(scrobble(now - 60, "Passing", "once"));
        let week = compute_now(&with_oneoff, preset(StatsRange::Week));
        let listed = week.discovery.unwrap().first_artists;
        let names: Vec<&str> = listed.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["New", "Passing"]);

        // Everything is "new" over all time, so the section is left out.
        assert!(compute_now(&scrobbles, preset(StatsRange::All)).discovery.is_none());
    }

    #[test]
    fn calendar_covers_long_windows_only() {
        let now = Local::now().timestamp();
        let scrobbles = vec![scrobble(now - 100, "A", "one")];
        assert!(compute_now(&scrobbles, preset(StatsRange::Week)).daily.is_empty());

        let month = compute_now(&scrobbles, preset(StatsRange::Month)).daily;
        assert_eq!(month.len(), 31, "30-day window spans 31 local dates at most");
        assert_eq!(month.iter().map(|d| d.count).sum::<u64>(), 1);

        // A decade is capped to the calendar's own window, not the range.
        let year = 365 * 86_400;
        let wide = StatsQuery {
            range: StatsRange::Custom,
            from: Some(now - 10 * year),
            to: Some(now),
        };
        assert!(compute_now(&scrobbles, wide).daily.len() <= CALENDAR_MAX_DAYS as usize + 1);
    }

    #[test]
    fn on_this_day_looks_back_by_year() {
        let now = fixed_now();
        let scrobbles = vec![
            // Same month/day, one and two years back (52-week jumps drift, so
            // build the timestamps from real dates).
            scrobble(same_day_years_ago(now, 2), "B", "two years ago"),
            scrobble(same_day_years_ago(now, 2), "B", "two years ago"),
            scrobble(same_day_years_ago(now, 1), "A", "last year"),
            // Today never counts as history.
            scrobble(now.timestamp() - 60, "C", "today"),
        ];
        let days = compute(&scrobbles, None, preset(StatsRange::Day), now).on_this_day;
        assert_eq!(days.len(), 2);
        assert_eq!(days[0].year, now.year() - 1);
        assert_eq!(days[0].plays, 1);
        assert_eq!(days[1].plays, 2);
        assert_eq!(days[1].songs[0].name, "two years ago");
        assert_eq!(days[1].songs[0].count, 2);
    }

    /// Noon on the same month/day `years` back — None when that date does not
    /// exist (29 February).
    fn same_day_years_ago(now: chrono::DateTime<Local>, years: i32) -> i64 {
        let date = now.date_naive().with_year(now.year() - years).expect("date exists");
        Local
            .from_local_datetime(&date.and_hms_opt(12, 0, 0).unwrap())
            .single()
            .expect("noon is unambiguous")
            .timestamp()
    }

    #[test]
    fn custom_window_is_inclusive_start_exclusive_end() {
        let day = 86_400;
        // 2024-03-01 12:00 UTC and the two neighbouring days at noon, so the
        // local-time bucket is the same day in every plausible zone.
        let base = 1_709_294_400;
        let scrobbles = vec![
            scrobble(base - day, "A", "before"),
            scrobble(base, "B", "inside"),
            scrobble(base + day, "C", "after"),
        ];
        let midnight = base - 12 * 3_600;
        let query =
            StatsQuery { range: StatsRange::Custom, from: Some(midnight), to: Some(midnight + day) };
        let s = compute_now(&scrobbles, query);
        assert_eq!(s.total_plays, 1);
        assert_eq!(s.top_songs[0].name, "inside");
        // A one-day window keeps hourly resolution without a trailing empty day.
        assert_eq!(s.timeline.len(), 24);
        assert_eq!(s.timeline.iter().map(|p| p.count).sum::<u64>(), 1);
    }

    #[test]
    fn custom_window_tolerates_reversed_bounds() {
        let now = Local::now().timestamp();
        let scrobbles = vec![scrobble(now - 2 * 86_400, "A", "one")];
        let reversed =
            StatsQuery { range: StatsRange::Custom, from: Some(now), to: Some(now - 5 * 86_400) };
        assert_eq!(compute_now(&scrobbles, reversed).total_plays, 1);
    }

    #[test]
    fn wide_custom_window_falls_back_to_coarse_buckets() {
        let year = 365 * 86_400;
        let now = Local::now().timestamp();
        let scrobbles = vec![scrobble(now - year, "A", "one"), scrobble(now - 100, "B", "two")];
        let query =
            StatsQuery { range: StatsRange::Custom, from: Some(now - 10 * year), to: Some(now) };
        let s = compute_now(&scrobbles, query);
        assert_eq!(s.total_plays, 2);
        // Yearly buckets over a decade — 11 calendar years at most.
        assert!(s.timeline.len() <= 11, "got {} points", s.timeline.len());
        assert_eq!(s.timeline.iter().map(|p| p.count).sum::<u64>(), 2);
    }

    #[test]
    fn top_five_artists_are_counted_on_the_activity_buckets() {
        let library = LibraryIndex::new(Vec::new(), vec![library::artist("a", "Artist")]);
        let (now, day) = (fixed_now().timestamp(), 86_400);
        // (days before 2024-03-01, artist), at noon UTC so every zone sees
        // the same local date.
        let plays = [
            (2, "Artist"),
            (2, "Artist feat. Guest"),
            (0, "Artist"),
            (1, "B"),
            (1, "B"),
            (2, "C"),
            (0, "C"),
            (1, "D"),
            (0, "E"),
            (2, "F"),
        ];
        let scrobbles: Vec<StatScrobble> =
            plays.iter().map(|&(ago, artist)| scrobble(now - ago * day, artist, "Song")).collect();
        let s = compute(&scrobbles, Some(&library), preset(StatsRange::Week), fixed_now());

        let over_time = &s.top_artists_over_time;
        let artists: Vec<&str> = over_time.iter().map(|a| a.artist.name.as_str()).collect();
        // F ties D and E on plays but ranks sixth by name.
        assert_eq!(artists, ["Artist", "B", "C", "D", "E"]);
        assert_eq!(over_time[0].artist.id.as_deref(), Some("a"));
        assert_eq!(over_time[0].artist.cover_art.as_deref(), Some("ar-a"));

        let labels = |points: &[TimelinePoint]| -> Vec<String> {
            points.iter().map(|p| p.label.clone()).collect()
        };
        let played = |i: usize| -> Vec<(&str, u64)> {
            let points = over_time[i].timeline.iter().filter(|p| p.count > 0);
            points.map(|p| (p.label.as_str(), p.count)).collect()
        };
        for artist in over_time {
            assert_eq!(labels(&artist.timeline), labels(&s.timeline));
        }
        assert_eq!(played(0), [("02-28", 2), ("03-01", 1)]);
        assert_eq!(played(1), [("02-29", 2)]);
        assert_eq!(played(2), [("02-28", 1), ("03-01", 1)]);
        assert_eq!(played(3), [("02-29", 1)]);
        assert_eq!(played(4), [("03-01", 1)]);
    }

    /// 2024-03-01 12:00 UTC: noon, so the local date is the same in every
    /// plausible zone.
    fn fixed_now() -> DateTime<Local> {
        Local.timestamp_opt(1_709_294_400, 0).unwrap()
    }

    /// `scrobble` on `album`.
    fn on_album(mut scrobble: StatScrobble, album: &str) -> StatScrobble {
        scrobble.album = Some(album.into());
        scrobble
    }

    /// Scrobbles of versions and credits a Library match merges, each a
    /// minute apart, ending just before [`fixed_now`].
    fn merge_candidates() -> Vec<StatScrobble> {
        let now = fixed_now().timestamp();
        let mut scrobbles = vec![
            on_album(scrobble(0, "Artist", "Song"), "Album"),
            on_album(scrobble(0, "Artist", "Song - 2011 Remaster"), "Album"),
            on_album(scrobble(0, "Artist feat. Guest", "Duet"), "Album"),
            on_album(scrobble(0, "X", "First"), "Hits"),
            on_album(scrobble(0, "Y", "Second"), "Hits"),
        ];
        let count = scrobbles.len() as i64;
        for (i, s) in scrobbles.iter_mut().enumerate() {
            s.ts = now - 60 * (count - i as i64);
        }
        scrobbles
    }

    fn names(entries: &[TopEntry]) -> Vec<(&str, Option<&str>, u64)> {
        entries.iter().map(|e| (e.name.as_str(), e.artist.as_deref(), e.count)).collect()
    }

    #[test]
    fn without_an_index_everything_groups_by_name() {
        let s = compute(&merge_candidates(), None, preset(StatsRange::All), fixed_now());

        assert!(!s.library_available);
        assert_eq!(
            names(&s.top_artists),
            [("Artist", None, 2), ("Artist feat. Guest", None, 1), ("X", None, 1), ("Y", None, 1)]
        );
        assert_eq!(
            names(&s.top_songs),
            [
                ("Duet", Some("Artist feat. Guest"), 1),
                ("First", Some("X"), 1),
                ("Second", Some("Y"), 1),
                ("Song", Some("Artist"), 1),
                ("Song - 2011 Remaster", Some("Artist"), 1),
            ]
        );
        assert_eq!(
            names(&s.top_albums),
            [
                ("Album", Some("Artist"), 2),
                ("Album", Some("Artist feat. Guest"), 1),
                ("Hits", Some("X"), 1),
                ("Hits", Some("Y"), 1),
            ]
        );
        let all = s.top_artists.iter().chain(&s.top_songs).chain(&s.top_albums);
        assert!(all.into_iter().all(|e| e.id.is_none() && e.cover_art.is_none()));
    }

    /// The library [`merge_candidates`] match: one album "Album" by Artist
    /// (in the artist listing, with art) and the compilation "Hits".
    fn merge_library() -> LibraryIndex {
        let songs = vec![
            library::on_album(library::song("s1", "Artist", "Song - 2011 Remaster"), "al1", "Album"),
            library::on_album(library::song("s2", "Artist", "Duet"), "al1", "Album"),
            library::on_album(library::song("s3", "X", "First"), "comp", "Hits"),
            library::on_album(library::song("s4", "Y", "Second"), "comp", "Hits"),
        ];
        LibraryIndex::new(songs, vec![library::artist("a", "Artist")])
    }

    fn entry<'a>(entries: &'a [TopEntry], name: &str) -> &'a TopEntry {
        entries.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("no entry {name}"))
    }

    #[test]
    fn a_feat_credit_counts_for_its_library_artist() {
        let library = merge_library();
        let s = compute(&merge_candidates(), Some(&library), preset(StatsRange::All), fixed_now());

        assert!(s.library_available);
        let artist = entry(&s.top_artists, "Artist");
        assert_eq!(artist.count, 3);
        assert_eq!(artist.id.as_deref(), Some("a"));
        assert_eq!(artist.cover_art.as_deref(), Some("ar-a"));
        assert!(s.top_artists.iter().all(|e| e.name != "Artist feat. Guest"));
    }

    #[test]
    fn a_remaster_counts_as_the_song_it_matches() {
        let library = merge_library();
        let s = compute(&merge_candidates(), Some(&library), preset(StatsRange::All), fixed_now());

        let song = entry(&s.top_songs, "Song - 2011 Remaster");
        assert_eq!((song.artist.as_deref(), song.count), (Some("Artist"), 2));
        assert_eq!(song.id.as_deref(), Some("s1"));
        assert_eq!(song.cover_art.as_deref(), Some("al-al1"));
        assert!(s.top_songs.iter().all(|e| e.name != "Song"));
        assert_eq!(s.unique_songs, 4);
    }

    #[test]
    fn a_compilation_counts_as_one_album() {
        let library = merge_library();
        let s = compute(&merge_candidates(), Some(&library), preset(StatsRange::All), fixed_now());

        let hits = entry(&s.top_albums, "Hits");
        assert_eq!(hits.count, 2);
        assert_eq!(hits.id.as_deref(), Some("comp"));
        assert_eq!(hits.cover_art.as_deref(), Some("al-comp"));
        // Its Songs are by different artists, so it names none of them.
        assert_eq!(hits.artist, None);
        // "Artist feat. Guest" is on Artist's album too.
        let album = entry(&s.top_albums, "Album");
        assert_eq!((album.artist.as_deref(), album.count), (Some("Artist"), 3));
        assert_eq!(s.unique_albums, 2);
    }

    #[test]
    fn a_duo_the_library_lists_stays_whole() {
        let artists = vec![library::artist("sg", "Simon & Garfunkel"), library::artist("s", "Simon")];
        let library = LibraryIndex::new(Vec::new(), artists);
        let now = fixed_now().timestamp();
        let scrobbles = vec![
            scrobble(now - 180, "Simon & Garfunkel", "The Boxer"),
            scrobble(now - 120, "Simon & Garfunkel", "Cecilia"),
            scrobble(now - 60, "Simon", "Solo"),
        ];
        let s = compute(&scrobbles, Some(&library), preset(StatsRange::All), fixed_now());

        let artists: Vec<(&str, Option<&str>, u64)> =
            s.top_artists.iter().map(|e| (e.name.as_str(), e.id.as_deref(), e.count)).collect();
        assert_eq!(artists, [("Simon & Garfunkel", Some("sg"), 2), ("Simon", Some("s"), 1)]);
    }

    #[test]
    fn unmatched_scrobbles_group_by_name_without_ids() {
        let library = merge_library();
        let now = fixed_now().timestamp();
        let scrobbles = vec![
            scrobble(now - 180, "Stranger", "Unknown"),
            scrobble(now - 120, "Stranger", "Unknown"),
            scrobble(now - 60, "Artist", "Unreleased"),
        ];
        let s = compute(&scrobbles, Some(&library), preset(StatsRange::All), fixed_now());

        assert_eq!(
            names(&s.top_songs),
            [("Unknown", Some("Stranger"), 2), ("Unreleased", Some("Artist"), 1)]
        );
        assert_eq!(
            names(&s.top_albums),
            [("Stranger album", Some("Stranger"), 2), ("Artist album", Some("Artist"), 1)]
        );
        let stranger = entry(&s.top_artists, "Stranger");
        assert_eq!((stranger.id.as_deref(), stranger.cover_art.as_deref()), (None, None));
        let unmatched = s.top_songs.iter().chain(&s.top_albums);
        assert!(unmatched.into_iter().all(|e| e.id.is_none() && e.cover_art.is_none()));
        // The artist is in the library even though this Song isn't.
        assert_eq!(entry(&s.top_artists, "Artist").id.as_deref(), Some("a"));
    }

    /// A library Song by "Artist" with a genre list, as OpenSubsonic sends it.
    fn tagged(id: &str, title: &str, genres: &[&str]) -> reverb_core::subsonic::models::Child {
        let mut song = library::song(id, "Artist", title);
        song.genres = genres
            .iter()
            .map(|&name| reverb_core::subsonic::models::GenreRef { name: name.into() })
            .collect();
        song
    }

    /// Scrobbles by "Artist" of `titles`, a minute apart, ending just before
    /// [`fixed_now`].
    fn plays_of(titles: &[&str]) -> Vec<StatScrobble> {
        let now = fixed_now().timestamp();
        let count = titles.len() as i64;
        (0..count).map(|i| scrobble(now - 60 * (count - i), "Artist", titles[i as usize])).collect()
    }

    fn genre_counts(s: &StatsSummary) -> Vec<(&str, u64)> {
        let genres = s.genres.as_ref().expect("genres with an index");
        genres.entries.iter().map(|g| (g.name.as_str(), g.count)).collect()
    }

    #[test]
    fn a_song_with_several_genres_counts_toward_each() {
        let library = LibraryIndex::new(
            vec![tagged("s1", "Anthem", &["Rock", "Pop"]), tagged("s2", "Riff", &["Rock"])],
            Vec::new(),
        );
        let scrobbles = plays_of(&["Anthem", "Riff", "Anthem"]);
        let s = compute(&scrobbles, Some(&library), preset(StatsRange::All), fixed_now());

        assert_eq!(genre_counts(&s), [("Rock", 3), ("Pop", 2)]);
    }

    #[test]
    fn a_song_without_a_genre_list_counts_toward_its_single_genre() {
        let mut ballad = library::song("s1", "Artist", "Ballad");
        ballad.genre = Some("Jazz".into());
        let songs = vec![ballad, tagged("s2", "Anthem", &["Rock"])];
        let library = LibraryIndex::new(songs, Vec::new());
        let scrobbles = plays_of(&["Ballad", "Ballad", "Anthem"]);
        let s = compute(&scrobbles, Some(&library), preset(StatsRange::All), fixed_now());

        assert_eq!(genre_counts(&s), [("Jazz", 2), ("Rock", 1)]);
    }

    #[test]
    fn genres_keep_the_ten_most_played() {
        // Twelve Songs, one genre each: G01 played 12 times down to G12 once.
        let genres: Vec<String> = (1..=12).map(|n| format!("G{n:02}")).collect();
        let songs = genres.iter().map(|g| tagged(g, g, &[g.as_str()])).collect();
        let library = LibraryIndex::new(songs, Vec::new());
        let titles: Vec<&str> =
            genres.iter().enumerate().flat_map(|(i, g)| vec![g.as_str(); 12 - i]).collect();
        let s = compute(&plays_of(&titles), Some(&library), preset(StatsRange::All), fixed_now());

        let expected: Vec<(&str, u64)> =
            genres.iter().take(10).zip((3..=12).rev()).map(|(g, n)| (g.as_str(), n)).collect();
        assert_eq!(genre_counts(&s), expected);
    }

    #[test]
    fn genres_say_what_share_of_the_periods_plays_they_cover() {
        let library = LibraryIndex::new(
            vec![
                tagged("s1", "Anthem", &["Rock", "Pop"]),
                tagged("s2", "Riff", &["Rock"]),
                tagged("s3", "Untagged", &[]),
            ],
            Vec::new(),
        );
        let mut scrobbles = plays_of(&["Anthem", "Anthem", "Riff", "Untagged", "Unreleased"]);
        // Before the week: not part of the share.
        scrobbles.insert(0, scrobble(fixed_now().timestamp() - 30 * 86_400, "Artist", "Anthem"));
        let s = compute(&scrobbles, Some(&library), preset(StatsRange::Week), fixed_now());

        // Three of the week's five plays are of a Song with a genre.
        assert_eq!(s.genres.expect("genres with an index").coverage, 0.6);
    }

    /// Library Songs by "Artist", newest first, and one without a year.
    fn dated_library() -> LibraryIndex {
        let years = [
            ("Now", Some(2003)),
            ("Middle", Some(1985)),
            ("Then", Some(1979)),
            ("Before", Some(1971)),
            ("Undated", None),
        ];
        let songs = years
            .into_iter()
            .map(|(title, year)| {
                let mut song = library::song(title, "Artist", title);
                song.year = year;
                song
            })
            .collect();
        LibraryIndex::new(songs, Vec::new())
    }

    fn decade_counts(s: &StatsSummary) -> Vec<(i32, u64)> {
        let decades = s.decades.as_ref().expect("decades with an index");
        decades.entries.iter().map(|d| (d.decade, d.count)).collect()
    }

    #[test]
    fn decades_run_oldest_first_and_leave_out_songs_without_a_year() {
        let library = dated_library();
        let scrobbles = plays_of(&["Now", "Middle", "Then", "Before", "Undated"]);
        let s = compute(&scrobbles, Some(&library), preset(StatsRange::All), fixed_now());

        assert_eq!(decade_counts(&s), [(1970, 2), (1980, 1), (2000, 1)]);
    }

    #[test]
    fn decades_say_what_share_of_the_periods_plays_they_cover() {
        let library = dated_library();
        let mut scrobbles = plays_of(&["Now", "Then", "Then", "Undated", "Unreleased"]);
        // Before the week: not part of the share.
        scrobbles.insert(0, scrobble(fixed_now().timestamp() - 30 * 86_400, "Artist", "Now"));
        let s = compute(&scrobbles, Some(&library), preset(StatsRange::Week), fixed_now());

        // Three of the week's five plays are of a Song with a year.
        assert_eq!(s.decades.expect("decades with an index").coverage, 0.6);
    }

    #[test]
    fn genres_and_decades_wait_for_the_index() {
        let scrobbles = plays_of(&["Now", "Then"]);
        let s = compute(&scrobbles, None, preset(StatsRange::All), fixed_now());

        assert!(s.genres.is_none());
        assert!(s.decades.is_none());
    }

    /// `(name, id, cover art)` of each artist New to you lists.
    fn first_heard(discovery: &Discovery) -> Vec<(&str, Option<&str>, Option<&str>)> {
        let first = &discovery.first_artists;
        first.iter().map(|a| (a.name.as_str(), a.id.as_deref(), a.cover_art.as_deref())).collect()
    }

    #[test]
    fn new_to_you_groups_by_library_match() {
        let songs = vec![
            library::song("s1", "Artist", "Song - 2011 Remaster"),
            library::song("s2", "Artist", "Duet"),
        ];
        let artists = vec![library::artist("a", "Artist"), library::artist("n", "Newcomer")];
        let library = LibraryIndex::new(songs, artists);
        let now = fixed_now().timestamp();
        let day = 86_400;
        let scrobbles = vec![
            // Known long before the week.
            scrobble(now - 300 * day, "Artist", "Song"),
            // Neither a featured credit nor a remaster makes them new.
            scrobble(now - 2 * day, "Artist feat. Guest", "Duet"),
            scrobble(now - day, "Artist", "Song - 2011 Remaster"),
            scrobble(now - 3_600, "Newcomer", "Debut"),
        ];
        let s = compute(&scrobbles, Some(&library), preset(StatsRange::Week), fixed_now());

        let week = s.discovery.unwrap();
        assert_eq!(week.new_artists, 1);
        // Duet and Debut: the remaster is the Song already known.
        assert_eq!(week.new_songs, 2);
        assert_eq!(first_heard(&week), [("Newcomer", Some("n"), Some("ar-n"))]);
    }

    #[test]
    fn new_to_you_groups_unmatched_scrobbles_by_name() {
        let library = merge_library();
        let now = fixed_now().timestamp();
        let day = 86_400;
        let scrobbles = vec![
            scrobble(now - 300 * day, "Stranger", "Old"),
            scrobble(now - 2 * day, "Stranger", "New"),
            scrobble(now - day, "Visitor", "Hello"),
            scrobble(now - 3_600, "Visitor", "Hello"),
            // Another credit is another name.
            scrobble(now - 60, "Stranger feat. Visitor", "Together"),
        ];
        let s = compute(&scrobbles, Some(&library), preset(StatsRange::Week), fixed_now());

        let week = s.discovery.unwrap();
        assert_eq!(week.new_artists, 2);
        assert_eq!(week.new_songs, 3);
        assert_eq!(
            first_heard(&week),
            [("Visitor", None, None), ("Stranger feat. Visitor", None, None)]
        );
        assert_eq!(week.first_artists[0].plays, 2);
    }

    #[test]
    fn on_this_day_groups_by_library_match() {
        let library = merge_library();
        let year_ago = same_day_years_ago(fixed_now(), 1);
        let scrobbles = vec![
            scrobble(year_ago, "Artist", "Song"),
            scrobble(year_ago + 60, "Artist", "Song - 2011 Remaster"),
            scrobble(year_ago + 120, "Stranger", "Unknown"),
        ];
        let s = compute(&scrobbles, Some(&library), preset(StatsRange::Day), fixed_now());

        let songs: Vec<(&str, Option<&str>, Option<&str>, u64)> = s.on_this_day[0]
            .songs
            .iter()
            .map(|e| (e.name.as_str(), e.id.as_deref(), e.cover_art.as_deref(), e.count))
            .collect();
        assert_eq!(
            songs,
            [("Song - 2011 Remaster", Some("s1"), Some("al-al1"), 2), ("Unknown", None, None, 1)]
        );
    }

    #[test]
    fn recent_scrobbles_carry_their_library_match() {
        let library = merge_library();
        let now = fixed_now().timestamp();
        let scrobbles = vec![
            on_album(scrobble(now - 120, "Artist", "Song"), "Album"),
            scrobble(now - 60, "Stranger", "Unknown"),
        ];
        let s = compute(&scrobbles, Some(&library), preset(StatsRange::Day), fixed_now());

        let recent: Vec<_> = s
            .recent
            .iter()
            .map(|p| {
                let (id, art) = (p.id.as_deref(), p.cover_art.as_deref());
                (p.name.as_str(), p.artist.as_str(), p.album.as_deref(), id, art)
            })
            .collect();
        assert_eq!(
            recent,
            [
                ("Unknown", "Stranger", Some("Stranger album"), None, None),
                ("Song - 2011 Remaster", "Artist", Some("Album"), Some("s1"), Some("al-al1")),
            ]
        );
    }
}
