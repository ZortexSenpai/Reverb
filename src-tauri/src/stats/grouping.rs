//! How Scrobbles add up into artist, album and Song entries: by their
//! Library match when they have one, else by name as scrobbled. Every list
//! that ranks or picks out entries groups through here, so the page never
//! disagrees with itself.

use std::collections::HashMap;

use super::compute::{LibraryMatch, TopEntry};
use super::store::StatScrobble;
use crate::library_index::index::{LibraryArtist, LibraryIndex, LibrarySong};

/// An artist, album or Song entry a Scrobble counts toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Group<'a> {
    pub name: &'a str,
    /// The artist, for album and Song entries.
    pub artist: Option<&'a str>,
    /// Absent without a Library match.
    pub matched: Option<MatchRef<'a>>,
}

/// A [`LibraryMatch`] as the index holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchRef<'a> {
    pub id: &'a str,
    pub cover_art: Option<&'a str>,
}

impl From<MatchRef<'_>> for LibraryMatch {
    fn from(matched: MatchRef<'_>) -> Self {
        Self { id: matched.id.to_owned(), cover_art: matched.cover_art.map(str::to_owned) }
    }
}

/// What a [`Group`] is counted under: its library item, else its names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroupKey<'a> {
    Library(&'a str),
    Name(&'a str, Option<&'a str>),
}

impl<'a> Group<'a> {
    /// An entry without a Library match.
    pub fn by_name(name: &'a str, artist: Option<&'a str>) -> Self {
        Self { name, artist, matched: None }
    }

    pub fn key(&self) -> GroupKey<'a> {
        match self.matched {
            Some(matched) => GroupKey::Library(matched.id),
            None => GroupKey::Name(self.name, self.artist),
        }
    }
}

impl TopEntry {
    /// What the entry was counted under, to pick out its Scrobbles again.
    pub fn key(&self) -> GroupKey<'_> {
        match &self.matched {
            Some(matched) => GroupKey::Library(&matched.id),
            None => GroupKey::Name(&self.name, self.artist.as_deref()),
        }
    }
}

/// Finds the entries Scrobbles count toward, looking each artist, and each
/// artist and title, up in the library once.
pub struct Grouper<'a> {
    library: Option<&'a LibraryIndex>,
    artists: HashMap<&'a str, Option<&'a LibraryArtist>>,
    songs: HashMap<(&'a str, &'a str), Option<LibrarySong<'a>>>,
}

impl<'a> Grouper<'a> {
    /// Without a `library`, everything groups by name.
    pub fn new(library: Option<&'a LibraryIndex>) -> Self {
        Self { library, artists: HashMap::new(), songs: HashMap::new() }
    }

    /// The library Artist the credit matches, else the credit itself.
    pub fn artist(&mut self, scrobble: &'a StatScrobble) -> Group<'a> {
        let library = self.library;
        let credit = scrobble.artist.as_str();
        let matched = *self.artists.entry(credit).or_insert_with(|| library?.find_artist(credit));
        match matched {
            Some(artist) => Group {
                name: &artist.name,
                artist: None,
                matched: Some(MatchRef { id: &artist.id, cover_art: artist.cover_art.as_deref() }),
            },
            None => Group::by_name(credit, None),
        }
    }

    /// The library Song the Scrobble matches, else its title and artist.
    pub fn song(&mut self, scrobble: &'a StatScrobble) -> Group<'a> {
        match self.library_song(scrobble) {
            Some(song) => Group {
                name: song.title(),
                artist: Some(song.artist().unwrap_or(&scrobble.artist)),
                matched: Some(MatchRef { id: song.id(), cover_art: song.cover_art() }),
            },
            None => Group::by_name(&scrobble.track, Some(&scrobble.artist)),
        }
    }

    /// The library Album of the Song the Scrobble matches, else the
    /// scrobbled album and artist. None when there's neither.
    pub fn album(&mut self, scrobble: &'a StatScrobble) -> Option<Group<'a>> {
        let Some(album) = self.library_song(scrobble).and_then(|song| song.album()) else {
            let name = scrobble.album.as_deref()?;
            return Some(Group::by_name(name, Some(&scrobble.artist)));
        };
        Some(Group {
            name: album.name,
            artist: Some(self.artist(scrobble).name),
            matched: Some(MatchRef { id: album.id, cover_art: album.cover_art }),
        })
    }

    /// The Scrobble's Library match, as a Song: for what only the library
    /// knows about it, like its length, genres and year.
    pub fn library_song(&mut self, scrobble: &'a StatScrobble) -> Option<LibrarySong<'a>> {
        let library = self.library?;
        let (artist, title) = (scrobble.artist.as_str(), scrobble.track.as_str());
        *self.songs.entry((artist, title)).or_insert_with(|| library.find_song(artist, title))
    }
}

/// Scrobbles counted per entry.
#[derive(Default)]
pub struct Tally<'a> {
    entries: HashMap<GroupKey<'a>, (Group<'a>, u64)>,
}

impl<'a> Tally<'a> {
    pub fn add(&mut self, group: Group<'a>) {
        let (entry, count) = self.entries.entry(group.key()).or_insert((group, 0));
        // An album whose Scrobbles name different artists, like a
        // compilation, is by none of them in particular.
        if entry.artist != group.artist {
            entry.artist = None;
        }
        *count += 1;
    }

    /// How many different entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Every entry, most played first, then by name and artist.
    pub fn ranked(self) -> Vec<TopEntry> {
        let mut entries: Vec<(Group, u64)> = self.entries.into_values().collect();
        entries.sort_by(|(a, a_count), (b, b_count)| {
            b_count.cmp(a_count).then_with(|| (a.name, a.artist).cmp(&(b.name, b.artist)))
        });
        entries
            .into_iter()
            .map(|(group, count)| TopEntry {
                name: group.name.to_owned(),
                artist: group.artist.map(str::to_owned),
                matched: group.matched.map(Into::into),
                count,
                movement: None,
            })
            .collect()
    }
}
