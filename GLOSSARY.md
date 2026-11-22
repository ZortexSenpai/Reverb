# Reverb

A desktop player for a Navidrome music server, built around exclusive output and a parametric EQ.

## Language

### Library

**Song**:
A single playable item in the Navidrome library.
_Avoid_: Track

### Playback

**Queue**:
The ordered Songs Reverb plays through: the ones already passed, the current one, and the ones up next. The Songs already passed are not the Play History.
_Avoid_: Playlist, history

**Play**:
One listening of a Song in Reverb on this computer, counted once playback passes the "Mark as played after" point. Every listening is its own Play, repeats included.
_Avoid_: Listen, stream

**Play History**:
The newest Plays of the signed-in account on this computer, newest first. Shown as "Recently played".
_Avoid_: Recently listened, listening history, history

**Scrobble**:
A listening as recorded by Last.fm, ListenBrainz or Navidrome, which may come from any device or client. A Play becomes a Scrobble only if it is sent there.
_Avoid_: Play (for plays recorded elsewhere), listen (ListenBrainz's own word for the same thing)

**Scrobbling account**:
The Last.fm or ListenBrainz account Reverb sends Plays to itself. A ListenBrainz account can live on listenbrainz.org or on another server that speaks its API.
_Avoid_: Scrobble service, integration

**Direct scrobbling**:
Sending Plays straight to the Scrobbling account instead of to Navidrome, which then hears nothing of them: no play count, no now-playing entry.
_Avoid_: Last.fm only

### Stats

**Stats source**:
The account whose Scrobbles Reverb imports for the Stats page and the mixes: the Scrobbling account by default, or a separate Last.fm or ListenBrainz account. There is one at a time, and its imported history goes when the Stats source becomes a different account.
_Avoid_: Integration, account (on its own)
