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

### Stats

**Library match**:
The library Song, Album or Artist a Scrobble is recognised as, matched by name. A Scrobble of music not in the library has none.
_Avoid_: Lookup, mapping

**Period**:
The span of time the stats page summarises: the last day, week, month or year, a custom date range, or all time.
_Avoid_: Range, window

**Previous period**:
The equal-length span immediately before a Period, which its numbers and ranks are compared against. All time has none.

**Streak**:
A run of consecutive days with at least one Scrobble. The current Streak ends today, or yesterday while today has none yet.

**Obsession**:
The Song with the most Scrobbles on a single day within a Period.
_Avoid_: On repeat (the name of a mix)
