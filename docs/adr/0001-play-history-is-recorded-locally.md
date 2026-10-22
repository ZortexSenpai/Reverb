---
status: accepted
---

# Play History is recorded locally, not read from Navidrome

Reverb records each Play itself and keeps the Play History on this computer, scoped to the signed-in account, instead of reading Navidrome's scrobble history (`GET /api/scrobble`, Navidrome 0.64.0+). Navidrome's history would include plays from other clients, but it only receives Reverb's own Plays while scrobbling to the server is on and "Last.fm only" is off, needs a server released in September 2026, costs a round trip plus a lookup per song, and lives in Navidrome's unversioned internal API. Plain Subsonic and OpenSubsonic have no song-level play history at all.

## Consequences

Play History only covers Reverb on this computer. If plays from other devices are wanted later, they get merged in on top of the local log rather than replacing it.
