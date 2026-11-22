//! Sending Plays straight to a listening service, leaving the Subsonic server
//! out of it. Which service is a preference; this module resolves it to a
//! [`Target`] carrying the credentials, and sends through whichever API that
//! service speaks.

use super::store::Account;
use super::{
    lastfm, listenbrainz, listenbrainz_token_key, Role, Source, LASTFM_SECRET_KEY,
    LASTFM_SESSION_KEY,
};
use crate::state::AppState;

/// One play, as the services identify it. `track` and `artist` are the only
/// fields they require; a submission missing either is rejected outright.
#[derive(Debug, Clone)]
pub struct ScrobbleTrack {
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    /// Track length in seconds; omitted when the engine never learned it.
    pub duration_secs: Option<u64>,
}

/// The artist name to submit for a play.
///
/// Last.fm and ListenBrainz identify a play by artist *name*, so a joined
/// credit like "A feat. B" usually matches no artist there and the play is
/// filed under nothing useful. `first_artist_only` trades the featured names
/// for a play that lands on the main artist — which is only possible where the
/// server enumerated the credit, so without a first artist the full string
/// still goes.
pub fn scrobble_artist(
    artist: Option<String>,
    first_artist: Option<String>,
    first_artist_only: bool,
) -> Option<String> {
    if !first_artist_only {
        return artist;
    }
    // A blank first artist would otherwise beat a perfectly good credit and
    // lose the scrobble entirely (`from_parts` drops empty artists).
    first_artist.filter(|name| !name.trim().is_empty()).or(artist)
}

impl ScrobbleTrack {
    /// Build a submission from the engine's (all-optional) track metadata.
    /// Returns `None` when artist or title is missing — the service would only
    /// reject it, and a play it cannot attribute is not worth a round trip.
    pub fn from_parts(
        artist: Option<String>,
        title: Option<String>,
        album: Option<String>,
        duration_ms: u64,
    ) -> Option<Self> {
        let artist = artist.filter(|s| !s.trim().is_empty())?;
        let track = title.filter(|s| !s.trim().is_empty())?;
        Some(Self {
            artist,
            track,
            album: album.filter(|s| !s.trim().is_empty()),
            duration_secs: (duration_ms > 0).then_some(duration_ms / 1000),
        })
    }
}

/// A service ready to take submissions, with everything needed to send one.
pub enum Target {
    Lastfm { api_key: String, secret: String, session_key: String },
    Listenbrainz { api_root: String, token: String },
}

impl Target {
    pub fn source(&self) -> Source {
        match self {
            Target::Lastfm { .. } => Source::Lastfm,
            Target::Listenbrainz { .. } => Source::Listenbrainz,
        }
    }

    /// Announce the track as now playing. Both services expire the entry on
    /// their own after roughly the track's length.
    pub async fn now_playing(
        &self,
        client: &reqwest::Client,
        track: &ScrobbleTrack,
    ) -> Result<(), String> {
        match self {
            Target::Lastfm { api_key, secret, session_key } => {
                lastfm::update_now_playing(client, api_key, secret, session_key, track).await
            }
            Target::Listenbrainz { api_root, token } => {
                listenbrainz::submit(client, api_root, token, track, None).await
            }
        }
    }

    /// Submit a finished play. `started_at` is unix seconds at which the track
    /// *started*, which is what both services record as the play time.
    pub async fn scrobble(
        &self,
        client: &reqwest::Client,
        track: &ScrobbleTrack,
        started_at: i64,
    ) -> Result<(), String> {
        match self {
            Target::Lastfm { api_key, secret, session_key } => {
                lastfm::scrobble(client, api_key, secret, session_key, track, started_at).await
            }
            Target::Listenbrainz { api_root, token } => {
                listenbrainz::submit(client, api_root, token, track, Some(started_at)).await
            }
        }
    }
}

/// The scrobbling account as a submission target, or `None` while there is
/// none or its secrets are missing from the keychain.
pub fn scrobble_target(state: &AppState) -> Option<Target> {
    match state.stats.config().scrobble? {
        Account::Lastfm { api_key, .. } => Some(Target::Lastfm {
            api_key,
            secret: state.keychain.retrieve_secret(LASTFM_SECRET_KEY)?,
            session_key: state.keychain.retrieve_secret(LASTFM_SESSION_KEY)?,
        }),
        Account::Listenbrainz { api_url, .. } => Some(Target::Listenbrainz {
            api_root: api_url.unwrap_or_else(|| listenbrainz::DEFAULT_API_ROOT.to_owned()),
            token: state.keychain.retrieve_secret(listenbrainz_token_key(Role::Scrobble))?,
        }),
    }
}

/// Resolve the direct-scrobbling target, or `None` to fall back to the
/// Subsonic server. Deliberately strict: the mode applies only when the
/// preference is on *and* the scrobbling account is fully set up. A
/// half-configured account would otherwise route plays away from Navidrome
/// and then drop them, which is worse than ignoring the preference.
pub fn direct_target(state: &AppState) -> Option<Target> {
    if !state.settings.get().prefs.direct_scrobble {
        return None;
    }
    scrobble_target(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrobble_artist_sends_the_whole_credit_by_default() {
        let artist = || Some("A feat. B".to_owned());
        let first = || Some("A".to_owned());
        assert_eq!(scrobble_artist(artist(), first(), false).as_deref(), Some("A feat. B"));
    }

    #[test]
    fn scrobble_artist_sends_only_the_first_when_asked() {
        let artist = || Some("A feat. B".to_owned());
        assert_eq!(
            scrobble_artist(artist(), Some("A".to_owned()), true).as_deref(),
            Some("A")
        );
    }

    #[test]
    fn scrobble_artist_falls_back_when_the_first_artist_is_unknown() {
        // Servers without the OpenSubsonic artists extension send the credit as
        // one opaque string: there is nothing to take the first artist from, and
        // guessing at separators would split "Simon & Garfunkel".
        let artist = || Some("Simon & Garfunkel".to_owned());
        assert_eq!(
            scrobble_artist(artist(), None, true).as_deref(),
            Some("Simon & Garfunkel")
        );
        // A blank one must not win, or the play is dropped rather than filed.
        assert_eq!(
            scrobble_artist(artist(), Some("  ".to_owned()), true).as_deref(),
            Some("Simon & Garfunkel")
        );
        // Nothing at all stays nothing, whichever way the preference is set.
        assert_eq!(scrobble_artist(None, None, true), None);
        assert_eq!(scrobble_artist(None, None, false), None);
    }

    #[test]
    fn submissions_need_an_artist_and_a_title() {
        assert!(ScrobbleTrack::from_parts(None, Some("T".into()), None, 0).is_none());
        assert!(ScrobbleTrack::from_parts(Some("A".into()), None, None, 0).is_none());
        // Whitespace-only metadata is as unusable as none at all.
        assert!(ScrobbleTrack::from_parts(Some("  ".into()), Some("T".into()), None, 0).is_none());
    }

    #[test]
    fn submissions_carry_album_and_duration_when_known() {
        let track =
            ScrobbleTrack::from_parts(Some("A".into()), Some("T".into()), Some("Al".into()), 245_000)
                .expect("artist and title are present");
        assert_eq!(track.album.as_deref(), Some("Al"));
        assert_eq!(track.duration_secs, Some(245));
        // An unknown duration is omitted rather than sent as zero.
        let unknown = ScrobbleTrack::from_parts(Some("A".into()), Some("T".into()), None, 0)
            .expect("artist and title are present");
        assert_eq!(unknown.duration_secs, None);
        assert_eq!(unknown.album, None);
    }
}
