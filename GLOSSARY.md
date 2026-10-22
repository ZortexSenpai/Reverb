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
A listening as recorded by Last.fm or Navidrome, which may come from any device or client. A Play becomes a Scrobble only if it is sent there.
_Avoid_: Play (for plays recorded elsewhere)
