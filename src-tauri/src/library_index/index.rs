//! The Library index itself: the account's library Songs and Artists,
//! indexed by name so Scrobbles find their Library match.

// Mixes only match Songs; the rest is for the stats aggregation.
#![cfg_attr(not(test), expect(dead_code, reason = "the stats aggregation reads the lookups"))]

use std::collections::{HashMap, HashSet};

use reverb_core::subsonic::models::{ArtistId3, Child};

use super::matcher::{credit_keys, credited_artists, fold, loose_title, name_key};

/// One account's library, ready for matching. Build it with
/// [`LibraryIndex::new`]; tests do so from hand-built Songs.
pub struct LibraryIndex {
    /// Each library Song once, in listing order.
    songs: Vec<Child>,
    /// Folded artist and title → song, by exact title and by loose title.
    exact: HashMap<String, usize>,
    loose: HashMap<String, usize>,
    /// The artist listing's Artists, then any others credited on Songs.
    artists: Vec<LibraryArtist>,
    artist_ids: HashMap<String, usize>,
    /// Folded name → artist. Where names collide, the first one wins.
    artist_names: HashMap<String, usize>,
}

/// A library Artist. Only those in the artist listing have art.
#[derive(Debug, Clone)]
pub struct LibraryArtist {
    pub id: String,
    pub name: String,
    pub cover_art: Option<String>,
}

impl LibraryIndex {
    /// Index `songs` (the song listing) and `artists` (the artist listing).
    pub fn new(songs: Vec<Child>, artists: Vec<ArtistId3>) -> Self {
        let mut seen = HashSet::new();
        // Library pages can overlap where the listing's sort order has ties.
        let songs: Vec<Child> =
            songs.into_iter().filter(|s| !s.is_dir && seen.insert(s.id.clone())).collect();

        let mut index = Self {
            songs: Vec::new(),
            exact: HashMap::new(),
            loose: HashMap::new(),
            artists: Vec::new(),
            artist_ids: HashMap::new(),
            artist_names: HashMap::new(),
        };
        for a in artists {
            index.add_artist(LibraryArtist { id: a.id, name: a.name, cover_art: a.cover_art });
        }
        // Featured artists often have no album of their own, so the artist
        // listing leaves them out.
        for (name, id) in songs.iter().flat_map(credited_artists) {
            if let Some(id) = id {
                index.add_artist(LibraryArtist { id, name, cover_art: None });
            }
        }

        // Title indexes under every way a song's artist can be written. Where
        // titles collide, the most played song wins.
        let plays = |i: usize| songs[i].play_count.unwrap_or(0);
        let (exact, loose) = (&mut index.exact, &mut index.loose);
        for (i, song) in songs.iter().enumerate() {
            let mut keys: Vec<String> = Vec::new();
            for (name, _) in credited_artists(song) {
                let key = fold(&name);
                if !key.is_empty() && !keys.contains(&key) {
                    keys.push(key);
                }
            }
            for credit in [&song.artist, &song.display_artist].into_iter().flatten() {
                let key = fold(credit);
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
            let titles = [fold(&song.title), loose_title(&song.title)];
            for artist in &keys {
                for (by_title, title) in [(&mut *exact, &titles[0]), (&mut *loose, &titles[1])] {
                    let slot = by_title.entry(name_key(artist, title)).or_insert(i);
                    if plays(i) > plays(*slot) {
                        *slot = i;
                    }
                }
            }
        }
        index.songs = songs;
        index
    }

    /// Add `artist` unless its id is already in.
    fn add_artist(&mut self, artist: LibraryArtist) {
        if self.artist_ids.contains_key(&artist.id) {
            return;
        }
        let at = self.artists.len();
        self.artist_ids.insert(artist.id.clone(), at);
        self.artist_names.entry(fold(&artist.name)).or_insert(at);
        self.artists.push(artist);
    }

    /// The library Artist a Scrobble credited to `artist` matches: the
    /// whole credit first, then the first of its artists in the library.
    pub fn find_artist(&self, artist: &str) -> Option<&LibraryArtist> {
        let at = credit_keys(artist).iter().find_map(|key| self.artist_names.get(key).copied())?;
        Some(&self.artists[at])
    }

    /// Every library Song, each once.
    pub fn songs(&self) -> &[Child] {
        &self.songs
    }

    /// The position in [`Self::songs`] of the Song a Scrobble matches, given
    /// its artist credit as [`credit_keys`]: exact titles first, then without
    /// version notes, trying the whole credit before its parts.
    pub fn match_song(&self, artist_keys: &[String], title: &str) -> Option<usize> {
        let exact_title = fold(title);
        let found = artist_keys.iter().find_map(|a| self.exact.get(&name_key(a, &exact_title)));
        if let Some(&i) = found {
            return Some(i);
        }
        let loose_title = loose_title(title);
        artist_keys.iter().find_map(|a| self.loose.get(&name_key(a, &loose_title)).copied())
    }

    /// The Song at `at` in [`Self::songs`].
    pub fn song(&self, at: usize) -> LibrarySong<'_> {
        LibrarySong { index: self, song: &self.songs[at] }
    }

    /// The library Song a Scrobble of `title` by `artist` matches.
    pub fn find_song(&self, artist: &str, title: &str) -> Option<LibrarySong<'_>> {
        self.match_song(&credit_keys(artist), title).map(|at| self.song(at))
    }

    /// The library Album of the Song a Scrobble matches.
    pub fn find_album(&self, artist: &str, title: &str) -> Option<LibraryAlbum<'_>> {
        self.find_song(artist, title)?.album()
    }
}

/// A library Album, as one of its Songs knows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LibraryAlbum<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub cover_art: Option<&'a str>,
}

/// A library Song, with what stats read from it.
#[derive(Clone, Copy)]
pub struct LibrarySong<'a> {
    index: &'a LibraryIndex,
    song: &'a Child,
}

impl<'a> LibrarySong<'a> {
    pub fn id(&self) -> &'a str {
        &self.song.id
    }

    pub fn cover_art(&self) -> Option<&'a str> {
        self.song.cover_art.as_deref()
    }

    /// Length in seconds.
    pub fn duration(&self) -> Option<u32> {
        self.song.duration
    }

    /// Release year.
    pub fn year(&self) -> Option<i32> {
        self.song.year.filter(|&year| year > 0)
    }

    /// Every genre when the server sends the list, else the single one.
    pub fn genres(&self) -> Vec<&'a str> {
        let song = self.song;
        let names: Vec<&'a str> = if song.genres.is_empty() {
            song.genre.as_deref().into_iter().collect()
        } else {
            song.genres.iter().map(|g| g.name.as_str()).collect()
        };
        names.into_iter().filter(|g| !g.trim().is_empty()).collect()
    }

    /// Its album, when the listing names one. The cover is the Song's own.
    pub fn album(&self) -> Option<LibraryAlbum<'a>> {
        Some(LibraryAlbum {
            id: self.song.album_id.as_deref()?,
            name: self.song.album.as_deref().unwrap_or_default(),
            cover_art: self.cover_art(),
        })
    }

    /// Every credited Artist, in credit order.
    pub fn artists(&self) -> Vec<&'a LibraryArtist> {
        let index = self.index;
        credited_artists(self.song)
            .into_iter()
            .filter_map(|(_, id)| index.artist_ids.get(&id?).map(|&at| &index.artists[at]))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use reverb_core::subsonic::models::{ArtistRef, GenreRef};

    use super::*;
    use crate::library_index::testing::{artist, on_album, song};

    fn genres(names: &[&str]) -> Vec<GenreRef> {
        names.iter().map(|&name| GenreRef { name: name.into() }).collect()
    }

    #[test]
    fn a_scrobble_finds_its_song_with_what_stats_need() {
        let mut remaster = on_album(song("s1", "Artist", "Song - 2011 Remaster"), "al1", "Album");
        remaster.duration = Some(215);
        remaster.year = Some(1979);
        remaster.genre = Some("Rock".into());
        remaster.genres = genres(&["Rock", "Pop"]);
        let index = LibraryIndex::new(vec![remaster], Vec::new());

        let found = index.find_song("artist", "Song").expect("matched by loose title");
        assert_eq!(found.id(), "s1");
        assert_eq!(found.cover_art(), Some("al-al1"));
        assert_eq!(found.duration(), Some(215));
        assert_eq!(found.year(), Some(1979));
        assert_eq!(found.genres(), ["Rock", "Pop"]);
        assert!(index.find_song("Artist", "Other Song").is_none());
    }

    #[test]
    fn artists_match_the_whole_credit_before_its_parts() {
        let artists = vec![
            artist("sg", "Simon & Garfunkel"),
            artist("s", "Simon"),
            artist("a", "A"),
            artist("c", "C"),
            artist("d", "D"),
        ];
        let index = LibraryIndex::new(Vec::new(), artists);
        let found = |credit: &str| index.find_artist(credit).map(|a| (a.id.as_str(), a.cover_art.as_deref()));

        assert_eq!(found("simon & garfunkel"), Some(("sg", Some("ar-sg"))));
        assert_eq!(found("A feat. B"), Some(("a", Some("ar-a"))));
        assert_eq!(found("B, A"), Some(("a", Some("ar-a"))));
        // The first credited Artist in the library wins.
        assert_eq!(found("D & C"), Some(("d", Some("ar-d"))));
        assert_eq!(found("Nobody"), None);
    }

    #[test]
    fn songs_name_their_credited_artists() {
        let mut duet = song("s1", "Main feat. Guest", "Duet");
        duet.artists = vec![
            ArtistRef { id: "m".into(), name: "Main".into() },
            ArtistRef { id: "g".into(), name: "Guest".into() },
        ];
        let index = LibraryIndex::new(vec![duet], vec![artist("m", "Main")]);

        let credited: Vec<(&str, Option<&str>)> = index
            .find_song("Main", "Duet")
            .expect("matched")
            .artists()
            .into_iter()
            .map(|a| (a.id.as_str(), a.cover_art.as_deref()))
            .collect();
        // Only the artist listing has art.
        assert_eq!(credited, [("m", Some("ar-m")), ("g", None)]);
        let guest = index.find_artist("Guest").expect("credited on a Song");
        assert_eq!(guest.id, "g");
    }

    #[test]
    fn albums_are_found_through_their_songs() {
        let songs = vec![
            on_album(song("1", "X", "First"), "comp", "Hits"),
            on_album(song("2", "Y", "Second"), "comp", "Hits"),
            song("3", "Z", "Single"),
        ];
        let index = LibraryIndex::new(songs, Vec::new());
        let album = |artist: &str, title: &str| {
            index.find_album(artist, title).map(|a| (a.id, a.name, a.cover_art))
        };

        // A compilation is one album whoever its Songs are by.
        assert_eq!(album("X", "First"), Some(("comp", "Hits", Some("al-comp"))));
        assert_eq!(album("Y", "Second (Remastered)"), Some(("comp", "Hits", Some("al-comp"))));
        assert_eq!(album("Z", "Single"), None);
        assert_eq!(album("X", "Unknown"), None);
    }
}
