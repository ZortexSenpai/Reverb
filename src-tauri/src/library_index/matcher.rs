//! Matching by name. Scrobbles know their Song only as artist and title
//! text, so both sides are folded into keys that meet despite the case,
//! punctuation and version notes scrobblers and taggers disagree on.

use reverb_core::subsonic::models::Child;

/// Lowercase alphanumerics only, so "AC/DC" and "ac dc" meet as "acdc". Text
/// made only of punctuation keeps its lowercase form rather than vanish.
pub fn fold(s: &str) -> String {
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
pub fn loose_title(title: &str) -> String {
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
pub fn credit_keys(credit: &str) -> Vec<String> {
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
pub fn credited_artists(song: &Child) -> Vec<(String, Option<String>)> {
    if !song.artists.is_empty() {
        return song.artists.iter().map(|a| (a.name.clone(), Some(a.id.clone()))).collect();
    }
    song.artist.iter().map(|name| (name.clone(), song.artist_id.clone())).collect()
}

pub fn name_key(artist: &str, title: &str) -> String {
    format!("{artist}\u{1}{title}")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
