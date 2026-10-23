pub mod index;
pub mod matcher;

/// Library Songs and Artists built by hand, as the listings send them.
#[cfg(test)]
pub mod testing {
    use reverb_core::subsonic::models::{ArtistId3, Child};

    /// A library Song credited to `artist` (id `ar-<artist>`), with nothing
    /// else set; fill in any other field on the result.
    pub fn song(id: &str, artist: &str, title: &str) -> Child {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "title": title,
            "artist": artist,
            "artistId": format!("ar-{artist}"),
        }))
        .unwrap()
    }

    /// `song` on album `id`, with that album's cover, as Navidrome lists it.
    pub fn on_album(mut song: Child, id: &str, name: &str) -> Child {
        song.album_id = Some(id.to_owned());
        song.album = Some(name.to_owned());
        song.cover_art = Some(format!("al-{id}"));
        song
    }

    /// An artist as the artist listing sends it, with art.
    pub fn artist(id: &str, name: &str) -> ArtistId3 {
        ArtistId3 {
            id: id.to_owned(),
            name: name.to_owned(),
            cover_art: Some(format!("ar-{id}")),
            album_count: None,
            artist_image_url: None,
            starred: None,
        }
    }
}
