---
status: accepted
---

# Stats images come from the Library match, not Last.fm

The stats page is built from Last.fm Scrobbles, but its artist and album images come from the Navidrome library: each Scrobble is matched by name to a library Song, Album or Artist, and the art is served through the same cover-art proxy as the rest of the app. Last.fm cannot supply artist images (since 2019 its API returns the same blank star for every artist), and capturing its album art would mean re-importing the whole history. The Library match also makes entries clickable and enables the stats only the library knows: listening time, genres and release decades.

## Consequences

Music that is not in the library shows a placeholder, so how good the page looks depends on how well names match. Matching every Scrobble needs a library index kept on disk, which the mixes share instead of fetching the library on their own. Last.fm album art for unmatched albums could be layered on later without undoing this.
