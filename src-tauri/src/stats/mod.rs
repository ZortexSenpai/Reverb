pub mod compute;
pub mod lastfm;
pub mod listenbrainz;
pub mod scrobble;
pub mod store;

use serde::{Deserialize, Serialize};

/// A listening service Reverb can send Plays to and import Scrobbles from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Lastfm,
    Listenbrainz,
}

impl Source {
    /// The wire and file name ("lastfm", "listenbrainz").
    pub fn id(self) -> &'static str {
        match self {
            Source::Lastfm => "lastfm",
            Source::Listenbrainz => "listenbrainz",
        }
    }

    /// The service's own spelling, for messages shown to the user.
    pub fn label(self) -> &'static str {
        match self {
            Source::Lastfm => "Last.fm",
            Source::Listenbrainz => "ListenBrainz",
        }
    }
}

/// What an account is connected for. Each role has an account of its own, so
/// plays can go to one service (or server) while the Stats page reads another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Takes Plays when direct scrobbling is on.
    Scrobble,
    /// Supplies the history for the Stats page and the mixes.
    Stats,
}

/// Keychain entry names for the accounts' secrets. The scrobbling ones keep
/// the names they had when Last.fm was the only service, so connections made
/// back then still find their session.
pub const LASTFM_SECRET_KEY: &str = "lastfm-shared-secret";
pub const LASTFM_SESSION_KEY: &str = "lastfm-session-key";
const SCROBBLE_LISTENBRAINZ_TOKEN_KEY: &str = "listenbrainz-token";
const STATS_LISTENBRAINZ_TOKEN_KEY: &str = "stats-listenbrainz-token";

/// Where a role's ListenBrainz token is kept. Separate per role, since the two
/// can be different accounts on different servers. (Last.fm's import needs no
/// secret, so only the scrobbling role keeps Last.fm credentials.)
pub fn listenbrainz_token_key(role: Role) -> &'static str {
    match role {
        Role::Scrobble => SCROBBLE_LISTENBRAINZ_TOKEN_KEY,
        Role::Stats => STATS_LISTENBRAINZ_TOKEN_KEY,
    }
}
