//! Aggregation for the stats page: top lists, clock/weekday distributions,
//! and a play-count timeline, computed over the imported scrobbles.

use std::collections::HashMap;

use chrono::{Datelike, Duration, Local, Months, NaiveDate, NaiveDateTime, TimeZone, Timelike};
use serde::Serialize;

use super::store::StatScrobble;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopEntry {
    pub name: String,
    /// Artist for track/album entries.
    pub artist: Option<String>,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelinePoint {
    pub label: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentPlay {
    pub ts: i64,
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsSummary {
    pub total_plays: u64,
    pub unique_artists: u64,
    pub unique_tracks: u64,
    pub unique_albums: u64,
    pub first_ts: Option<i64>,
    pub last_ts: Option<i64>,
    pub plays_per_day: f64,
    pub top_artists: Vec<TopEntry>,
    pub top_tracks: Vec<TopEntry>,
    pub top_albums: Vec<TopEntry>,
    /// Local-time hour of day, 24 buckets.
    pub by_hour: Vec<u64>,
    /// Monday-first weekday, 7 buckets.
    pub by_weekday: Vec<u64>,
    pub timeline: Vec<TimelinePoint>,
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
    pub new_tracks: u64,
    /// Artists first heard in the window, most-played first.
    pub first_artists: Vec<FirstHeard>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstHeard {
    pub name: String,
    pub ts: i64,
    /// Plays since, within the window.
    pub plays: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnThisDay {
    pub year: i32,
    pub plays: u64,
    pub tracks: Vec<TopEntry>,
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
const TOP_SESSIONS: usize = 5;
const ON_THIS_DAY_YEARS: usize = 3;
const ON_THIS_DAY_TRACKS: usize = 3;

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

fn top_n(counts: HashMap<(String, Option<String>), u64>) -> Vec<TopEntry> {
    let mut entries: Vec<TopEntry> = counts
        .into_iter()
        .map(|((name, artist), count)| TopEntry { name, artist, count })
        .collect();
    entries.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    entries.truncate(TOP_N);
    entries
}

pub fn compute(scrobbles: &[StatScrobble], query: StatsQuery) -> StatsSummary {
    let now = Local::now();
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

    let mut artists: HashMap<(String, Option<String>), u64> = HashMap::new();
    let mut tracks: HashMap<(String, Option<String>), u64> = HashMap::new();
    let mut albums: HashMap<(String, Option<String>), u64> = HashMap::new();
    let mut by_hour = vec![0u64; 24];
    let mut by_weekday = vec![0u64; 7];
    let mut buckets: HashMap<String, u64> = HashMap::new();
    let mut day_counts: HashMap<i32, u64> = HashMap::new();

    for s in &filtered {
        *artists.entry((s.artist.clone(), None)).or_default() += 1;
        *tracks.entry((s.track.clone(), Some(s.artist.clone()))).or_default() += 1;
        if let Some(album) = &s.album {
            *albums.entry((album.clone(), Some(s.artist.clone()))).or_default() += 1;
        }
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
        .map(|s| RecentPlay {
            ts: s.ts,
            artist: s.artist.clone(),
            track: s.track.clone(),
            album: s.album.clone(),
        })
        .collect();

    // Dense timeline (include empty buckets so gaps are visible). `end` is
    // exclusive, so the last point is the bucket holding its final second.
    let mut timeline = Vec::new();
    if let (Some(first), Some(last)) = (local_naive(start), local_naive(end - 1)) {
        let mut cursor = bucket.floor(first);
        while cursor <= last && timeline.len() < MAX_TIMELINE_POINTS {
            timeline.push(TimelinePoint {
                count: buckets.get(&bucket.key(cursor)).copied().unwrap_or(0),
                label: bucket.label(cursor),
            });
            cursor = bucket.next(cursor);
        }
    }

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
    let discovery = from.map(|window_start| discover(scrobbles, window_start, to, &artists));
    let on_this_day = on_this_day(scrobbles, now.date_naive());

    let first_ts = filtered.first().map(|s| s.ts);
    let last_ts = filtered.last().map(|s| s.ts);
    let span_days = match (first_ts, last_ts) {
        (Some(a), Some(b)) if b > a => ((b - a) as f64 / 86_400.0).max(1.0),
        _ => 1.0,
    };

    StatsSummary {
        total_plays: filtered.len() as u64,
        unique_artists: artists.len() as u64,
        unique_tracks: tracks.len() as u64,
        unique_albums: albums.len() as u64,
        first_ts,
        last_ts,
        plays_per_day: filtered.len() as f64 / span_days,
        top_artists: top_n(artists),
        top_tracks: top_n(tracks),
        top_albums: top_n(albums),
        by_hour,
        by_weekday,
        timeline,
        recent,
        busiest_day,
        longest_streak_days,
        daily,
        sessions,
        discovery,
        on_this_day,
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

/// Artists and tracks whose *first ever* play falls inside the window.
fn discover(
    all: &[StatScrobble],
    from: i64,
    to: Option<i64>,
    window_artist_plays: &HashMap<(String, Option<String>), u64>,
) -> Discovery {
    let in_window = |ts: i64| ts >= from && to.is_none_or(|t| ts < t);
    let mut first_artist: HashMap<&str, i64> = HashMap::new();
    let mut first_track: HashMap<(&str, &str), i64> = HashMap::new();
    // Scrobbles are chronological, so the first sighting wins.
    for s in all {
        first_artist.entry(s.artist.as_str()).or_insert(s.ts);
        first_track.entry((s.artist.as_str(), s.track.as_str())).or_insert(s.ts);
    }

    let mut first_artists: Vec<FirstHeard> = first_artist
        .iter()
        .filter(|(_, ts)| in_window(**ts))
        .map(|(name, ts)| FirstHeard {
            name: (*name).to_owned(),
            ts: *ts,
            plays: window_artist_plays.get(&((*name).to_owned(), None)).copied().unwrap_or(0),
        })
        .collect();
    // Discoveries that stuck first: a one-off play says less about the period
    // than an artist picked up and kept.
    first_artists.sort_by(|a, b| b.plays.cmp(&a.plays).then_with(|| b.ts.cmp(&a.ts)));
    let new_artists = first_artists.len() as u64;
    first_artists.truncate(TOP_N);

    Discovery {
        new_artists,
        new_tracks: first_track.values().filter(|ts| in_window(**ts)).count() as u64,
        first_artists,
    }
}

/// One earlier year's plays on today's date, still being tallied.
#[derive(Default)]
struct DayInYear {
    plays: u64,
    tracks: HashMap<(String, Option<String>), u64>,
}

/// Plays from the same month/day in earlier years, most recent year first.
fn on_this_day(all: &[StatScrobble], today: NaiveDate) -> Vec<OnThisDay> {
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
        *entry.tracks.entry((s.track.clone(), Some(s.artist.clone()))).or_default() += 1;
    }

    let mut out: Vec<OnThisDay> = years
        .into_iter()
        .map(|(year, day)| {
            let mut tracks = top_n(day.tracks);
            tracks.truncate(ON_THIS_DAY_TRACKS);
            OnThisDay { year, plays: day.plays, tracks }
        })
        .collect();
    out.sort_by_key(|day| std::cmp::Reverse(day.year));
    out.truncate(ON_THIS_DAY_YEARS);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A preset range needs no explicit bounds.
    fn preset(range: StatsRange) -> StatsQuery {
        StatsQuery { range, from: None, to: None }
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
        let s = compute(&scrobbles, preset(StatsRange::All));
        assert_eq!(s.total_plays, 4);
        assert_eq!(s.unique_artists, 2);
        assert_eq!(s.unique_tracks, 3);
        assert_eq!(s.top_artists[0].name, "A");
        assert_eq!(s.top_artists[0].count, 3);
        assert_eq!(s.top_tracks[0].name, "one");
        assert_eq!(s.top_tracks[0].count, 2);
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
        let year = compute(&scrobbles, preset(StatsRange::Year));
        assert_eq!(year.total_plays, 1);
        let all = compute(&scrobbles, preset(StatsRange::All));
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

        let day = compute(&scrobbles, preset(StatsRange::Day));
        assert_eq!(day.total_plays, 1);
        // 24h window, hourly buckets: current hour plus the 24 before it.
        assert_eq!(day.timeline.len(), 25);
        assert_eq!(day.timeline.iter().map(|p| p.count).sum::<u64>(), 1);
        // A 10-minute-old play lands in this hour's bucket, or the one before
        // it when the hour has just turned over.
        let newest: u64 = day.timeline.iter().rev().take(2).map(|p| p.count).sum();
        assert_eq!(newest, 1);

        let week = compute(&scrobbles, preset(StatsRange::Week));
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
        let s = compute(&scrobbles, preset(StatsRange::Day)).sessions;
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
        assert_eq!(compute(&scrobbles, preset(StatsRange::Day)).sessions.count, 1);
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
        let week = compute(&scrobbles, preset(StatsRange::Week)).discovery.unwrap();
        assert_eq!(week.new_artists, 1);
        assert_eq!(week.new_tracks, 2);
        assert_eq!(week.first_artists.len(), 1);
        assert_eq!(week.first_artists[0].name, "New");
        // Both of the new artist's plays fall in the window.
        assert_eq!(week.first_artists[0].plays, 2);

        // A discovery that stuck outranks a more recent one-off.
        let mut with_oneoff = scrobbles.clone();
        with_oneoff.push(scrobble(now - 60, "Passing", "once"));
        let listed = compute(&with_oneoff, preset(StatsRange::Week)).discovery.unwrap().first_artists;
        let names: Vec<&str> = listed.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["New", "Passing"]);

        // Everything is "new" over all time, so the section is left out.
        assert!(compute(&scrobbles, preset(StatsRange::All)).discovery.is_none());
    }

    #[test]
    fn calendar_covers_long_windows_only() {
        let now = Local::now().timestamp();
        let scrobbles = vec![scrobble(now - 100, "A", "one")];
        assert!(compute(&scrobbles, preset(StatsRange::Week)).daily.is_empty());

        let month = compute(&scrobbles, preset(StatsRange::Month)).daily;
        assert_eq!(month.len(), 31, "30-day window spans 31 local dates at most");
        assert_eq!(month.iter().map(|d| d.count).sum::<u64>(), 1);

        // A decade is capped to the calendar's own window, not the range.
        let year = 365 * 86_400;
        let wide = StatsQuery {
            range: StatsRange::Custom,
            from: Some(now - 10 * year),
            to: Some(now),
        };
        assert!(compute(&scrobbles, wide).daily.len() <= CALENDAR_MAX_DAYS as usize + 1);
    }

    #[test]
    fn on_this_day_looks_back_by_year() {
        let now = Local::now();
        let scrobbles = vec![
            // Same month/day, one and two years back (52-week jumps drift, so
            // build the timestamps from real dates).
            scrobble(same_day_years_ago(now, 1), "A", "last year"),
            scrobble(same_day_years_ago(now, 2), "B", "two years ago"),
            scrobble(same_day_years_ago(now, 2), "B", "two years ago"),
            // Today never counts as history.
            scrobble(now.timestamp() - 60, "C", "today"),
        ];
        let days = on_this_day(&scrobbles, now.date_naive());
        assert_eq!(days.len(), 2);
        assert_eq!(days[0].year, now.year() - 1);
        assert_eq!(days[0].plays, 1);
        assert_eq!(days[1].plays, 2);
        assert_eq!(days[1].tracks[0].name, "two years ago");
        assert_eq!(days[1].tracks[0].count, 2);
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
        let s = compute(&scrobbles, query);
        assert_eq!(s.total_plays, 1);
        assert_eq!(s.top_tracks[0].name, "inside");
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
        assert_eq!(compute(&scrobbles, reversed).total_plays, 1);
    }

    #[test]
    fn wide_custom_window_falls_back_to_coarse_buckets() {
        let year = 365 * 86_400;
        let now = Local::now().timestamp();
        let scrobbles = vec![scrobble(now - year, "A", "one"), scrobble(now - 100, "B", "two")];
        let query =
            StatsQuery { range: StatsRange::Custom, from: Some(now - 10 * year), to: Some(now) };
        let s = compute(&scrobbles, query);
        assert_eq!(s.total_plays, 2);
        // Yearly buckets over a decade — 11 calendar years at most.
        assert!(s.timeline.len() <= 11, "got {} points", s.timeline.len());
        assert_eq!(s.timeline.iter().map(|p| p.count).sum::<u64>(), 2);
    }
}
